use crate::error::{Context, Result, RsmediaError};
use crate::pixel::PixelFormat;
use crate::{Options, imgutils, strutils};

use dashmap::DashMap;
use once_cell::sync::Lazy;
use rsmpeg::avcodec::{AVCodec, AVCodecContext};
use rsmpeg::avutil::{AVFrame, AVHWDeviceContext, AVPixFmtDescriptorRef};
use rsmpeg::{UnsafeDerefMut, ffi};

use std::sync::Arc;

/// Hardware device configuration.
/// This struct contains all the necessary information to create a hardware device context.
///
/// The sw / hw frames conversion process includes the following steps:
///
/// CPU(NV12) -> GPU(CUDA) -> transform -> GPU(CUDA) -> CPU(NV12)
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
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
    ///
    /// `device_id` 为 GPU 编号字符串（如 `"0"`、`"1"`），与其他设备构造器
    /// 的类型保持一致（VAAPI 传 DRM 设备路径、QSV 传设备序号等）。
    pub fn cuda(device_id: Option<String>) -> Self {
        Self::new(
            HWDeviceType::CUDA,
            PixelFormat::CUDA,
            PixelFormat::NV12,
            device_id,
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

    /// build AMD AMF HWDeviceConfig（Windows 平台，基于 D3D11 设备）。
    ///
    /// FFmpeg 的 AMF 编码器（`h264_amf`/`hevc_amf`/`av1_amf`）没有独立的
    /// hw_context 类型，挂在 `AV_HWDEVICE_TYPE_D3D11VA` 下：软件帧（NV12）
    /// 先上传到 D3D11 surface，再由 AMF 编码。
    #[cfg(target_os = "windows")]
    pub fn amf(device_id: Option<String>) -> Self {
        Self::new(
            HWDeviceType::D3D11VA,
            PixelFormat::D3D11,
            PixelFormat::NV12,
            device_id,
            None,
        )
    }

    /// 按当前平台自动选择最佳可用的硬件加速配置。
    ///
    /// 依 [`HWDeviceType::platform_preference`] 的平台优先级依次探测
    /// （`av_hwdevice_iterate_types`），返回第一个可用的设备配置；全部不可用
    /// （无 GPU / 无驱动 / 无 FFmpeg 支持编译）时返回错误，**不会**回退到
    /// 随机设备 —— 需要软件路径时由调用方显式省略 hw 配置。
    pub fn auto_platform() -> Result<Self> {
        HWDeviceType::auto_platform_config(None)
    }

    /// [`Self::auto_platform`] 的可定制版本：传入自定义候选顺序（如只想在
    /// CUDA 与 QSV 之间选择）；空切片返回错误（等价于无候选可探测）。
    pub fn auto_platform_with(candidates: &[HWDeviceType]) -> Result<Self> {
        HWDeviceType::auto_platform_config(Some(candidates))
    }
}

impl std::fmt::Display for HWDeviceConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, f)
    }
}

/// `HWContext` cache for safe sharing of hardware device contexts.
/// Uses DashMap for better concurrent read/write performance.
static HW_CTX_CACHE: Lazy<DashMap<HWDeviceConfig, Arc<HWContext>>> = Lazy::new(DashMap::new);

/// 缓存容量上限：超出时自动驱逐**未被使用**（引用计数为 1）的条目。
///
/// 硬件设备上下文持有 GPU 资源，长驻进程中持续切换配置（device_id / 选项 /
/// 设备类型组合不同）会不断产生新条目；不设上限会导致 GPU 资源泄漏。
/// 典型应用只使用 1~2 种配置，8 已留足余量。仍在使用的条目不会被驱逐，
/// 全部在使用中时可超额容纳（等待 [`clear_hw_ctx_cache`] 后续清理）。
const HW_CTX_CACHE_MAX_ENTRIES: usize = 8;

/// 容量超限时驱逐未使用条目（保留使用中的与新创建的 `keep` 条目）。
fn prune_hw_ctx_cache(keep: &HWDeviceConfig) {
    if HW_CTX_CACHE.len() <= HW_CTX_CACHE_MAX_ENTRIES {
        return;
    }
    let mut removed = 0;
    HW_CTX_CACHE.retain(|config, ctx| {
        if Arc::strong_count(ctx) > 1 || config == keep {
            true
        } else {
            removed += 1;
            false
        }
    });
    if HW_CTX_CACHE.len() > HW_CTX_CACHE_MAX_ENTRIES {
        log::warn!(
            "HW context cache still holds {} entries (> {HW_CTX_CACHE_MAX_ENTRIES}) \
             after pruning {removed}: all in use, will shrink once released.",
            HW_CTX_CACHE.len()
        );
    } else if removed > 0 {
        log::debug!("Pruned {removed} unused hardware device context(s) (cache over cap).");
    }
}

/// 清理硬件上下文缓存。
///
/// 移除所有**当前未被使用**（引用计数为 1，仅缓存自身持有）的硬件设备上下文，
/// 释放对应的 GPU 资源。仍在被解码器/编码器使用的上下文会保留，待其释放后
/// 可再次调用清理。
///
/// 除本手动 API 外，缓存超过 [`HW_CTX_CACHE_MAX_ENTRIES`] 时会在新建条目时
/// 自动驱逐未使用条目。
///
/// 返回被移除的条目数。
pub fn clear_hw_ctx_cache() -> usize {
    let mut removed = 0;
    // retain 逐条检查：仅保留仍在使用或为当前 config 未缓存的条目。
    HW_CTX_CACHE.retain(|_config, ctx| {
        if Arc::strong_count(ctx) > 1 {
            true
        } else {
            removed += 1;
            false
        }
    });
    if removed > 0 {
        log::debug!("Cleared {removed} unused hardware device context(s).");
    }
    removed
}

/// `HWContext` represents a hardware context.
///
/// It includes methods for setting up hardware frames, downloading frames from hardware to system memory,
/// and uploading frames from system memory to hardware.
///
/// 所有方法只做共享访问（`&self`）：底层 `av_hwframe_ctx_alloc` / `av_buffer_ref`
/// 均为 FFmpeg 保证的线程安全原子操作，因此 [`Send`]/[`Sync`] 实现成立，
/// 相同配置的多个解码器/编码器可跨线程共享同一 `Arc<HWContext>`。
pub struct HWContext {
    config: HWDeviceConfig,
    device_ctx: AVHWDeviceContext,
}

impl HWContext {
    /// create a new HWContext with the given HWDeviceConfig
    pub fn new(config: HWDeviceConfig) -> Result<Arc<HWContext>> {
        // Try to get existing context from cache (lock-free read)
        if let Some(ctx) = HW_CTX_CACHE.get(&config) {
            log::debug!("Reusing existing hardware device context. config:{config:?}");
            return Ok(ctx.clone());
        }

        // create a new hardware device context
        let hw_device_ctx = {
            let device = strutils::str_to_cstring_opt(config.device_id.as_ref());
            let opts = config.options.as_ref().and_then(|opts| opts.to_dict());
            AVHWDeviceContext::create(
                config.device_type.into(),
                device.as_deref(),
                opts.as_ref(),
                0,
            )
            .context("Failed to create hardware device context")?
        };

        log::debug!("Created hardware device context successfully. config:{config}");

        let ctx = Arc::new(Self {
            config: config.clone(),
            device_ctx: hw_device_ctx,
        });
        // Insert into cache (lock-free write)
        HW_CTX_CACHE.insert(config, ctx.clone());
        // 超过容量上限时自动驱逐未使用的旧条目（本条目刚插入、且被局部
        // `ctx` 持有，不会被驱逐）
        prune_hw_ctx_cache(&ctx.config);

        Ok(ctx)
    }

    /// Initialize the hardware frames context for a **decoder**.
    ///
    /// Besides creating and attaching the `AVHWFramesContext`, this also:
    /// - installs the [`hwaccel_get_format`] callback so the decoder picks the
    ///   hardware surface format during `avcodec_open2`;
    /// - sets `sw_pix_fmt` to the configured software format;
    /// - holds an independent reference (`av_buffer_ref`) to the hardware
    ///   device context, so the decoder owns its own ref and unrefs it on
    ///   close — no manual teardown needed in `Decoder::Drop`.
    ///
    /// # Arguments
    ///
    /// * `codec_ctx` - The decoder codec context to initialize
    /// * `width` - The width of the decoded frames
    /// * `height` - The height of the decoded frames
    pub fn setup_decoder_frames(
        &self,
        codec_ctx: &mut AVCodecContext,
        width: i32,
        height: i32,
    ) -> Result<()> {
        let hw_frames_ctx = self.create_hw_frames_ctx(width, height)?;
        codec_ctx.set_hw_frames_ctx(hw_frames_ctx);
        codec_ctx.set_pix_fmt(self.get_format(true));

        unsafe {
            let ctx_mut_ptr = codec_ctx.deref_mut();
            ctx_mut_ptr.get_format = Some(hwaccel_get_format);
            ctx_mut_ptr.sw_pix_fmt = self.get_format(false);
        }
        // clone 即 av_buffer_ref：codec_ctx 拥有独立引用，析构时正确 unref，
        // 无需 Decoder::Drop 手动置空防 double-free。
        codec_ctx.set_hw_device_ctx(self.device_ctx.clone());

        Ok(())
    }

    /// Initialize the hardware frames context for an **encoder**.
    ///
    /// Encoders upload software frames into surfaces allocated from this
    /// frames context (see [`HWContext::hw_upload`]); they do not need a
    /// `get_format` callback or a separate device ref (the frames context
    /// already references the device).
    ///
    /// # Arguments
    ///
    /// * `codec_ctx` - The encoder codec context to initialize
    /// * `width` - The width of the frames to encode
    /// * `height` - The height of the frames to encode
    pub fn setup_encoder_frames(
        &self,
        codec_ctx: &mut AVCodecContext,
        width: i32,
        height: i32,
    ) -> Result<()> {
        let hw_frames_ctx = self.create_hw_frames_ctx(width, height)?;
        codec_ctx.set_hw_frames_ctx(hw_frames_ctx);
        codec_ctx.set_pix_fmt(self.get_format(true));

        Ok(())
    }

    /// Allocate and initialize an `AVHWFramesContext` bound to this device.
    ///
    /// 仅共享访问 device_ctx：`hwframe_ctx_alloc` 内部只做 av_buffer_ref（原子），
    /// 每次调用都新建独立的 AVHWFramesContext，由调用方（codec_ctx）独占持有。
    fn create_hw_frames_ctx(
        &self,
        width: i32,
        height: i32,
    ) -> Result<rsmpeg::avutil::AVHWFramesContext> {
        let mut hw_frames_ctx = self.device_ctx.hwframe_ctx_alloc();
        hw_frames_ctx.data().format = self.get_format(true);
        hw_frames_ctx.data().sw_format = self.get_format(false);
        hw_frames_ctx.data().width = width;
        hw_frames_ctx.data().height = height;
        hw_frames_ctx.data().initial_pool_size = 20;

        hw_frames_ctx
            .init()
            .context("Failed to initialize hardware frame context")?;
        Ok(hw_frames_ctx)
    }

    /// Download frame from hardware acceleration device to system memory.
    ///
    /// This method transfers the frame data from GPU memory to CPU memory,
    /// converting from hardware pixel format to software pixel format.
    ///
    /// 纯函数：transfer 使用的 `AVHWFramesContext` 取自 `hw_frame` 自身的
    /// `hw_frames_ctx`（解码器输出帧自带），无需解码器上下文参与。
    ///
    /// # Arguments
    /// * `hw_frame` - The source frame in hardware memory
    ///
    /// # Returns
    /// * `Result<AVFrame>` - A new frame in system memory with transferred data
    pub fn hw_download(&self, hw_frame: &AVFrame) -> Result<AVFrame> {
        let hw_down_start = std::time::Instant::now();

        // Check if input frame is actually in hardware memory
        if !self.is_hw_frame(hw_frame) {
            return Err(RsmediaError::custom(format!(
                "Input frame is not a valid hardware frame: format={:?}, expected={:?}, hw_frames_ctx={:p}",
                hw_frame.format, self.config.hw_pixel_format, hw_frame.hw_frames_ctx
            )));
        }

        // 创建软件帧
        let mut sw_frame = AVFrame::new();
        sw_frame.set_width(hw_frame.width);
        sw_frame.set_height(hw_frame.height);
        sw_frame.set_format(self.get_format(false));
        sw_frame
            .alloc_buffer()
            .context("Failed to allocate software frame buffer")?;

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
    pub fn hw_upload(&self, encoder: &mut AVCodecContext, sw_frame: &AVFrame) -> Result<AVFrame> {
        let hw_up_start = std::time::Instant::now();

        // Check if input frame format matches our software format
        if !self.is_sw_frame(sw_frame) {
            return Err(RsmediaError::custom(format!(
                "Input frame format ({:?}) doesn't match expected software format ({:?})",
                sw_frame.format, self.config.sw_pixel_format
            )));
        }

        // 确保编码器上下文有硬件帧上下文
        let mut hw_frames_ctx = encoder
            .hw_frames_ctx_mut()
            .ok_or_else(|| RsmediaError::custom("Encoder has no hardware frames context"))?;

        // 创建硬件帧
        let mut hw_frame = AVFrame::new();
        hw_frame.set_width(sw_frame.width);
        hw_frame.set_height(sw_frame.height);
        hw_frame.set_format(self.get_format(true));
        // 注意：这里不要再手动把 hw_frames_ctx 赋给 hw_frame。
        // av_hwframe_get_buffer 会在 frame->hw_frames_ctx 为 NULL 时自动 av_buffer_ref；
        // 若预先赋成与编码器共享同一 ref，则 hw_frame.unref 会把共享 ref 减到 0，
        // 造成编码器 hw_frames_ctx 悬空 → double-free。

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

    /// 复制视频帧属性（时间戳/画面类型/宽高比等）与 side-data 元数据。
    ///
    /// # Arguments
    /// * `dst` - The destination frame to which properties will be copied.
    /// * `src` - The source frame from which properties will be copied.
    fn copy_frame_props(&self, src: &AVFrame, dst: &mut AVFrame) -> Result<()> {
        dst.set_pts(src.pts);
        dst.set_time_base(src.time_base);
        dst.set_pict_type(src.pict_type);

        unsafe {
            let dst_ptr = dst.as_mut_ptr();
            (*dst_ptr).flags = src.flags;
            (*dst_ptr).opaque = src.opaque;
            (*dst_ptr).quality = src.quality;
            (*dst_ptr).duration = src.duration;
            (*dst_ptr).sample_aspect_ratio = src.sample_aspect_ratio;
        }

        // 复制 side-data 与帧级元数据
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
        frame.format == self.get_format(true)
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

/// SAFETY:
/// - `AVHWDeviceContext` 底层是引用计数的 `AVBufferRef`；本类型的所有方法
///   （`setup_decoder_frames`/`setup_encoder_frames`/`hw_download`/`hw_upload`）
///   只做共享访问，涉及的
///   FFI 调用（`av_buffer_ref`/`av_hwframe_ctx_alloc`/`av_hwframe_get_buffer`/
///   `av_hwframe_transfer_data`）均为 FFmpeg 保证的线程安全操作。
/// - 硬件设备本身由驱动保证并发使用安全。
unsafe impl Send for HWContext {}
unsafe impl Sync for HWContext {}

ffi_enum_wrap_from!(
    /// Hardware device type (FFmpeg `AV_HWDEVICE_TYPE_*`).
    ///
    /// Generated from one `variant => constant` table with a two-way `From`. A value the table
    /// does not list panics instead of degrading to `NONE`: an unknown device type means the
    /// caller's assumption about the source is wrong, so it should fail fast rather than silently
    /// fall back to "no hardware device".
    ///
    /// `AV_HWDEVICE_TYPE_NONE` itself is a listed value, so it still converts to `NONE`.
    HWDeviceType => ffi::AVHWDeviceType,
    repr = u32,
    fallback = panic {
        /// ffi definition NONE: 0
        NONE => ffi::AV_HWDEVICE_TYPE_NONE;
        /// Video Decode and Presentation API for Unix (VDPAU)
        VDPAU => ffi::AV_HWDEVICE_TYPE_VDPAU;
        /// NVIDIA CUDA
        CUDA => ffi::AV_HWDEVICE_TYPE_CUDA;
        /// Video Acceleration API (VA-API)
        VAAPI => ffi::AV_HWDEVICE_TYPE_VAAPI;
        /// DirectX Video Acceleration 2.0
        DXVA2 => ffi::AV_HWDEVICE_TYPE_DXVA2;
        /// Quick Sync Video
        QSV => ffi::AV_HWDEVICE_TYPE_QSV;
        /// VideoToolbox
        VIDEOTOOLBOX => ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX;
        /// Direct3D 11 Video Acceleration
        D3D11VA => ffi::AV_HWDEVICE_TYPE_D3D11VA;
        /// Linux Direct Rendering Manager
        DRM => ffi::AV_HWDEVICE_TYPE_DRM;
        /// OpenCL
        OPENCL => ffi::AV_HWDEVICE_TYPE_OPENCL;
        /// MediaCodec
        MEDIACODEC => ffi::AV_HWDEVICE_TYPE_MEDIACODEC;
        /// Vulkan
        VULKAN => ffi::AV_HWDEVICE_TYPE_VULKAN;
        /// Direct3D 12 Video Acceleration
        #[cfg(any(feature = "ffmpeg7", feature = "ffmpeg8", feature = "ffmpeg9"))]
        D3D12VA => ffi::AV_HWDEVICE_TYPE_D3D12VA;
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        AMF => ffi::AV_HWDEVICE_TYPE_AMF;
        #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
        OHCODEC => ffi::AV_HWDEVICE_TYPE_OHCODEC;
    }
);

impl HWDeviceType {
    /// Whether or not the device type is available on this system.
    pub fn is_available(self) -> bool {
        Self::list_available().contains(&self)
    }

    /// 当前平台的硬件加速优先级（从高到低）。
    ///
    /// 排序依据与 FFmpeg CLI / 主流转码器的默认习惯一致：
    /// - macOS: VideoToolbox（Apple Silicon/Intel 均原生支持）
    /// - Windows: D3D11VA（承载 AMD AMF 及通用 D3D11 hwaccel）> QSV > CUDA > Vulkan
    /// - Linux: VAAPI（Intel/AMD 开箱即用）> CUDA > Vulkan
    /// - Android: MediaCodec
    pub fn platform_preference() -> Vec<HWDeviceType> {
        match std::env::consts::OS {
            "macos" => vec![HWDeviceType::VIDEOTOOLBOX, HWDeviceType::VULKAN],
            "windows" => vec![
                HWDeviceType::D3D11VA,
                HWDeviceType::QSV,
                HWDeviceType::CUDA,
                HWDeviceType::VULKAN,
                HWDeviceType::DXVA2,
            ],
            "linux" => vec![
                HWDeviceType::VAAPI,
                HWDeviceType::CUDA,
                HWDeviceType::VULKAN,
                HWDeviceType::VDPAU,
                HWDeviceType::OPENCL,
                HWDeviceType::DRM,
            ],
            "android" => vec![HWDeviceType::MEDIACODEC],
            _ => vec![],
        }
    }

    /// 平台自动选择：按 [`Self::platform_preference`]（或调用方自定义候选）
    /// 顺序探测，返回第一个可用设备的配置。
    ///
    /// # Arguments
    ///
    /// * `candidates` - 自定义候选顺序（空切片必报错）；`None` 使用平台默认优先级。
    pub fn auto_platform_config(candidates: Option<&[HWDeviceType]>) -> Result<HWDeviceConfig> {
        let preference: Vec<HWDeviceType> = match candidates {
            Some(list) => list.to_vec(),
            None => Self::platform_preference(),
        };
        if preference.is_empty() {
            return Err(RsmediaError::custom(format!(
                "No hardware acceleration preference defined for platform: {}",
                std::env::consts::OS
            )));
        }
        // 逐个候选探测可用性，避免只用首个候选的 available 集合去匹配其它候选，
        // 导致首个候选不可用但后续候选可用时误判为“无可用设备”。
        let device = preference
            .iter()
            .find(|ty| ty.is_available())
            .copied()
            .ok_or_else(|| {
                RsmediaError::custom(format!(
                    "No available hardware acceleration device on {} (candidates probed: {preference:?})",
                    std::env::consts::OS
                ))
            })?;
        log::info!("Auto-selected hardware device: {device:?}");
        Ok(HWDeviceConfig::new(
            device,
            device.default_hw_pixel_format(),
            device.default_sw_pixel_format(),
            None,
            None,
        ))
    }

    /// List available hardware acceleration device types on this system.
    ///
    /// Uses `av_hwdevice_iterate_types` internally.
    pub fn list_available() -> Vec<HWDeviceType> {
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
            #[cfg(any(feature = "ffmpeg7", feature = "ffmpeg8", feature = "ffmpeg9"))]
            HWDeviceType::D3D12VA => PixelFormat::D3D12,
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            HWDeviceType::AMF => PixelFormat::AMF_SURFACE,
            #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
            HWDeviceType::OHCODEC => PixelFormat::OHCODEC,
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
                    if hw_config_supports_codec
                        && HWDeviceType::from((*hw_config).device_type) == *self
                    {
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

/// `get_format` 回调：在解码器给出的候选像素格式列表中选择硬件表面格式。
///
/// 采用 FFmpeg 官方 `hw_decode` 示例的做法 —— 候选列表中带
/// `AV_PIX_FMT_FLAG_HWACCEL` 标志的格式即为本设备可用的硬件格式
/// （列表由解码器结合已设置的 `hw_device_ctx` / `hw_frames_ctx` 在
/// `avcodec_open2` 阶段生成）。因此**不需要**通过 `AVCodecContext.opaque`
/// 传递每实例数据：`opaque` 是 FFmpeg 留给应用层的私有字段，库占用会与
/// 用户代码互相破坏。
///
/// 该函数仅以函数指针形式安装到 codec context，无需导出符号，故不使用
/// `#[no_mangle]`，避免污染全局符号表。
unsafe extern "C" fn hwaccel_get_format(
    _ctx: *mut ffi::AVCodecContext,
    pix_fmts: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    unsafe {
        let mut p = pix_fmts;
        while *p != ffi::AV_PIX_FMT_NONE {
            if let Some(desc) = AVPixFmtDescriptorRef::get(*p)
                && (desc.flags & ffi::AV_PIX_FMT_FLAG_HWACCEL as u64) != 0
            {
                return *p;
            }
            p = p.add(1);
        }
        ffi::AV_PIX_FMT_NONE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 串行化所有触碰 `HW_CTX_CACHE` 的测试：缓存为进程级静态，lib 测试
    /// 共享同一进程，并行运行会破坏彼此的计数断言。
    static HW_CACHE_TEST_LOCK: Lazy<std::sync::Mutex<()>> = Lazy::new(|| std::sync::Mutex::new(()));

    /// 自动探测并创建硬件上下文。设备探测（`av_hwdevice_iterate_types`）
    /// 只覆盖 FFmpeg 的**编译期**支持，运行时可能仍打不开设备（如 CI 虚机
    /// 无 `/dev/dri` 渲染节点或 D3D11 适配器），创建失败视为"本机无可用
    /// GPU"，打印原因并返回 `None` 让调用方跳过而非 panic。
    fn try_auto_hw_context() -> Option<Arc<HWContext>> {
        let config = match HWDeviceConfig::auto_platform() {
            Ok(config) => config,
            Err(e) => {
                println!("skip: no hardware acceleration device probed: {e}");
                return None;
            }
        };
        println!("config: {config}");
        match HWContext::new(config) {
            Ok(ctx) => Some(ctx),
            Err(e) => {
                println!("skip: hardware device not usable on this machine: {e}");
                None
            }
        }
    }

    /// 空缓存清理应返回 0 且不 panic（CI / 无 GPU 环境）。
    /// 有 GPU 时：创建 context → 释放引用 → 清理应移除该条目。
    #[test]
    fn test_clear_hw_ctx_cache() {
        let _guard = HW_CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        // 基线：先清理其它测试可能遗留的条目，再验证空缓存清理返回 0
        let _leftovers = clear_hw_ctx_cache();
        let removed = clear_hw_ctx_cache();
        assert_eq!(removed, 0, "empty cache should remove nothing");

        // 若本机有可用 GPU 设备：创建后释放，缓存条目应可被清理
        let Some(ctx) = try_auto_hw_context() else {
            return;
        };
        // 持有期间清理不应移除
        assert_eq!(clear_hw_ctx_cache(), 0);
        drop(ctx);
        // 引用释放后（仅缓存持有），清理应移除该条目
        assert_eq!(clear_hw_ctx_cache(), 1);
        // 再次清理：缓存已空
        assert_eq!(clear_hw_ctx_cache(), 0);
    }

    /// `HWContext` 跨线程共享同一 Arc 不应触发数据竞争（回归测试：
    /// setup_decoder_frames 并发调用曾通过 UnsafeCell 做可变访问）。
    #[test]
    fn test_hw_context_shared_across_threads() {
        let _guard = HW_CACHE_TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());

        let Some(ctx) = try_auto_hw_context() else {
            return; // 无 GPU 环境跳过
        };
        let ctx2 = Arc::clone(&ctx);
        let handle = std::thread::spawn(move || {
            // 共享引用上的只读方法跨线程调用
            let _ = ctx2.get_format(true);
        });
        let _ = ctx.get_format(false);
        handle.join().expect("thread should not panic");
    }

    /// 所有变体与 ffi 值双向映射后应保持自身（编译期常量，跨平台一致）。
    #[test]
    fn test_hw_device_type_roundtrip() {
        let variants = [
            HWDeviceType::NONE,
            HWDeviceType::VDPAU,
            HWDeviceType::CUDA,
            HWDeviceType::VAAPI,
            HWDeviceType::DXVA2,
            HWDeviceType::QSV,
            HWDeviceType::VIDEOTOOLBOX,
            HWDeviceType::D3D11VA,
            HWDeviceType::DRM,
            HWDeviceType::OPENCL,
            HWDeviceType::MEDIACODEC,
            HWDeviceType::VULKAN,
        ];
        for v in variants {
            let ffi_value: ffi::AVHWDeviceType = v.into();
            assert_eq!(
                HWDeviceType::from(ffi_value),
                v,
                "roundtrip failed for {v:?}"
            );
        }
    }

    /// 平台优先级：每个已知平台都应定义非空列表，且首选设备符合平台惯例。
    #[test]
    fn test_platform_preference() {
        let preference = HWDeviceType::platform_preference();
        match std::env::consts::OS {
            "macos" => {
                assert_eq!(preference[0], HWDeviceType::VIDEOTOOLBOX);
            }
            "windows" => {
                assert_eq!(preference[0], HWDeviceType::D3D11VA);
                assert!(preference.contains(&HWDeviceType::QSV));
            }
            "linux" => {
                assert_eq!(preference[0], HWDeviceType::VAAPI);
                assert!(preference.contains(&HWDeviceType::CUDA));
            }
            "android" => {
                assert_eq!(preference, vec![HWDeviceType::MEDIACODEC]);
            }
            _ => {}
        }
    }

    /// `list_available` 与 `is_available` 的一致性；探测过程不应 panic。
    #[test]
    fn test_list_available_consistency() {
        let variants = [
            HWDeviceType::CUDA,
            HWDeviceType::VAAPI,
            HWDeviceType::VIDEOTOOLBOX,
            HWDeviceType::D3D11VA,
            HWDeviceType::VULKAN,
        ];
        for v in variants {
            assert_eq!(
                v.is_available(),
                HWDeviceType::list_available().contains(&v)
            );
        }
    }

    /// 平台自动选择：有可用设备时返回平台优先级内的配置；无设备时返回
    /// 描述性错误（CI / 无 GPU 环境），不应 panic。
    #[test]
    fn test_auto_platform() {
        match HWDeviceConfig::auto_platform() {
            Ok(config) => {
                assert!(
                    HWDeviceType::platform_preference().contains(&config.device_type),
                    "auto-selected device {:?} not in platform preference",
                    config.device_type
                );
            }
            Err(err) => {
                let message = format!("{err:#}");
                assert!(
                    message.contains("No available hardware acceleration"),
                    "unexpected error: {message}"
                );
            }
        }
    }

    /// 自定义候选：只允许 D3D11VA（AMD AMF 的承载设备类型）。
    /// 可用时应给出 D3D11 硬件格式 + NV12 软件格式；不可用时应优雅报错。
    #[test]
    fn test_auto_platform_amf_candidate() {
        match HWDeviceConfig::auto_platform_with(&[HWDeviceType::D3D11VA]) {
            Ok(config) => {
                assert_eq!(config.device_type, HWDeviceType::D3D11VA);
                assert_eq!(config.hw_pixel_format, PixelFormat::D3D11);
                assert_eq!(config.sw_pixel_format, PixelFormat::NV12);
            }
            Err(err) => {
                let message = format!("{err:#}");
                assert!(
                    message.contains("No available hardware acceleration"),
                    "unexpected error: {message}"
                );
            }
        }
    }

    /// 空候选列表应报错而不是 panic（未知平台 / 显式空列表）。
    #[test]
    fn test_auto_platform_empty_candidates() {
        let result = HWDeviceConfig::auto_platform_with(&[]);
        assert!(result.is_err());
    }

    /// AMF builder 仅在 Windows 上编译：验证字段映射（D3D11VA 设备 + D3D11
    /// 硬件格式 + NV12 软件格式）。
    #[cfg(target_os = "windows")]
    #[test]
    fn test_amf_config_builder() {
        let config = HWDeviceConfig::amf(Some("0".to_string()));
        assert_eq!(config.device_type, HWDeviceType::D3D11VA);
        assert_eq!(config.hw_pixel_format, PixelFormat::D3D11);
        assert_eq!(config.sw_pixel_format, PixelFormat::NV12);
        assert_eq!(config.device_id.as_deref(), Some("0"));
    }
}
