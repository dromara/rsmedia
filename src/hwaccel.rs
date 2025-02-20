use crate::pixel::PixelFormat;
use anyhow::{Context, Error, Result};
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avutil::{AVFrame, AVHWDeviceContext, AVPixelFormat};
use rsmpeg::ffi;

/// 硬件加速设备配置
#[derive(Debug, Clone)]
pub struct HWDeviceConfig {
    device_type: HWDeviceType,
    hw_pixel_format: PixelFormat,
    sw_pixel_format: PixelFormat,
    device_path: Option<String>,
}

impl HWDeviceConfig {
    pub fn new(
        device_type: HWDeviceType,
        hw_pixel_format: PixelFormat,
        sw_pixel_format: PixelFormat,
        device_path: Option<String>,
    ) -> Self {
        Self {
            device_type,
            hw_pixel_format,
            sw_pixel_format,
            device_path,
        }
    }

    /// 创建NVIDIA配置
    pub fn cuda() -> Self {
        Self::new(
            HWDeviceType::CUDA,
            PixelFormat::CUDA,
            PixelFormat::NV12,
            None,
        )
    }

    /// 创建VAAPI配置
    pub fn vaapi(device_path: Option<String>) -> Self {
        Self::new(
            HWDeviceType::VAAPI,
            PixelFormat::VAAPI,
            PixelFormat::NV12,
            device_path,
        )
    }

    /// VDPAU设 备配置
    pub fn vdpau() -> Self {
        Self::new(
            HWDeviceType::VDPAU,
            PixelFormat::VDPAU,
            PixelFormat::NV12,
            None,
        )
    }
}

pub struct HWContext {
    device_ctx: AVHWDeviceContext,
    config: HWDeviceConfig,
}

impl HWContext {
    pub fn new(config: HWDeviceConfig) -> Result<Self> {
        let device_path = config.device_path.as_deref();
        let mut device_ctx = AVHWDeviceContext::create(
            config.device_type.into(),
            device_path
                .map(std::ffi::CString::new)
                .transpose()
                .unwrap()
                .as_deref(),
            None,
            0,
        )
        .context("Failed to create hardware device context")?;

        device_ctx.init()?;

        Ok(Self { device_ctx, config })
    }

    /// 设置编解码器的硬件帧上下文
    pub fn setup_hw_frames(
        &self,
        codec_ctx: &mut AVCodecContext,
        width: i32,
        height: i32,
    ) -> Result<()> {
        let mut hw_frames_ref = self.device_ctx.hwframe_ctx_alloc();

        let frames_data = hw_frames_ref.data();
        frames_data.format = self.config.hw_pixel_format.into_raw();
        frames_data.sw_format = self.config.sw_pixel_format.into_raw();
        frames_data.width = width;
        frames_data.height = height;
        frames_data.initial_pool_size = 20;

        hw_frames_ref.init().unwrap();

        codec_ctx.set_pix_fmt(self.config.hw_pixel_format.into_raw());
        codec_ctx.set_hw_frames_ctx(hw_frames_ref);

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
    /// let sw_frame = hw_context.download_frame(&hw_frame)?;
    /// // Now sw_frame contains the data in CPU memory
    /// ```
    pub fn download_frame(&self, hw_frame: &AVFrame) -> Result<AVFrame> {
        // Check if input frame is actually in hardware memory
        if hw_frame.buf[0].is_null() && hw_frame.format == self.config.hw_pixel_format as i32 {
            return Err(Error::msg("Input frame is not a hardware frame"));
        }

        let mut sw_frame = AVFrame::new();

        // Set properties for the software frame
        sw_frame.set_width(hw_frame.width);
        sw_frame.set_height(hw_frame.height);
        sw_frame.set_format(self.config.sw_pixel_format.into_raw());
        sw_frame
            .alloc_buffer()
            .context("Failed to  Allocate buffer for software frame")?;

        // Transfer data from hardware to software frame
        sw_frame
            .hwframe_transfer_data(hw_frame)
            .context("Failed to transfer frame data from hardware to system memory")?;

        // Copy frame properties
        unsafe {
            ffi::av_frame_copy_props(sw_frame.as_mut_ptr(), hw_frame.as_ptr());
        }

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
    /// let hw_frame = hw_context.upload_frame(&sw_frame)?;
    /// // Now hw_frame contains the data in GPU memory
    /// ```
    pub fn upload_frame(&self, sw_frame: &AVFrame) -> Result<AVFrame> {
        // Check if input frame format matches our software format
        if sw_frame.format != self.config.sw_pixel_format.into_raw() {
            return Err(Error::msg(format!(
                "Input frame format ({:?}) doesn't match expected software format ({:?})",
                sw_frame.format, self.config.sw_pixel_format
            )));
        }

        // Create new frame for hardware format
        let mut hw_frame = AVFrame::new();

        // Set basic properties
        hw_frame.set_width(sw_frame.width);
        hw_frame.set_height(sw_frame.height);
        hw_frame.set_format(self.config.hw_pixel_format.into_raw());
        hw_frame
            .alloc_buffer()
            .context("AVFrame alloc buffer error")?;

        let mut frame_ctx = self.device_ctx.hwframe_ctx_alloc();
        frame_ctx.make_writable();
        frame_ctx
            .get_buffer(&mut hw_frame)
            .context("Failed to allocate hardware frame buffer")?;

        // Transfer data from software to hardware frame
        hw_frame
            .hwframe_transfer_data(sw_frame)
            .context("Failed to transfer frame data to hardware memory")?;

        // Copy frame properties
        unsafe {
            ffi::av_frame_copy_props(hw_frame.as_mut_ptr(), sw_frame.as_ptr());
        }

        Ok(hw_frame)
    }

    /// Determine if a frame is in hardware memory
    ///
    /// # Arguments
    /// * `frame` - The frame to check
    ///
    /// # Returns
    /// * `bool` - True if the frame is in hardware memory
    pub fn is_hw_frame(&self, frame: &AVFrame) -> bool {
        !frame.buf[0].is_null() && frame.format == self.config.hw_pixel_format as i32
    }

    /// Helper function to get the appropriate pixel format for a frame
    pub fn get_frame_format(&self, is_hw: bool) -> AVPixelFormat {
        if is_hw {
            self.config.hw_pixel_format.into_raw()
        } else {
            self.config.sw_pixel_format.into_raw()
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HWDeviceType {
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
                hw_device_types.push(HWDeviceType::from(hwdevice_type).unwrap());
                hwdevice_type = ffi::av_hwdevice_iterate_types(hwdevice_type);
            }
            hw_device_types
        }
    }

    pub fn codec_find_hwaccel_pixfmt(
        self,
        codec: &AVCodec,
        hwaccel_type: HWDeviceType,
    ) -> Option<AVPixelFormat> {
        let mut i = 0;
        loop {
            unsafe {
                let hw_config = ffi::avcodec_get_hw_config(codec.as_ptr(), i);
                if !hw_config.is_null() {
                    let hw_config_supports_codec = (((*hw_config).methods) as i32
                        & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as i32)
                        != 0;
                    if hw_config_supports_codec && (*hw_config).device_type == hwaccel_type.into() {
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

impl HWDeviceType {
    pub fn from(value: ffi::AVHWDeviceType) -> Option<HWDeviceType> {
        match value {
            ffi::AV_HWDEVICE_TYPE_NONE => None,
            ffi::AV_HWDEVICE_TYPE_VDPAU => Some(Self::VDPAU),
            ffi::AV_HWDEVICE_TYPE_CUDA => Some(Self::CUDA),
            ffi::AV_HWDEVICE_TYPE_VAAPI => Some(Self::VAAPI),
            ffi::AV_HWDEVICE_TYPE_DXVA2 => Some(Self::DXVA2),
            ffi::AV_HWDEVICE_TYPE_QSV => Some(Self::QSV),
            ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX => Some(Self::VIDEOTOOLBOX),
            ffi::AV_HWDEVICE_TYPE_D3D11VA => Some(Self::D3D11VA),
            ffi::AV_HWDEVICE_TYPE_DRM => Some(Self::DRM),
            ffi::AV_HWDEVICE_TYPE_OPENCL => Some(Self::OPENCL),
            ffi::AV_HWDEVICE_TYPE_MEDIACODEC => Some(Self::MEDIACODEC),
            ffi::AV_HWDEVICE_TYPE_VULKAN => Some(Self::VULKAN),
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_HWDEVICE_TYPE_D3D12VA => Some(Self::D3D12VA),

            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }
}

impl From<HWDeviceType> for ffi::AVHWDeviceType {
    fn from(value: HWDeviceType) -> Self {
        match value {
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
