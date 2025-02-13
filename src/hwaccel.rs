use crate::error::MediaError;
use crate::ffi_hwaccel;
use rsmpeg::avcodec::AVCodec;
use rsmpeg::avcodec::AVCodecContext;
use rsmpeg::avutil::AVPixelFormat;
use rsmpeg::ffi;
use std::ops::Deref;

type Result<T> = std::result::Result<T, MediaError>;

pub(crate) struct HWContext {
    pixel_format: AVPixelFormat,
    _hw_device_context: ffi_hwaccel::HWDeviceContext,
}

impl HWContext {
    pub(crate) fn new(decoder: &mut AVCodecContext, device_type: HWDeviceType) -> Result<Self> {
        let codec =
            AVCodec::find_encoder(decoder.codec_id).ok_or(MediaError::UninitializedCodec)?;
        let pixel_format = ffi_hwaccel::codec_find_hwaccel_pixfmt(codec.deref(), device_type)
            .ok_or(MediaError::UnsupportedCodecHWDeviceType)?;

        ffi_hwaccel::codec_context_hwaccel_set_get_format(decoder, pixel_format);

        let hardware_device_context = ffi_hwaccel::HWDeviceContext::new(device_type)?;
        ffi_hwaccel::codec_context_hwaccel_set_hw_device_ctx(decoder, &hardware_device_context);

        Ok(HWContext {
            pixel_format,
            _hw_device_context: hardware_device_context,
        })
    }

    pub(crate) fn format(&self) -> AVPixelFormat {
        self.pixel_format
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum HWDeviceType {
    /// Video Decode and Presentation API for Unix (VDPAU)
    Vdpau,
    /// NVIDIA CUDA
    Cuda,
    /// Video Acceleration API (VA-API)
    VaApi,
    /// DirectX Video Acceleration 2.0
    Dxva2,
    /// Quick Sync Video
    Qsv,
    /// VideoToolbox
    VideoToolbox,
    /// Direct3D 11 Video Acceleration
    D3D11Va,
    /// Linux Direct Rendering Manager
    Drm,
    /// OpenCL
    OpenCl,
    /// MediaCodec
    MediaCodec,
    /// Vulkan
    Vulkan,
    /// Direct3D 12 Video Acceleration
    #[cfg(feature = "ffmpeg7")]
    D3D12Va,
}

impl HWDeviceType {
    /// Whether or not the device type is available on this system.
    pub fn is_available(self) -> bool {
        Self::list_available().contains(&self)
    }

    /// List available hardware acceleration device types on this system.
    ///
    /// Uses `av_hwdevice_iterate_types` internally.
    pub fn list_available() -> Vec<HWDeviceType> {
        ffi_hwaccel::hwdevice_list_available_device_types()
    }
}

impl HWDeviceType {
    pub fn from(value: ffi::AVHWDeviceType) -> Option<HWDeviceType> {
        match value {
            ffi::AV_HWDEVICE_TYPE_NONE => None,
            ffi::AV_HWDEVICE_TYPE_VDPAU => Some(Self::Vdpau),
            ffi::AV_HWDEVICE_TYPE_CUDA => Some(Self::Cuda),
            ffi::AV_HWDEVICE_TYPE_VAAPI => Some(Self::VaApi),
            ffi::AV_HWDEVICE_TYPE_DXVA2 => Some(Self::Dxva2),
            ffi::AV_HWDEVICE_TYPE_QSV => Some(Self::Qsv),
            ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX => Some(Self::VideoToolbox),
            ffi::AV_HWDEVICE_TYPE_D3D11VA => Some(Self::D3D11Va),
            ffi::AV_HWDEVICE_TYPE_DRM => Some(Self::Drm),
            ffi::AV_HWDEVICE_TYPE_OPENCL => Some(Self::OpenCl),
            ffi::AV_HWDEVICE_TYPE_MEDIACODEC => Some(Self::MediaCodec),
            ffi::AV_HWDEVICE_TYPE_VULKAN => Some(Self::Vulkan),
            #[cfg(feature = "ffmpeg7")]
            ffi::AV_HWDEVICE_TYPE_D3D12VA => Some(Self::D3D12Va),

            #[allow(unreachable_patterns)]
            _ => unimplemented!(),
        }
    }
}

impl From<HWDeviceType> for ffi::AVHWDeviceType {
    fn from(value: HWDeviceType) -> Self {
        match value {
            HWDeviceType::Vdpau => ffi::AV_HWDEVICE_TYPE_VDPAU,
            HWDeviceType::Cuda => ffi::AV_HWDEVICE_TYPE_CUDA,
            HWDeviceType::VaApi => ffi::AV_HWDEVICE_TYPE_VAAPI,
            HWDeviceType::Dxva2 => ffi::AV_HWDEVICE_TYPE_DXVA2,
            HWDeviceType::Qsv => ffi::AV_HWDEVICE_TYPE_QSV,
            HWDeviceType::VideoToolbox => ffi::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
            HWDeviceType::D3D11Va => ffi::AV_HWDEVICE_TYPE_D3D11VA,
            HWDeviceType::Drm => ffi::AV_HWDEVICE_TYPE_DRM,
            HWDeviceType::OpenCl => ffi::AV_HWDEVICE_TYPE_OPENCL,
            HWDeviceType::MediaCodec => ffi::AV_HWDEVICE_TYPE_MEDIACODEC,
            HWDeviceType::Vulkan => ffi::AV_HWDEVICE_TYPE_VULKAN,
            #[cfg(feature = "ffmpeg7")]
            HWDeviceType::D3D12Va => ffi::AV_HWDEVICE_TYPE_D3D12VA,
        }
    }
}
