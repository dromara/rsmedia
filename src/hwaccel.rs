use crate::pixel::PixelFormat;
use crate::{imgutils, utils, Options};

use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avutil::{AVFrame, AVHWDeviceContext, AVHWFramesContext};
use rsmpeg::{ffi, UnsafeDerefMut};

use anyhow::{Context, Error, Result};
use once_cell::sync::Lazy;
use std::cell::UnsafeCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Hardware device configuration.
/// This struct contains all the necessary information to create a hardware device context.
///
/// The sw / hw frames conversion process includes the following steps:
///
/// CPU(NV12) -> GPU(CUDA) -> transform -> GPU(CUDA) -> CPU(NV12)
#[derive(Clone)]
pub struct HWDeviceConfig {
    pub device_type: HWDeviceType,
    pub hw_pixel_format: PixelFormat,
    pub sw_pixel_format: PixelFormat,
    pub device_id: Option<String>,
    pub options: Option<Options>,
}

impl HWDeviceConfig {
    /// create a new HWDeviceConfig with the given parameters
    ///
    /// # Arguments
    ///
    /// * `device_type` - The type of hardware device
    /// * `hw_pixel_format` - The pixel format of the hardware device
    /// * `sw_pixel_format` - The pixel format of the software device
    /// * `device_id` - The type-specific string identifying of the GPU device,
    ///   e.g. for NVIDIA CUDA, device_id should be explicitly the GPU ID  "0" or "1",
    ///   for VAAPI: device_id should be set like "/dev/dri/renderD128"
    /// * `options` - Additional (type-specific) options to use in opening the device
    pub fn new(
        device_type: HWDeviceType,
        hw_pixel_format: PixelFormat,
        sw_pixel_format: PixelFormat,
        device_id: Option<String>,
        options: Option<Options>,
    ) -> Self {
        Self {
            device_type,
            hw_pixel_format,
            sw_pixel_format,
            device_id,
            options,
        }
    }

    /// build CUDA HWDeviceConfig
    pub fn cuda(id: Option<usize>) -> Self {
        Self::new(
            HWDeviceType::CUDA,
            PixelFormat::CUDA,
            PixelFormat::NV12,
            id.map(|id| format!("{id}")),
            None,
        )
    }

    /// build VAAPI HWDeviceConfig
    pub fn vaapi(device_id: Option<String>) -> Self {
        Self::new(
            HWDeviceType::VAAPI,
            PixelFormat::VAAPI,
            PixelFormat::NV12,
            device_id,
            None,
        )
    }

    /// build VULKAN HWDeviceConfig
    pub fn vulkan(device_id: Option<String>) -> Self {
        Self::new(
            HWDeviceType::VULKAN,
            PixelFormat::VULKAN,
            PixelFormat::NV12,
            device_id,
            None,
        )
    }

    /// build QSV (Intel Quick Sync Video) HWDeviceConfig
    pub fn qsv(device_id: Option<String>) -> Self {
        Self::new(
            HWDeviceType::QSV,
            PixelFormat::QSV,
            PixelFormat::NV12,
            device_id,
            None,
        )
    }
}

impl std::hash::Hash for HWDeviceConfig {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.device_type.hash(state);
        self.device_id.hash(state);
        self.hw_pixel_format.hash(state);
        self.sw_pixel_format.hash(state);
        if let Some(opts) = &self.options {
            let pairs: HashMap<String, String> = opts.into();
            for (key, value) in pairs {
                key.hash(state);
                value.hash(state);
            }
        }
    }
}

impl PartialEq for HWDeviceConfig {
    fn eq(&self, other: &Self) -> bool {
        self.device_type == other.device_type
            && self.device_id == other.device_id
            && self.hw_pixel_format == other.hw_pixel_format
            && self.sw_pixel_format == other.sw_pixel_format
            && match (&self.options, &other.options) {
                (Some(a), Some(b)) => {
                    let pairs_a: HashMap<String, String> = a.into();
                    let pairs_b: HashMap<String, String> = b.into();
                    pairs_a.len() == pairs_b.len()
                        && pairs_a.iter().all(|(k, v)| pairs_b.get(k) == Some(v))
                }
                (None, None) => true,
                _ => false,
            }
    }
}

impl Eq for HWDeviceConfig {}

impl std::fmt::Debug for HWDeviceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "HWDeviceConfig {{ device_type: {:?}, device_id: {:?},  hw_pixel_format: {:?}, sw_pixel_format: {:?}, options: {:?} }}",
            self.device_type,
            self.device_id,
            self.hw_pixel_format,
            self.sw_pixel_format,
            self.options,
        )
    }
}

impl std::fmt::Display for HWDeviceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// `HWContext` cache for safe sharing of hardware device contexts.
static HW_CTX_CACHE: Lazy<Mutex<HashMap<HWDeviceConfig, Arc<HWContext>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// `HWContext` represents a hardware context.
///
/// It includes methods for setting up hardware frames, downloading frames from hardware to system memory,
/// and uploading frames from system memory to hardware.
pub struct HWContext {
    config: HWDeviceConfig,
    device_ctx: UnsafeCell<AVHWDeviceContext>,
}

impl HWContext {
    /// create a new HWContext with the given HWDeviceConfig
    pub fn new(config: HWDeviceConfig) -> Result<Arc<HWContext>> {
        let mut cache = HW_CTX_CACHE.lock().unwrap();

        if let Some(ctx) = cache.get(&config) {
            log::debug!("Reusing existing hardware device context. config:{config:?}");
            return Ok(ctx.clone());
        }

        // create a new hardware device context
        let hw_device_ctx = {
            let device = utils::from_str_opt(config.device_id.as_ref());
            let opts = config.options.as_ref().map(|opts| opts.as_dict());
            AVHWDeviceContext::create(config.device_type.into(), device.as_deref(), opts, 0)
                .context("Failed to create hardware device context")?
        };

        log::debug!("Created hardware device context successfully. config:{config}");

        let ctx = Arc::new(Self {
            config: config.clone(),
            device_ctx: UnsafeCell::new(hw_device_ctx),
        });
        cache.insert(config, ctx.clone());

        Ok(ctx)
    }

    /// initialize HWFramesContext for the given codec context
    ///
    /// # Arguments
    ///
    /// * `is_decoder` - Whether the codec context is for decoding or encoding
    /// * `codec_ctx` - The codec context to initialize
    /// * `width` - The width of the input/output frames
    /// * `height` - The height of the input/output frames
    pub fn setup_hw_frames(
        &self,
        is_decoder: bool,
        codec_ctx: &mut AVCodecContext,
        width: i32,
        height: i32,
    ) -> Result<()> {
        let hw_device_ctx_ref = unsafe { &mut *self.device_ctx.get() };
        let mut hw_frames_ctx = hw_device_ctx_ref.hwframe_ctx_alloc();
        hw_frames_ctx.data().format = self.get_format(true);
        hw_frames_ctx.data().sw_format = self.get_format(false);
        hw_frames_ctx.data().width = width;
        hw_frames_ctx.data().height = height;
        hw_frames_ctx.data().initial_pool_size = 20;

        hw_frames_ctx
            .init()
            .context("Failed to initialize hardware frame context")?;

        codec_ctx.set_hw_frames_ctx(hw_frames_ctx);
        codec_ctx.set_pix_fmt(self.get_format(true));

        // only used by decoders
        if is_decoder {
            unsafe {
                let ctx_mut_ptr = codec_ctx.deref_mut();
                ctx_mut_ptr.opaque = self.get_format(true) as *mut std::os::raw::c_void;
                ctx_mut_ptr.get_format = Some(hwaccel_get_format);
                ctx_mut_ptr.sw_pix_fmt = self.get_format(false);
                ctx_mut_ptr.hw_device_ctx = hw_device_ctx_ref.as_mut_ptr();
            }
        }

        Ok(())
    }

    /// Download frame from hardware acceleration device to system memory.
    ///
    /// This method transfers the frame data from GPU memory to CPU memory,
    /// converting from hardware pixel format to software pixel format.
    ///
    /// # Arguments
    /// * `hw_frame` - The source frame in hardware memory
    ///
    /// # Returns
    /// * `Result<AVFrame>` - A new frame in system memory with transferred data
    ///
    /// # Example
    /// ```rust,ignore
    /// let hw_frame = // ... frame from decoder
    /// let sw_frame = hw_context.hw_download(&hw_frame)?;
    /// // Now sw_frame contains the data in CPU memory
    /// ```
    pub fn hw_download(&self, decoder: &mut AVCodecContext, hw_frame: &AVFrame) -> Result<AVFrame> {
        let hw_down_start = std::time::Instant::now();

        // Check if input frame is actually in hardware memory
        if !self.is_hw_frame(hw_frame) {
            return Err(Error::msg(format!(
                "Input frame is not a valid hardware frame: format={:?}, expected={:?}, hw_frames_ctx={:?}",
                hw_frame.format,
                self.config.hw_pixel_format,
                hw_frame.hw_frames_ctx.is_null()
            )));
        }

        unsafe {
            if decoder.hw_frames_ctx().is_none() {
                log::debug!(
                    "decoder hw_frames_ctx is null, is_hwaccel:{}",
                    decoder.is_hwaccel()
                );
                decoder.set_hw_frames_ctx(AVHWFramesContext::from_raw(
                    std::ptr::NonNull::new(hw_frame.hw_frames_ctx).unwrap(),
                ));
            }
        }

        // 创建软件帧
        let mut sw_frame = AVFrame::new();
        sw_frame.set_width(hw_frame.width);
        sw_frame.set_height(hw_frame.height);
        sw_frame.set_format(self.get_format(false));
        sw_frame
            .alloc_buffer()
            .context("Failed to allocate software frame buffer")?;

        // 该方法分配硬件帧缓冲区，这里是从硬件帧转换为软件帧，所以需要分配软件帧缓冲区
        // hw_frames_ctx
        //     .get_buffer(&mut sw_frame)
        //     .context("Failed to allocate software frame buffer")?;

        // 从硬件帧传输数据到软件帧
        sw_frame
            .hwframe_transfer_data(hw_frame)
            .context("Failed to transfer data from hardware frame to software frame")?;

        // 复制帧属性
        self.copy_frame_props(hw_frame, &mut sw_frame)?;

        log::debug!(
            "Downloaded from GPU: format={:?}, size={}x{}, linesize=[{}, {}], cost={:?}ms",
            PixelFormat::from(sw_frame.format),
            sw_frame.width,
            sw_frame.height,
            sw_frame.linesize[0],
            sw_frame.linesize[1],
            hw_down_start.elapsed()
        );

        Ok(sw_frame)
    }

    /// Upload frame to hardware acceleration device.
    ///
    /// This method transfers the frame data from CPU memory to GPU memory,
    /// converting from software pixel format to hardware pixel format.
    ///
    /// # Arguments
    /// * `sw_frame` - The source frame in system memory
    ///
    /// # Returns
    /// * `Result<AVFrame>` - A new frame in hardware memory with transferred data
    ///
    /// # Example
    /// ```rust,ignore
    /// let sw_frame = // ... frame in system memory
    /// let hw_frame = hw_context.hw_upload(&sw_frame)?;
    /// // Now hw_frame contains the data in GPU memory
    /// ```
    pub fn hw_upload(&self, encoder: &mut AVCodecContext, sw_frame: &AVFrame) -> Result<AVFrame> {
        let hw_up_start = std::time::Instant::now();

        // Check if input frame format matches our software format
        if !self.is_sw_frame(sw_frame) {
            return Err(Error::msg(format!(
                "Input frame format ({:?}) doesn't match expected software format ({:?})",
                sw_frame.format, self.config.sw_pixel_format
            )));
        }

        // 确保编码器上下文有硬件帧上下文
        let mut hw_frames_ctx = encoder
            .hw_frames_ctx_mut()
            .ok_or_else(|| Error::msg("Encoder has no hardware frames context"))?;

        // 创建硬件帧
        let mut hw_frame = AVFrame::new();
        hw_frame.set_width(sw_frame.width);
        hw_frame.set_height(sw_frame.height);
        hw_frame.set_format(self.get_format(true));
        unsafe {
            (*hw_frame.as_mut_ptr()).hw_frames_ctx = hw_frames_ctx.as_mut_ptr();
        }

        // 分配硬件缓冲区
        hw_frames_ctx
            .get_buffer(&mut hw_frame)
            .context("Failed to allocate hardware frame buffer")?;

        // 从软件帧传输数据到硬件帧
        hw_frame
            .hwframe_transfer_data(sw_frame)
            .context("Failed to transfer data from software frame to hardware frame")?;

        // 复制帧属性
        self.copy_frame_props(sw_frame, &mut hw_frame)?;

        log::debug!(
            "Uploaded to GPU: format={:?}, size={}x{}, linesize=[{}, {}], cost={:?}ms",
            PixelFormat::from(hw_frame.format),
            hw_frame.width,
            hw_frame.height,
            hw_frame.linesize[0],
            hw_frame.linesize[1],
            hw_up_start.elapsed()
        );

        Ok(hw_frame)
    }

    /// 复制帧属性
    ///
    /// # Arguments
    /// * `dst` - The destination frame to which properties will be copied.
    /// * `src` - The source frame from which properties will be copied.
    fn copy_frame_props(&self, src: &AVFrame, dst: &mut AVFrame) -> Result<()> {
        dst.set_pts(src.pts);
        dst.set_time_base(src.time_base);
        dst.set_pict_type(src.pict_type);
        dst.set_ch_layout(src.ch_layout);
        dst.set_nb_samples(src.nb_samples);
        dst.set_sample_rate(src.sample_rate);

        unsafe {
            let dst_ptr = dst.as_mut_ptr();
            (*dst_ptr).flags = src.flags;
            (*dst_ptr).opaque = src.opaque;
            (*dst_ptr).quality = src.quality;
            (*dst_ptr).duration = src.duration;
            (*dst_ptr).sample_aspect_ratio = src.sample_aspect_ratio;
        }

        // 复制帧属性
        imgutils::copy_frame_metadata(src, dst, false)
    }

    /// Determine if a frame is in hardware memory
    ///
    /// # Arguments
    /// * `frame` - The frame to check
    ///
    /// # Returns
    /// * `bool` - True if the frame is in hardware memory
    pub fn is_hw_frame(&self, frame: &AVFrame) -> bool {
        // 检查硬件帧上下文是否为空
        if frame.hw_frames_ctx.is_null() {
            log::debug!("Frame hardware context is null");
            return false;
        }

        // 检查帧格式是否匹配硬件像素格式
        if frame.format != self.get_format(true) {
            return false;
        }

        true
    }

    /// Check if a frame is in software memory format
    pub fn is_sw_frame(&self, frame: &AVFrame) -> bool {
        frame.format == self.get_format(false)
    }

    /// Helper function to get the appropriate pixel format for a frame
    pub fn get_format(&self, is_hw: bool) -> ffi::AVPixelFormat {
        if is_hw {
            self.config.hw_pixel_format.into()
        } else {
            self.config.sw_pixel_format.into()
        }
    }
}

unsafe impl Send for HWContext {}
unsafe impl Sync for HWContext {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum HWDeviceType {
    /// ffi definition NONE: 0
    NONE,
    /// Video Decode and Presentation API for Unix (VDPAU)
    VDPAU,
    /// NVIDIA CUDA
    CUDA,
    /// Video Acceleration API (VA-API)
    VAAPI,
    /// DirectX Video Acceleration 2.0
    DXVA2,
    /// Quick Sync Video
    QSV,
    /// VideoToolbox
    VIDEOTOOLBOX,
    /// Direct3D 11 Video Acceleration
    D3D11VA,
    /// Linux Direct Rendering Manager
    DRM,
    /// OpenCL
    OPENCL,
    /// MediaCodec
    MEDIACODEC,
    /// Vulkan
    VULKAN,
    /// Direct3D 12 Video Acceleration
    #[cfg(feature = "ffmpeg7")]
    D3D12VA,
}

impl HWDeviceType {
    /// Whether or not the device type is available on this system.
    pub fn is_available(self) -> bool {
        self.list_available().contains(&self)
    }

    /// List available hardware acceleration device types on this system.
    ///
    /// Uses `av_hwdevice_iterate_types` internally.
    pub fn list_available(self) -> Vec<HWDeviceType> {
        let mut hw_device_types = Vec::new();
        unsafe {
            let mut hwdevice_type = ffi::av_hwdevice_iterate_types(ffi::AV_HWDEVICE_TYPE_NONE);
            while hwdevice_type != ffi::AV_HWDEVICE_TYPE_NONE {
                hw_device_types.push(HWDeviceType::from(hwdevice_type));
                hwdevice_type = ffi::av_hwdevice_iterate_types(hwdevice_type);
            }
            hw_device_types
        }
    }

    /// Find the best available hardware acceleration device config on this system.
    pub fn auto_best_config(self) -> Result<HWDeviceConfig> {
        if self.is_available() {
            Ok(HWDeviceConfig::new(
                self,
                self.default_hw_pixel_format(),
                self.default_sw_pixel_format(),
                None,
                None,
            ))
        } else {
            let devices = self.list_available();
            if devices.is_empty() {
                return Err(Error::msg("No suitable hardware acceleration device found"));
            }
            let device = devices[0];
            Ok(HWDeviceConfig::new(
                device,
                device.default_hw_pixel_format(),
                device.default_sw_pixel_format(),
                None,
                None,
            ))
        }
    }

    /// 获取硬件设备对应的像素格式
    pub fn default_hw_pixel_format(&self) -> PixelFormat {
        match self {
            HWDeviceType::NONE => PixelFormat::NONE,
            HWDeviceType::VDPAU => PixelFormat::VDPAU,
            HWDeviceType::CUDA => PixelFormat::CUDA,
            HWDeviceType::VAAPI => PixelFormat::VAAPI,
            HWDeviceType::DXVA2 => PixelFormat::DXVA2_VLD,
            HWDeviceType::QSV => PixelFormat::QSV,
            HWDeviceType::VIDEOTOOLBOX => PixelFormat::VIDEOTOOLBOX,
            HWDeviceType::D3D11VA => PixelFormat::D3D11,
            HWDeviceType::DRM => PixelFormat::DRM_PRIME,
            HWDeviceType::OPENCL => PixelFormat::OPENCL,
            HWDeviceType::MEDIACODEC => PixelFormat::MEDIACODEC,
            HWDeviceType::VULKAN => PixelFormat::VULKAN,
            #[cfg(feature = "ffmpeg7")]
            HWDeviceType::D3D12VA => PixelFormat::D3D12,
        }
    }

    /// 获取硬件设备默认支持的软件像素格式
    pub fn default_sw_pixel_format(&self) -> PixelFormat {
        match self {
            // OpenCL/Vulkan 默认使用 RGBA
            HWDeviceType::OPENCL | HWDeviceType::VULKAN => PixelFormat::RGBA,
            // 其他设备默认使用 NV12
            _ => PixelFormat::NV12,
        }
    }

    /// 注意：该方法只适用于硬件解码，用于查找解码器输出到硬件表面所需的像素格式。
    /// 对于硬件编码，应检查编码器 AVCodec 的 pix_fmts 字段来确定支持的输入像素格式。
    pub fn find_hw_pixel_format_with_codec(&self, codec: &AVCodec) -> Option<ffi::AVPixelFormat> {
        let mut i = 0;
        loop {
            unsafe {
                let hw_config = ffi::avcodec_get_hw_config(codec.as_ptr(), i);
                if !hw_config.is_null() {
                    #[allow(clippy::unnecessary_cast)]
                    let hw_config_supports_codec = ((*hw_config).methods as i32
                        & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32)
                        != 0;
                    if hw_config_supports_codec && (*hw_config).device_type == (*self).into() {
                        break Some((*hw_config).pix_fmt);
                    }
                } else {
                    break None;
                }
            }
            i += 1;
        }
    }
}

impl From<ffi::AVHWDeviceType> for HWDeviceType {
    fn from(value: ffi::AVHWDeviceType) -> Self {
        match value {
            ffi::AV_HWDEVICE_TYPE_NONE => HWDeviceType::NONE,
            ffi::AV_HWDEVICE_TYPE_VDPAU => HWDeviceType::VDPAU,
            ffi::AV_HWDEVICE_TYPE_CUDA => HWDeviceType::CUDA,
            ffi::AV_HWDEVICE_TYPE_VAAPI => HWDeviceType::VAAPI,
            ffi::AV_HWDEVICE_TYPE_DXVA2 => HWDeviceType::DXVA2,
            ffi::AV_HWDEVICE_TYPE_QSV => HWDeviceType::QSV,
            ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX => HWDeviceType::VIDEOTOOLBOX,
            ffi::AV_HWDEVICE_TYPE_D3D11VA => HWDeviceType::D3D11VA,
            ffi::AV_HWDEVICE_TYPE_DRM => HWDeviceType::DRM,
            ffi::AV_HWDEVICE_TYPE_OPENCL => HWDeviceType::OPENCL,
            ffi::AV_HWDEVICE_TYPE_MEDIACODEC => HWDeviceType::MEDIACODEC,
            ffi::AV_HWDEVICE_TYPE_VULKAN => HWDeviceType::VULKAN,
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_HWDEVICE_TYPE_D3D12VA => HWDeviceType::D3D12VA,

            #[allow(unreachable_patterns)]
            _ => panic!("Unknown HWDeviceType"),
        }
    }
}

impl From<HWDeviceType> for ffi::AVHWDeviceType {
    fn from(value: HWDeviceType) -> Self {
        match value {
            HWDeviceType::NONE => ffi::AV_HWDEVICE_TYPE_NONE,
            HWDeviceType::VDPAU => ffi::AV_HWDEVICE_TYPE_VDPAU,
            HWDeviceType::CUDA => ffi::AV_HWDEVICE_TYPE_CUDA,
            HWDeviceType::VAAPI => ffi::AV_HWDEVICE_TYPE_VAAPI,
            HWDeviceType::DXVA2 => ffi::AV_HWDEVICE_TYPE_DXVA2,
            HWDeviceType::QSV => ffi::AV_HWDEVICE_TYPE_QSV,
            HWDeviceType::VIDEOTOOLBOX => ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            HWDeviceType::D3D11VA => ffi::AV_HWDEVICE_TYPE_D3D11VA,
            HWDeviceType::DRM => ffi::AV_HWDEVICE_TYPE_DRM,
            HWDeviceType::OPENCL => ffi::AV_HWDEVICE_TYPE_OPENCL,
            HWDeviceType::MEDIACODEC => ffi::AV_HWDEVICE_TYPE_MEDIACODEC,
            HWDeviceType::VULKAN => ffi::AV_HWDEVICE_TYPE_VULKAN,
            #[cfg(feature = "ffmpeg7")]
            HWDeviceType::D3D12VA => ffi::AV_HWDEVICE_TYPE_D3D12VA,
        }
    }
}

#[no_mangle]
unsafe extern "C" fn hwaccel_get_format(
    ctx: *mut ffi::AVCodecContext,
    pix_fmts: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    let mut p = pix_fmts;
    let hw_format = (*ctx).opaque as ffi::AVPixelFormat;
    while *p != ffi::AV_PIX_FMT_NONE {
        if *p == hw_format {
            return *p;
        }
        p = p.add(1);
    }
    ffi::AV_PIX_FMT_NONE
}
