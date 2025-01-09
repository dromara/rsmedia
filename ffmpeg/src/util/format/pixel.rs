use std::error;
use std::ffi::{CStr, CString, NulError};
use std::fmt;

use rsmpeg::ffi;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub enum Pixel {
    None,

    YUV420P,
    YUYV422,
    RGB24,
    BGR24,
    YUV422P,
    YUV444P,
    YUV410P,
    YUV411P,
    GRAY8,
    MonoWhite,
    MonoBlack,
    PAL8,
    YUVJ420P,
    YUVJ422P,
    YUVJ444P,
    // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
    XVMC_MPEG2_MC,
    // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
    XVMC_MPEG2_IDCT,
    UYVY422,
    UYYVYY411,
    BGR8,
    BGR4,
    BGR4_BYTE,
    RGB8,
    RGB4,
    RGB4_BYTE,
    NV12,
    NV21,

    ARGB,
    RGBA,
    ABGR,
    BGRA,

    GRAY16BE,
    GRAY16LE,
    YUV440P,
    YUVJ440P,
    YUVA420P,
    // #[cfg(feature = "ff_api_vdpau")]
    VDPAU_H264,
    // #[cfg(feature = "ff_api_vdpau")]
    VDPAU_MPEG1,
    // #[cfg(feature = "ff_api_vdpau")]
    VDPAU_MPEG2,
    // #[cfg(feature = "ff_api_vdpau")]
    VDPAU_WMV3,
    // #[cfg(feature = "ff_api_vdpau")]
    VDPAU_VC1,
    RGB48BE,
    RGB48LE,

    RGB565BE,
    RGB565LE,
    RGB555BE,
    RGB555LE,

    BGR565BE,
    BGR565LE,
    BGR555BE,
    BGR555LE,

    // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
    VAAPI_MOCO,
    // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
    VAAPI_IDCT,
    // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
    VAAPI_VLD,
    // #[cfg(any(not(feature = "ff_api_vaapi"), feature = "ffmpeg_5_0"))]
    VAAPI,

    YUV420P16LE,
    YUV420P16BE,
    YUV422P16LE,
    YUV422P16BE,
    YUV444P16LE,
    YUV444P16BE,
    // #[cfg(feature = "ff_api_vdpau")]
    VDPAU_MPEG4,
    DXVA2_VLD,

    RGB444LE,
    RGB444BE,
    BGR444LE,
    BGR444BE,
    YA8,

    BGR48BE,
    BGR48LE,

    YUV420P9BE,
    YUV420P9LE,
    YUV420P10BE,
    YUV420P10LE,
    YUV422P10BE,
    YUV422P10LE,
    YUV444P9BE,
    YUV444P9LE,
    YUV444P10BE,
    YUV444P10LE,
    YUV422P9BE,
    YUV422P9LE,
    // #[cfg(not(feature = "ffmpeg_4_0"))]
    VDA_VLD,

    GBRP,
    GBRP9BE,
    GBRP9LE,
    GBRP10BE,
    GBRP10LE,
    GBRP16BE,
    GBRP16LE,

    YUVA420P9BE,
    YUVA420P9LE,
    YUVA422P9BE,
    YUVA422P9LE,
    YUVA444P9BE,
    YUVA444P9LE,
    YUVA420P10BE,
    YUVA420P10LE,
    YUVA422P10BE,
    YUVA422P10LE,
    YUVA444P10BE,
    YUVA444P10LE,
    YUVA420P16BE,
    YUVA420P16LE,
    YUVA422P16BE,
    YUVA422P16LE,
    YUVA444P16BE,
    YUVA444P16LE,

    VDPAU,

    XYZ12LE,
    XYZ12BE,
    NV16,
    NV20LE,
    NV20BE,

    RGBA64BE,
    RGBA64LE,
    BGRA64BE,
    BGRA64LE,

    YVYU422,

    // #[cfg(not(feature = "ffmpeg_4_0"))]
    VDA,

    YA16BE,
    YA16LE,

    QSV,
    MMAL,

    D3D11VA_VLD,

    CUDA,

    ZRGB,
    RGBZ,
    ZBGR,
    BGRZ,
    YUVA444P,
    YUVA422P,

    YUV420P12BE,
    YUV420P12LE,
    YUV420P14BE,
    YUV420P14LE,
    YUV422P12BE,
    YUV422P12LE,
    YUV422P14BE,
    YUV422P14LE,
    YUV444P12BE,
    YUV444P12LE,
    YUV444P14BE,
    YUV444P14LE,
    GBRP12BE,
    GBRP12LE,
    GBRP14BE,
    GBRP14LE,
    GBRAP,
    GBRAP16BE,
    GBRAP16LE,
    YUVJ411P,

    BAYER_BGGR8,
    BAYER_RGGB8,
    BAYER_GBRG8,
    BAYER_GRBG8,
    BAYER_BGGR16LE,
    BAYER_BGGR16BE,
    BAYER_RGGB16LE,
    BAYER_RGGB16BE,
    BAYER_GBRG16LE,
    BAYER_GBRG16BE,
    BAYER_GRBG16LE,
    BAYER_GRBG16BE,

    YUV440P10LE,
    YUV440P10BE,
    YUV440P12LE,
    YUV440P12BE,
    AYUV64LE,
    AYUV64BE,

    VIDEOTOOLBOX,

    // --- defaults
    // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg7")))]
    XVMC,

    RGB32,
    RGB32_1,
    BGR32,
    BGR32_1,
    ZRGB32,
    ZBGR32,

    GRAY16,
    YA16,
    RGB48,
    RGB565,
    RGB555,
    RGB444,
    BGR48,
    BGR565,
    BGR555,
    BGR444,

    YUV420P9,
    YUV422P9,
    YUV444P9,
    YUV420P10,
    YUV422P10,
    YUV440P10,
    YUV444P10,
    YUV420P12,
    YUV422P12,
    YUV440P12,
    YUV444P12,
    YUV420P14,
    YUV422P14,
    YUV444P14,
    YUV420P16,
    YUV422P16,
    YUV444P16,

    GBRP9,
    GBRP10,
    GBRP12,
    GBRP14,
    GBRP16,
    GBRAP16,

    BAYER_BGGR16,
    BAYER_RGGB16,
    BAYER_GBRG16,
    BAYER_GRBG16,

    YUVA420P9,
    YUVA422P9,
    YUVA444P9,
    YUVA420P10,
    YUVA422P10,
    YUVA444P10,
    YUVA420P16,
    YUVA422P16,
    YUVA444P16,

    XYZ12,
    NV20,
    AYUV64,

    P010LE,
    P010BE,
    GBRAP12BE,
    GBRAP12LE,
    GBRAP10LE,
    GBRAP10BE,
    MEDIACODEC,
    GRAY12BE,
    GRAY12LE,
    GRAY10BE,
    GRAY10LE,
    P016LE,
    P016BE,

    D3D11,
    GRAY9BE,
    GRAY9LE,
    GBRPF32BE,
    GBRPF32LE,
    GBRAPF32BE,
    GBRAPF32LE,
    DRM_PRIME,

    // #[cfg(feature = "ffmpeg_4_0")]
    OPENCL,

    // #[cfg(feature = "ffmpeg_4_1")]
    GRAY14BE,
    // #[cfg(feature = "ffmpeg_4_1")]
    GRAY14LE,
    // #[cfg(feature = "ffmpeg_4_1")]
    GRAYF32BE,
    // #[cfg(feature = "ffmpeg_4_1")]
    GRAYF32LE,

    // #[cfg(feature = "ffmpeg_4_2")]
    YUVA422P12BE,
    // #[cfg(feature = "ffmpeg_4_2")]
    YUVA422P12LE,
    // #[cfg(feature = "ffmpeg_4_2")]
    YUVA444P12BE,
    // #[cfg(feature = "ffmpeg_4_2")]
    YUVA444P12LE,
    // #[cfg(feature = "ffmpeg_4_2")]
    NV24,
    // #[cfg(feature = "ffmpeg_4_2")]
    NV42,

    // #[cfg(feature = "ffmpeg_4_3")]
    VULKAN,
    // #[cfg(feature = "ffmpeg_4_3")]
    Y210BE,
    // #[cfg(feature = "ffmpeg_4_3")]
    Y210LE,

    // #[cfg(feature = "ffmpeg_4_4")]
    X2RGB10LE,
    // #[cfg(feature = "ffmpeg_4_4")]
    X2RGB10BE,

    // #[cfg(feature = "ffmpeg_5_0")]
    X2BGR10LE,
    // #[cfg(feature = "ffmpeg_5_0")]
    X2BGR10BE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P210BE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P210LE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P410BE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P410LE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P216BE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P216LE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P416BE,
    // #[cfg(feature = "ffmpeg_5_0")]
    P416LE,

    #[cfg(feature = "ffmpeg6")]
    VUYA,
    #[cfg(feature = "ffmpeg6")]
    RGBAF16BE,
    #[cfg(feature = "ffmpeg6")]
    RGBAF16LE,
    #[cfg(feature = "ffmpeg6")]
    VUYX,
    #[cfg(feature = "ffmpeg6")]
    P012LE,
    #[cfg(feature = "ffmpeg6")]
    P012BE,
    #[cfg(feature = "ffmpeg6")]
    Y212BE,
    #[cfg(feature = "ffmpeg6")]
    Y212LE,
    #[cfg(feature = "ffmpeg6")]
    XV30BE,
    #[cfg(feature = "ffmpeg6")]
    XV30LE,
    #[cfg(feature = "ffmpeg6")]
    XV36BE,
    #[cfg(feature = "ffmpeg6")]
    XV36LE,
    #[cfg(feature = "ffmpeg6")]
    RGBF32BE,
    #[cfg(feature = "ffmpeg6")]
    RGBF32LE,
    #[cfg(feature = "ffmpeg6")]
    RGBAF32BE,
    #[cfg(feature = "ffmpeg6")]
    RGBAF32LE,

    #[cfg(feature = "ffmpeg6")]
    P212BE,
    #[cfg(feature = "ffmpeg6")]
    P212LE,
    #[cfg(feature = "ffmpeg6")]
    P412BE,
    #[cfg(feature = "ffmpeg6")]
    P412LE,
    #[cfg(feature = "ffmpeg6")]
    GBRAP14BE,
    #[cfg(feature = "ffmpeg6")]
    GBRAP14LE,

    #[cfg(feature = "ffmpeg7")]
    D3D12,

    // #[cfg(feature = "rpi")]
    SAND128,
    // #[cfg(feature = "rpi")]
    SAND64_10,
    // #[cfg(feature = "rpi")]
    SAND64_16,
    // #[cfg(feature = "rpi")]
    RPI4_8,
    // #[cfg(feature = "rpi")]
    RPI4_10,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Descriptor {
    ptr: *const ffi::AVPixFmtDescriptor,
}

unsafe impl Send for Descriptor {}
unsafe impl Sync for Descriptor {}

impl Pixel {
    pub const Y400A: Pixel = Pixel::YA8;
    pub const GRAY8A: Pixel = Pixel::YA8;
    pub const GBR24P: Pixel = Pixel::GBRP;
    // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
    pub const XVMC: Pixel = Pixel::XVMC_MPEG2_IDCT;

    pub fn descriptor(self) -> Option<Descriptor> {
        unsafe {
            let ptr = ffi::av_pix_fmt_desc_get(self.into());

            ptr.as_ref().map(|ptr| Descriptor { ptr })
        }
    }
}

impl Descriptor {
    pub fn as_ptr(self) -> *const ffi::AVPixFmtDescriptor {
        self.ptr
    }

    pub fn name(self) -> &'static str {
        unsafe { std::str::from_utf8_unchecked(CStr::from_ptr((*self.as_ptr()).name).to_bytes()) }
    }

    pub fn nb_components(self) -> u8 {
        unsafe { (*self.as_ptr()).nb_components }
    }

    pub fn log2_chroma_w(self) -> u8 {
        unsafe { (*self.as_ptr()).log2_chroma_w }
    }

    pub fn log2_chroma_h(self) -> u8 {
        unsafe { (*self.as_ptr()).log2_chroma_h }
    }
}

impl From<ffi::AVPixelFormat> for Pixel {
    #[inline]
    fn from(value: ffi::AVPixelFormat) -> Self {
        match value {
            ffi::AV_PIX_FMT_NONE => Pixel::None,

            ffi::AV_PIX_FMT_YUV420P => Pixel::YUV420P,
            ffi::AV_PIX_FMT_YUYV422 => Pixel::YUYV422,
            ffi::AV_PIX_FMT_RGB24 => Pixel::RGB24,
            ffi::AV_PIX_FMT_BGR24 => Pixel::BGR24,
            ffi::AV_PIX_FMT_YUV422P => Pixel::YUV422P,
            ffi::AV_PIX_FMT_YUV444P => Pixel::YUV444P,
            ffi::AV_PIX_FMT_YUV410P => Pixel::YUV410P,
            ffi::AV_PIX_FMT_YUV411P => Pixel::YUV411P,
            ffi::AV_PIX_FMT_GRAY8 => Pixel::GRAY8,
            ffi::AV_PIX_FMT_MONOWHITE => Pixel::MonoWhite,
            ffi::AV_PIX_FMT_MONOBLACK => Pixel::MonoBlack,
            ffi::AV_PIX_FMT_PAL8 => Pixel::PAL8,
            ffi::AV_PIX_FMT_YUVJ420P => Pixel::YUVJ420P,
            ffi::AV_PIX_FMT_YUVJ422P => Pixel::YUVJ422P,
            ffi::AV_PIX_FMT_YUVJ444P => Pixel::YUVJ444P,
            // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg7")))]
            // ffi::AV_PIX_FMT_XVMC => Pixel::XVMC,
            // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_PIX_FMT_XVMC_MPEG2_MC => Pixel::XVMC_MPEG2_MC,
            // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_PIX_FMT_XVMC_MPEG2_IDCT => Pixel::XVMC_MPEG2_IDCT,
            ffi::AV_PIX_FMT_UYVY422 => Pixel::UYVY422,
            ffi::AV_PIX_FMT_UYYVYY411 => Pixel::UYYVYY411,
            ffi::AV_PIX_FMT_BGR8 => Pixel::BGR8,
            ffi::AV_PIX_FMT_BGR4 => Pixel::BGR4,
            ffi::AV_PIX_FMT_BGR4_BYTE => Pixel::BGR4_BYTE,
            ffi::AV_PIX_FMT_RGB8 => Pixel::RGB8,
            ffi::AV_PIX_FMT_RGB4 => Pixel::RGB4,
            ffi::AV_PIX_FMT_RGB4_BYTE => Pixel::RGB4_BYTE,
            ffi::AV_PIX_FMT_NV12 => Pixel::NV12,
            ffi::AV_PIX_FMT_NV21 => Pixel::NV21,

            ffi::AV_PIX_FMT_ARGB => Pixel::ARGB,
            ffi::AV_PIX_FMT_RGBA => Pixel::RGBA,
            ffi::AV_PIX_FMT_ABGR => Pixel::ABGR,
            ffi::AV_PIX_FMT_BGRA => Pixel::BGRA,

            ffi::AV_PIX_FMT_GRAY16BE => Pixel::GRAY16BE,
            ffi::AV_PIX_FMT_GRAY16LE => Pixel::GRAY16LE,
            ffi::AV_PIX_FMT_YUV440P => Pixel::YUV440P,
            ffi::AV_PIX_FMT_YUVJ440P => Pixel::YUVJ440P,
            ffi::AV_PIX_FMT_YUVA420P => Pixel::YUVA420P,
            // #[cfg(feature = "ff_api_vdpau")]
            // ffi::AV_PIX_FMT_VDPAU_H264 => Pixel::VDPAU_H264,
            // #[cfg(feature = "ff_api_vdpau")]
            // ffi::AV_PIX_FMT_VDPAU_MPEG1 => Pixel::VDPAU_MPEG1,
            // #[cfg(feature = "ff_api_vdpau")]
            // ffi::AV_PIX_FMT_VDPAU_MPEG2 => Pixel::VDPAU_MPEG2,
            // #[cfg(feature = "ff_api_vdpau")]
            // ffi::AV_PIX_FMT_VDPAU_WMV3 => Pixel::VDPAU_WMV3,
            // #[cfg(feature = "ff_api_vdpau")]
            // ffi::AV_PIX_FMT_VDPAU_VC1 => Pixel::VDPAU_VC1,
            ffi::AV_PIX_FMT_RGB48BE => Pixel::RGB48BE,
            ffi::AV_PIX_FMT_RGB48LE => Pixel::RGB48LE,

            ffi::AV_PIX_FMT_RGB565BE => Pixel::RGB565BE,
            ffi::AV_PIX_FMT_RGB565LE => Pixel::RGB565LE,
            ffi::AV_PIX_FMT_RGB555BE => Pixel::RGB555BE,
            ffi::AV_PIX_FMT_RGB555LE => Pixel::RGB555LE,

            ffi::AV_PIX_FMT_BGR565BE => Pixel::BGR565BE,
            ffi::AV_PIX_FMT_BGR565LE => Pixel::BGR565LE,
            ffi::AV_PIX_FMT_BGR555BE => Pixel::BGR555BE,
            ffi::AV_PIX_FMT_BGR555LE => Pixel::BGR555LE,

            // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_PIX_FMT_VAAPI_MOCO => Pixel::VAAPI_MOCO,
            // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_PIX_FMT_VAAPI_IDCT => Pixel::VAAPI_IDCT,
            // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
            // ffi::AV_PIX_FMT_VAAPI_VLD => Pixel::VAAPI_VLD,
            // #[cfg(any(not(feature = "ff_api_vaapi"), feature = "ffmpeg_5_0"))]
            // ffi::AV_PIX_FMT_VAAPI => Pixel::VAAPI,
            ffi::AV_PIX_FMT_YUV420P16LE => Pixel::YUV420P16LE,
            ffi::AV_PIX_FMT_YUV420P16BE => Pixel::YUV420P16BE,
            ffi::AV_PIX_FMT_YUV422P16LE => Pixel::YUV422P16LE,
            ffi::AV_PIX_FMT_YUV422P16BE => Pixel::YUV422P16BE,
            ffi::AV_PIX_FMT_YUV444P16LE => Pixel::YUV444P16LE,
            ffi::AV_PIX_FMT_YUV444P16BE => Pixel::YUV444P16BE,
            // #[cfg(feature = "ff_api_vdpau")]
            // ffi::AV_PIX_FMT_VDPAU_MPEG4 => Pixel::VDPAU_MPEG4,
            ffi::AV_PIX_FMT_DXVA2_VLD => Pixel::DXVA2_VLD,

            ffi::AV_PIX_FMT_RGB444LE => Pixel::RGB444LE,
            ffi::AV_PIX_FMT_RGB444BE => Pixel::RGB444BE,
            ffi::AV_PIX_FMT_BGR444LE => Pixel::BGR444LE,
            ffi::AV_PIX_FMT_BGR444BE => Pixel::BGR444BE,
            ffi::AV_PIX_FMT_YA8 => Pixel::YA8,

            ffi::AV_PIX_FMT_BGR48BE => Pixel::BGR48BE,
            ffi::AV_PIX_FMT_BGR48LE => Pixel::BGR48LE,

            ffi::AV_PIX_FMT_YUV420P9BE => Pixel::YUV420P9BE,
            ffi::AV_PIX_FMT_YUV420P9LE => Pixel::YUV420P9LE,
            ffi::AV_PIX_FMT_YUV420P10BE => Pixel::YUV420P10BE,
            ffi::AV_PIX_FMT_YUV420P10LE => Pixel::YUV420P10LE,
            ffi::AV_PIX_FMT_YUV422P10BE => Pixel::YUV422P10BE,
            ffi::AV_PIX_FMT_YUV422P10LE => Pixel::YUV422P10LE,
            ffi::AV_PIX_FMT_YUV444P9BE => Pixel::YUV444P9BE,
            ffi::AV_PIX_FMT_YUV444P9LE => Pixel::YUV444P9LE,
            ffi::AV_PIX_FMT_YUV444P10BE => Pixel::YUV444P10BE,
            ffi::AV_PIX_FMT_YUV444P10LE => Pixel::YUV444P10LE,
            ffi::AV_PIX_FMT_YUV422P9BE => Pixel::YUV422P9BE,
            ffi::AV_PIX_FMT_YUV422P9LE => Pixel::YUV422P9LE,
            // #[cfg(not(feature = "ffmpeg_4_0"))]
            // ffi::AV_PIX_FMT_VDA_VLD => Pixel::VDA_VLD,
            ffi::AV_PIX_FMT_GBRP => Pixel::GBRP,
            ffi::AV_PIX_FMT_GBRP9BE => Pixel::GBRP9BE,
            ffi::AV_PIX_FMT_GBRP9LE => Pixel::GBRP9LE,
            ffi::AV_PIX_FMT_GBRP10BE => Pixel::GBRP10BE,
            ffi::AV_PIX_FMT_GBRP10LE => Pixel::GBRP10LE,
            ffi::AV_PIX_FMT_GBRP16BE => Pixel::GBRP16BE,
            ffi::AV_PIX_FMT_GBRP16LE => Pixel::GBRP16LE,

            ffi::AV_PIX_FMT_YUVA420P9BE => Pixel::YUVA420P9BE,
            ffi::AV_PIX_FMT_YUVA420P9LE => Pixel::YUVA420P9LE,
            ffi::AV_PIX_FMT_YUVA422P9BE => Pixel::YUVA422P9BE,
            ffi::AV_PIX_FMT_YUVA422P9LE => Pixel::YUVA422P9LE,
            ffi::AV_PIX_FMT_YUVA444P9BE => Pixel::YUVA444P9BE,
            ffi::AV_PIX_FMT_YUVA444P9LE => Pixel::YUVA444P9LE,
            ffi::AV_PIX_FMT_YUVA420P10BE => Pixel::YUVA420P10BE,
            ffi::AV_PIX_FMT_YUVA420P10LE => Pixel::YUVA420P10LE,
            ffi::AV_PIX_FMT_YUVA422P10BE => Pixel::YUVA422P10BE,
            ffi::AV_PIX_FMT_YUVA422P10LE => Pixel::YUVA422P10LE,
            ffi::AV_PIX_FMT_YUVA444P10BE => Pixel::YUVA444P10BE,
            ffi::AV_PIX_FMT_YUVA444P10LE => Pixel::YUVA444P10LE,
            ffi::AV_PIX_FMT_YUVA420P16BE => Pixel::YUVA420P16BE,
            ffi::AV_PIX_FMT_YUVA420P16LE => Pixel::YUVA420P16LE,
            ffi::AV_PIX_FMT_YUVA422P16BE => Pixel::YUVA422P16BE,
            ffi::AV_PIX_FMT_YUVA422P16LE => Pixel::YUVA422P16LE,
            ffi::AV_PIX_FMT_YUVA444P16BE => Pixel::YUVA444P16BE,
            ffi::AV_PIX_FMT_YUVA444P16LE => Pixel::YUVA444P16LE,

            ffi::AV_PIX_FMT_VDPAU => Pixel::VDPAU,

            ffi::AV_PIX_FMT_XYZ12LE => Pixel::XYZ12LE,
            ffi::AV_PIX_FMT_XYZ12BE => Pixel::XYZ12BE,
            ffi::AV_PIX_FMT_NV16 => Pixel::NV16,
            ffi::AV_PIX_FMT_NV20LE => Pixel::NV20LE,
            ffi::AV_PIX_FMT_NV20BE => Pixel::NV20BE,

            ffi::AV_PIX_FMT_RGBA64BE => Pixel::RGBA64BE,
            ffi::AV_PIX_FMT_RGBA64LE => Pixel::RGBA64LE,
            ffi::AV_PIX_FMT_BGRA64BE => Pixel::BGRA64BE,
            ffi::AV_PIX_FMT_BGRA64LE => Pixel::BGRA64LE,

            ffi::AV_PIX_FMT_YVYU422 => Pixel::YVYU422,

            // #[cfg(not(feature = "ffmpeg_4_0"))]
            // ffi::AV_PIX_FMT_VDA => Pixel::VDA,
            ffi::AV_PIX_FMT_YA16BE => Pixel::YA16BE,
            ffi::AV_PIX_FMT_YA16LE => Pixel::YA16LE,

            ffi::AV_PIX_FMT_QSV => Pixel::QSV,
            ffi::AV_PIX_FMT_MMAL => Pixel::MMAL,

            ffi::AV_PIX_FMT_D3D11VA_VLD => Pixel::D3D11VA_VLD,

            ffi::AV_PIX_FMT_CUDA => Pixel::CUDA,

            ffi::AV_PIX_FMT_0RGB => Pixel::ZRGB,
            ffi::AV_PIX_FMT_RGB0 => Pixel::RGBZ,
            ffi::AV_PIX_FMT_0BGR => Pixel::ZBGR,
            ffi::AV_PIX_FMT_BGR0 => Pixel::BGRZ,
            ffi::AV_PIX_FMT_YUVA444P => Pixel::YUVA444P,
            ffi::AV_PIX_FMT_YUVA422P => Pixel::YUVA422P,

            ffi::AV_PIX_FMT_YUV420P12BE => Pixel::YUV420P12BE,
            ffi::AV_PIX_FMT_YUV420P12LE => Pixel::YUV420P12LE,
            ffi::AV_PIX_FMT_YUV420P14BE => Pixel::YUV420P14BE,
            ffi::AV_PIX_FMT_YUV420P14LE => Pixel::YUV420P14LE,
            ffi::AV_PIX_FMT_YUV422P12BE => Pixel::YUV422P12BE,
            ffi::AV_PIX_FMT_YUV422P12LE => Pixel::YUV422P12LE,
            ffi::AV_PIX_FMT_YUV422P14BE => Pixel::YUV422P14BE,
            ffi::AV_PIX_FMT_YUV422P14LE => Pixel::YUV422P14LE,
            ffi::AV_PIX_FMT_YUV444P12BE => Pixel::YUV444P12BE,
            ffi::AV_PIX_FMT_YUV444P12LE => Pixel::YUV444P12LE,
            ffi::AV_PIX_FMT_YUV444P14BE => Pixel::YUV444P14BE,
            ffi::AV_PIX_FMT_YUV444P14LE => Pixel::YUV444P14LE,
            ffi::AV_PIX_FMT_GBRP12BE => Pixel::GBRP12BE,
            ffi::AV_PIX_FMT_GBRP12LE => Pixel::GBRP12LE,
            ffi::AV_PIX_FMT_GBRP14BE => Pixel::GBRP14BE,
            ffi::AV_PIX_FMT_GBRP14LE => Pixel::GBRP14LE,
            ffi::AV_PIX_FMT_GBRAP => Pixel::GBRAP,
            ffi::AV_PIX_FMT_GBRAP16BE => Pixel::GBRAP16BE,
            ffi::AV_PIX_FMT_GBRAP16LE => Pixel::GBRAP16LE,
            ffi::AV_PIX_FMT_YUVJ411P => Pixel::YUVJ411P,

            ffi::AV_PIX_FMT_BAYER_BGGR8 => Pixel::BAYER_BGGR8,
            ffi::AV_PIX_FMT_BAYER_RGGB8 => Pixel::BAYER_RGGB8,
            ffi::AV_PIX_FMT_BAYER_GBRG8 => Pixel::BAYER_GBRG8,
            ffi::AV_PIX_FMT_BAYER_GRBG8 => Pixel::BAYER_GRBG8,
            ffi::AV_PIX_FMT_BAYER_BGGR16LE => Pixel::BAYER_BGGR16LE,
            ffi::AV_PIX_FMT_BAYER_BGGR16BE => Pixel::BAYER_BGGR16BE,
            ffi::AV_PIX_FMT_BAYER_RGGB16LE => Pixel::BAYER_RGGB16LE,
            ffi::AV_PIX_FMT_BAYER_RGGB16BE => Pixel::BAYER_RGGB16BE,
            ffi::AV_PIX_FMT_BAYER_GBRG16LE => Pixel::BAYER_GBRG16LE,
            ffi::AV_PIX_FMT_BAYER_GBRG16BE => Pixel::BAYER_GBRG16BE,
            ffi::AV_PIX_FMT_BAYER_GRBG16LE => Pixel::BAYER_GRBG16LE,
            ffi::AV_PIX_FMT_BAYER_GRBG16BE => Pixel::BAYER_GRBG16BE,

            ffi::AV_PIX_FMT_YUV440P10LE => Pixel::YUV440P10LE,
            ffi::AV_PIX_FMT_YUV440P10BE => Pixel::YUV440P10BE,
            ffi::AV_PIX_FMT_YUV440P12LE => Pixel::YUV440P12LE,
            ffi::AV_PIX_FMT_YUV440P12BE => Pixel::YUV440P12BE,
            ffi::AV_PIX_FMT_AYUV64LE => Pixel::AYUV64LE,
            ffi::AV_PIX_FMT_AYUV64BE => Pixel::AYUV64BE,

            ffi::AV_PIX_FMT_VIDEOTOOLBOX => Pixel::VIDEOTOOLBOX,

            ffi::AV_PIX_FMT_P010LE => Pixel::P010LE,
            ffi::AV_PIX_FMT_P010BE => Pixel::P010BE,
            ffi::AV_PIX_FMT_GBRAP12BE => Pixel::GBRAP12BE,
            ffi::AV_PIX_FMT_GBRAP12LE => Pixel::GBRAP12LE,
            ffi::AV_PIX_FMT_GBRAP10LE => Pixel::GBRAP10LE,
            ffi::AV_PIX_FMT_GBRAP10BE => Pixel::GBRAP10BE,
            ffi::AV_PIX_FMT_MEDIACODEC => Pixel::MEDIACODEC,
            ffi::AV_PIX_FMT_GRAY12BE => Pixel::GRAY12BE,
            ffi::AV_PIX_FMT_GRAY12LE => Pixel::GRAY12LE,
            ffi::AV_PIX_FMT_GRAY10BE => Pixel::GRAY10BE,
            ffi::AV_PIX_FMT_GRAY10LE => Pixel::GRAY10LE,
            ffi::AV_PIX_FMT_P016LE => Pixel::P016LE,
            ffi::AV_PIX_FMT_P016BE => Pixel::P016BE,

            ffi::AV_PIX_FMT_NB => Pixel::None,

            ffi::AV_PIX_FMT_D3D11 => Pixel::D3D11,
            ffi::AV_PIX_FMT_GRAY9BE => Pixel::GRAY9BE,
            ffi::AV_PIX_FMT_GRAY9LE => Pixel::GRAY9LE,
            ffi::AV_PIX_FMT_GBRPF32BE => Pixel::GBRPF32BE,
            ffi::AV_PIX_FMT_GBRPF32LE => Pixel::GBRPF32LE,
            ffi::AV_PIX_FMT_GBRAPF32BE => Pixel::GBRAPF32BE,
            ffi::AV_PIX_FMT_GBRAPF32LE => Pixel::GBRAPF32LE,
            ffi::AV_PIX_FMT_DRM_PRIME => Pixel::DRM_PRIME,

            // #[cfg(feature = "ffmpeg_4_0")]
            ffi::AV_PIX_FMT_OPENCL => Pixel::OPENCL,

            // #[cfg(feature = "ffmpeg_4_1")]
            ffi::AV_PIX_FMT_GRAY14BE => Pixel::GRAY14BE,
            // #[cfg(feature = "ffmpeg_4_1")]
            ffi::AV_PIX_FMT_GRAY14LE => Pixel::GRAY14LE,
            // #[cfg(feature = "ffmpeg_4_1")]
            ffi::AV_PIX_FMT_GRAYF32BE => Pixel::GRAYF32BE,
            // #[cfg(feature = "ffmpeg_4_1")]
            ffi::AV_PIX_FMT_GRAYF32LE => Pixel::GRAYF32LE,

            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_PIX_FMT_YUVA422P12BE => Pixel::YUVA422P12BE,
            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_PIX_FMT_YUVA422P12LE => Pixel::YUVA422P12LE,
            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_PIX_FMT_YUVA444P12BE => Pixel::YUVA444P12BE,
            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_PIX_FMT_YUVA444P12LE => Pixel::YUVA444P12LE,
            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_PIX_FMT_NV24 => Pixel::NV24,
            // #[cfg(feature = "ffmpeg_4_2")]
            ffi::AV_PIX_FMT_NV42 => Pixel::NV42,

            // #[cfg(feature = "ffmpeg_4_3")]
            ffi::AV_PIX_FMT_VULKAN => Pixel::VULKAN,
            // #[cfg(feature = "ffmpeg_4_3")]
            ffi::AV_PIX_FMT_Y210BE => Pixel::Y210BE,
            // #[cfg(feature = "ffmpeg_4_3")]
            ffi::AV_PIX_FMT_Y210LE => Pixel::Y210LE,

            // #[cfg(feature = "ffmpeg_4_4")]
            ffi::AV_PIX_FMT_X2RGB10LE => Pixel::X2RGB10LE,
            // #[cfg(feature = "ffmpeg_4_4")]
            ffi::AV_PIX_FMT_X2RGB10BE => Pixel::X2RGB10BE,

            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_X2BGR10LE => Pixel::X2BGR10LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_X2BGR10BE => Pixel::X2BGR10BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P210BE => Pixel::P210BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P210LE => Pixel::P210LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P410BE => Pixel::P410BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P410LE => Pixel::P410LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P216BE => Pixel::P216BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P216LE => Pixel::P216LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P416BE => Pixel::P416BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            ffi::AV_PIX_FMT_P416LE => Pixel::P416LE,

            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_VUYA => Pixel::VUYA,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_RGBAF16BE => Pixel::RGBAF16BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_RGBAF16LE => Pixel::RGBAF16LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_VUYX => Pixel::VUYX,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_P012LE => Pixel::P012LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_P012BE => Pixel::P012BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_Y212BE => Pixel::Y212BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_Y212LE => Pixel::Y212LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_XV30BE => Pixel::XV30BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_XV30LE => Pixel::XV30LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_XV36BE => Pixel::XV36BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_XV36LE => Pixel::XV36LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_RGBF32BE => Pixel::RGBF32BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_RGBF32LE => Pixel::RGBF32LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_RGBAF32BE => Pixel::RGBAF32BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_RGBAF32LE => Pixel::RGBAF32LE,

            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_P212BE => Pixel::P212BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_P212LE => Pixel::P212LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_P412BE => Pixel::P412BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_P412LE => Pixel::P412LE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_GBRAP14BE => Pixel::GBRAP14BE,
            #[cfg(feature = "ffmpeg6")]
            ffi::AV_PIX_FMT_GBRAP14LE => Pixel::GBRAP14LE,

            #[cfg(feature = "ffmpeg7")]
            ffi::AV_PIX_FMT_D3D12 => Pixel::D3D12,

            // #[cfg(feature = "rpi")]
            // ffi::AV_PIX_FMT_SAND128 => Pixel::SAND128,
            // #[cfg(feature = "rpi")]
            // ffi::AV_PIX_FMT_SAND64_10 => Pixel::SAND64_10,
            // #[cfg(feature = "rpi")]
            // ffi::AV_PIX_FMT_SAND64_16 => Pixel::SAND64_16,
            // #[cfg(feature = "rpi")]
            // ffi::AV_PIX_FMT_RPI4_8 => Pixel::RPI4_8,
            // #[cfg(feature = "rpi")]
            // ffi::AV_PIX_FMT_RPI4_10 => Pixel::RPI4_10,
            _ => panic!("Unsupported pixel type"),
        }
    }
}

impl From<Pixel> for ffi::AVPixelFormat {
    #[inline]
    fn from(value: Pixel) -> ffi::AVPixelFormat {
        match value {
            Pixel::None => ffi::AV_PIX_FMT_NONE,

            Pixel::YUV420P => ffi::AV_PIX_FMT_YUV420P,
            Pixel::YUYV422 => ffi::AV_PIX_FMT_YUYV422,
            Pixel::RGB24 => ffi::AV_PIX_FMT_RGB24,
            Pixel::BGR24 => ffi::AV_PIX_FMT_BGR24,
            Pixel::YUV422P => ffi::AV_PIX_FMT_YUV422P,
            Pixel::YUV444P => ffi::AV_PIX_FMT_YUV444P,
            Pixel::YUV410P => ffi::AV_PIX_FMT_YUV410P,
            Pixel::YUV411P => ffi::AV_PIX_FMT_YUV411P,
            Pixel::GRAY8 => ffi::AV_PIX_FMT_GRAY8,
            Pixel::MonoWhite => ffi::AV_PIX_FMT_MONOWHITE,
            Pixel::MonoBlack => ffi::AV_PIX_FMT_MONOBLACK,
            Pixel::PAL8 => ffi::AV_PIX_FMT_PAL8,
            Pixel::YUVJ420P => ffi::AV_PIX_FMT_YUVJ420P,
            Pixel::YUVJ422P => ffi::AV_PIX_FMT_YUVJ422P,
            Pixel::YUVJ444P => ffi::AV_PIX_FMT_YUVJ444P,
            // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
            Pixel::XVMC_MPEG2_MC => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_XVMC_MPEG2_MC,
            // #[cfg(all(feature = "ff_api_xvmc", not(feature = "ffmpeg_5_0")))]
            Pixel::XVMC_MPEG2_IDCT => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_XVMC_MPEG2_IDCT,
            Pixel::UYVY422 => ffi::AV_PIX_FMT_UYVY422,
            Pixel::UYYVYY411 => ffi::AV_PIX_FMT_UYYVYY411,
            Pixel::BGR8 => ffi::AV_PIX_FMT_BGR8,
            Pixel::BGR4 => ffi::AV_PIX_FMT_BGR4,
            Pixel::BGR4_BYTE => ffi::AV_PIX_FMT_BGR4_BYTE,
            Pixel::RGB8 => ffi::AV_PIX_FMT_RGB8,
            Pixel::RGB4 => ffi::AV_PIX_FMT_RGB4,
            Pixel::RGB4_BYTE => ffi::AV_PIX_FMT_RGB4_BYTE,
            Pixel::NV12 => ffi::AV_PIX_FMT_NV12,
            Pixel::NV21 => ffi::AV_PIX_FMT_NV21,

            Pixel::ARGB => ffi::AV_PIX_FMT_ARGB,
            Pixel::RGBA => ffi::AV_PIX_FMT_RGBA,
            Pixel::ABGR => ffi::AV_PIX_FMT_ABGR,
            Pixel::BGRA => ffi::AV_PIX_FMT_BGRA,

            Pixel::GRAY16BE => ffi::AV_PIX_FMT_GRAY16BE,
            Pixel::GRAY16LE => ffi::AV_PIX_FMT_GRAY16LE,
            Pixel::YUV440P => ffi::AV_PIX_FMT_YUV440P,
            Pixel::YUVJ440P => ffi::AV_PIX_FMT_YUVJ440P,
            Pixel::YUVA420P => ffi::AV_PIX_FMT_YUVA420P,
            // #[cfg(feature = "ff_api_vdpau")]
            Pixel::VDPAU_H264 => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VDPAU_H264,
            // #[cfg(feature = "ff_api_vdpau")]
            Pixel::VDPAU_MPEG1 => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VDPAU_MPEG1,
            // #[cfg(feature = "ff_api_vdpau")]
            Pixel::VDPAU_MPEG2 => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VDPAU_MPEG2,
            // #[cfg(feature = "ff_api_vdpau")]
            Pixel::VDPAU_WMV3 => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VDPAU_WMV3,
            // #[cfg(feature = "ff_api_vdpau")]
            Pixel::VDPAU_VC1 => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VDPAU_VC1,
            Pixel::RGB48BE => ffi::AV_PIX_FMT_RGB48BE,
            Pixel::RGB48LE => ffi::AV_PIX_FMT_RGB48LE,

            Pixel::RGB565BE => ffi::AV_PIX_FMT_RGB565BE,
            Pixel::RGB565LE => ffi::AV_PIX_FMT_RGB565LE,
            Pixel::RGB555BE => ffi::AV_PIX_FMT_RGB555BE,
            Pixel::RGB555LE => ffi::AV_PIX_FMT_RGB555LE,

            Pixel::BGR565BE => ffi::AV_PIX_FMT_BGR565BE,
            Pixel::BGR565LE => ffi::AV_PIX_FMT_BGR565LE,
            Pixel::BGR555BE => ffi::AV_PIX_FMT_BGR555BE,
            Pixel::BGR555LE => ffi::AV_PIX_FMT_BGR555LE,

            // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
            Pixel::VAAPI_MOCO => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VAAPI_MOCO,
            // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
            Pixel::VAAPI_IDCT => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VAAPI_IDCT,
            // #[cfg(all(feature = "ff_api_vaapi", not(feature = "ffmpeg_5_0")))]
            Pixel::VAAPI_VLD => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VAAPI_VLD,
            // #[cfg(not(feature = "ff_api_vaapi"))]
            Pixel::VAAPI => ffi::AV_PIX_FMT_VAAPI,

            Pixel::YUV420P16LE => ffi::AV_PIX_FMT_YUV420P16LE,
            Pixel::YUV420P16BE => ffi::AV_PIX_FMT_YUV420P16BE,
            Pixel::YUV422P16LE => ffi::AV_PIX_FMT_YUV422P16LE,
            Pixel::YUV422P16BE => ffi::AV_PIX_FMT_YUV422P16BE,
            Pixel::YUV444P16LE => ffi::AV_PIX_FMT_YUV444P16LE,
            Pixel::YUV444P16BE => ffi::AV_PIX_FMT_YUV444P16BE,
            // #[cfg(feature = "ff_api_vdpau")]
            Pixel::VDPAU_MPEG4 => panic!("Unsupported pixel type"), // ffi::AV_PIX_FMT_VDPAU_MPEG4,
            Pixel::DXVA2_VLD => ffi::AV_PIX_FMT_DXVA2_VLD,

            Pixel::RGB444LE => ffi::AV_PIX_FMT_RGB444LE,
            Pixel::RGB444BE => ffi::AV_PIX_FMT_RGB444BE,
            Pixel::BGR444LE => ffi::AV_PIX_FMT_BGR444LE,
            Pixel::BGR444BE => ffi::AV_PIX_FMT_BGR444BE,
            Pixel::YA8 => ffi::AV_PIX_FMT_YA8,

            Pixel::BGR48BE => ffi::AV_PIX_FMT_BGR48BE,
            Pixel::BGR48LE => ffi::AV_PIX_FMT_BGR48LE,

            Pixel::YUV420P9BE => ffi::AV_PIX_FMT_YUV420P9BE,
            Pixel::YUV420P9LE => ffi::AV_PIX_FMT_YUV420P9LE,
            Pixel::YUV420P10BE => ffi::AV_PIX_FMT_YUV420P10BE,
            Pixel::YUV420P10LE => ffi::AV_PIX_FMT_YUV420P10LE,
            Pixel::YUV422P10BE => ffi::AV_PIX_FMT_YUV422P10BE,
            Pixel::YUV422P10LE => ffi::AV_PIX_FMT_YUV422P10LE,
            Pixel::YUV444P9BE => ffi::AV_PIX_FMT_YUV444P9BE,
            Pixel::YUV444P9LE => ffi::AV_PIX_FMT_YUV444P9LE,
            Pixel::YUV444P10BE => ffi::AV_PIX_FMT_YUV444P10BE,
            Pixel::YUV444P10LE => ffi::AV_PIX_FMT_YUV444P10LE,
            Pixel::YUV422P9BE => ffi::AV_PIX_FMT_YUV422P9BE,
            Pixel::YUV422P9LE => ffi::AV_PIX_FMT_YUV422P9LE,
            #[cfg(not(feature = "ffmpeg7"))]
            Pixel::VDA_VLD => ffi::AV_PIX_FMT_VDA_VLD,

            Pixel::GBRP => ffi::AV_PIX_FMT_GBRP,
            Pixel::GBRP9BE => ffi::AV_PIX_FMT_GBRP9BE,
            Pixel::GBRP9LE => ffi::AV_PIX_FMT_GBRP9LE,
            Pixel::GBRP10BE => ffi::AV_PIX_FMT_GBRP10BE,
            Pixel::GBRP10LE => ffi::AV_PIX_FMT_GBRP10LE,
            Pixel::GBRP16BE => ffi::AV_PIX_FMT_GBRP16BE,
            Pixel::GBRP16LE => ffi::AV_PIX_FMT_GBRP16LE,

            Pixel::YUVA420P9BE => ffi::AV_PIX_FMT_YUVA420P9BE,
            Pixel::YUVA420P9LE => ffi::AV_PIX_FMT_YUVA420P9LE,
            Pixel::YUVA422P9BE => ffi::AV_PIX_FMT_YUVA422P9BE,
            Pixel::YUVA422P9LE => ffi::AV_PIX_FMT_YUVA422P9LE,
            Pixel::YUVA444P9BE => ffi::AV_PIX_FMT_YUVA444P9BE,
            Pixel::YUVA444P9LE => ffi::AV_PIX_FMT_YUVA444P9LE,
            Pixel::YUVA420P10BE => ffi::AV_PIX_FMT_YUVA420P10BE,
            Pixel::YUVA420P10LE => ffi::AV_PIX_FMT_YUVA420P10LE,
            Pixel::YUVA422P10BE => ffi::AV_PIX_FMT_YUVA422P10BE,
            Pixel::YUVA422P10LE => ffi::AV_PIX_FMT_YUVA422P10LE,
            Pixel::YUVA444P10BE => ffi::AV_PIX_FMT_YUVA444P10BE,
            Pixel::YUVA444P10LE => ffi::AV_PIX_FMT_YUVA444P10LE,
            Pixel::YUVA420P16BE => ffi::AV_PIX_FMT_YUVA420P16BE,
            Pixel::YUVA420P16LE => ffi::AV_PIX_FMT_YUVA420P16LE,
            Pixel::YUVA422P16BE => ffi::AV_PIX_FMT_YUVA422P16BE,
            Pixel::YUVA422P16LE => ffi::AV_PIX_FMT_YUVA422P16LE,
            Pixel::YUVA444P16BE => ffi::AV_PIX_FMT_YUVA444P16BE,
            Pixel::YUVA444P16LE => ffi::AV_PIX_FMT_YUVA444P16LE,

            Pixel::VDPAU => ffi::AV_PIX_FMT_VDPAU,

            Pixel::XYZ12LE => ffi::AV_PIX_FMT_XYZ12LE,
            Pixel::XYZ12BE => ffi::AV_PIX_FMT_XYZ12BE,
            Pixel::NV16 => ffi::AV_PIX_FMT_NV16,
            Pixel::NV20LE => ffi::AV_PIX_FMT_NV20LE,
            Pixel::NV20BE => ffi::AV_PIX_FMT_NV20BE,

            Pixel::RGBA64BE => ffi::AV_PIX_FMT_RGBA64BE,
            Pixel::RGBA64LE => ffi::AV_PIX_FMT_RGBA64LE,
            Pixel::BGRA64BE => ffi::AV_PIX_FMT_BGRA64BE,
            Pixel::BGRA64LE => ffi::AV_PIX_FMT_BGRA64LE,

            Pixel::YVYU422 => ffi::AV_PIX_FMT_YVYU422,

            #[cfg(not(feature = "ffmpeg7"))]
            Pixel::VDA => ffi::AV_PIX_FMT_VDA,

            Pixel::YA16BE => ffi::AV_PIX_FMT_YA16BE,
            Pixel::YA16LE => ffi::AV_PIX_FMT_YA16LE,

            Pixel::QSV => ffi::AV_PIX_FMT_QSV,
            Pixel::MMAL => ffi::AV_PIX_FMT_MMAL,

            Pixel::D3D11VA_VLD => ffi::AV_PIX_FMT_D3D11VA_VLD,

            Pixel::CUDA => ffi::AV_PIX_FMT_CUDA,

            Pixel::ZRGB => ffi::AV_PIX_FMT_0RGB,
            Pixel::RGBZ => ffi::AV_PIX_FMT_RGB0,
            Pixel::ZBGR => ffi::AV_PIX_FMT_0BGR,
            Pixel::BGRZ => ffi::AV_PIX_FMT_BGR0,
            Pixel::YUVA444P => ffi::AV_PIX_FMT_YUVA444P,
            Pixel::YUVA422P => ffi::AV_PIX_FMT_YUVA422P,

            Pixel::YUV420P12BE => ffi::AV_PIX_FMT_YUV420P12BE,
            Pixel::YUV420P12LE => ffi::AV_PIX_FMT_YUV420P12LE,
            Pixel::YUV420P14BE => ffi::AV_PIX_FMT_YUV420P14BE,
            Pixel::YUV420P14LE => ffi::AV_PIX_FMT_YUV420P14LE,
            Pixel::YUV422P12BE => ffi::AV_PIX_FMT_YUV422P12BE,
            Pixel::YUV422P12LE => ffi::AV_PIX_FMT_YUV422P12LE,
            Pixel::YUV422P14BE => ffi::AV_PIX_FMT_YUV422P14BE,
            Pixel::YUV422P14LE => ffi::AV_PIX_FMT_YUV422P14LE,
            Pixel::YUV444P12BE => ffi::AV_PIX_FMT_YUV444P12BE,
            Pixel::YUV444P12LE => ffi::AV_PIX_FMT_YUV444P12LE,
            Pixel::YUV444P14BE => ffi::AV_PIX_FMT_YUV444P14BE,
            Pixel::YUV444P14LE => ffi::AV_PIX_FMT_YUV444P14LE,
            Pixel::GBRP12BE => ffi::AV_PIX_FMT_GBRP12BE,
            Pixel::GBRP12LE => ffi::AV_PIX_FMT_GBRP12LE,
            Pixel::GBRP14BE => ffi::AV_PIX_FMT_GBRP14BE,
            Pixel::GBRP14LE => ffi::AV_PIX_FMT_GBRP14LE,
            Pixel::GBRAP => ffi::AV_PIX_FMT_GBRAP,
            Pixel::GBRAP16BE => ffi::AV_PIX_FMT_GBRAP16BE,
            Pixel::GBRAP16LE => ffi::AV_PIX_FMT_GBRAP16LE,
            Pixel::YUVJ411P => ffi::AV_PIX_FMT_YUVJ411P,

            Pixel::BAYER_BGGR8 => ffi::AV_PIX_FMT_BAYER_BGGR8,
            Pixel::BAYER_RGGB8 => ffi::AV_PIX_FMT_BAYER_RGGB8,
            Pixel::BAYER_GBRG8 => ffi::AV_PIX_FMT_BAYER_GBRG8,
            Pixel::BAYER_GRBG8 => ffi::AV_PIX_FMT_BAYER_GRBG8,
            Pixel::BAYER_BGGR16LE => ffi::AV_PIX_FMT_BAYER_BGGR16LE,
            Pixel::BAYER_BGGR16BE => ffi::AV_PIX_FMT_BAYER_BGGR16BE,
            Pixel::BAYER_RGGB16LE => ffi::AV_PIX_FMT_BAYER_RGGB16LE,
            Pixel::BAYER_RGGB16BE => ffi::AV_PIX_FMT_BAYER_RGGB16BE,
            Pixel::BAYER_GBRG16LE => ffi::AV_PIX_FMT_BAYER_GBRG16LE,
            Pixel::BAYER_GBRG16BE => ffi::AV_PIX_FMT_BAYER_GBRG16BE,
            Pixel::BAYER_GRBG16LE => ffi::AV_PIX_FMT_BAYER_GRBG16LE,
            Pixel::BAYER_GRBG16BE => ffi::AV_PIX_FMT_BAYER_GRBG16BE,

            Pixel::YUV440P10LE => ffi::AV_PIX_FMT_YUV440P10LE,
            Pixel::YUV440P10BE => ffi::AV_PIX_FMT_YUV440P10BE,
            Pixel::YUV440P12LE => ffi::AV_PIX_FMT_YUV440P12LE,
            Pixel::YUV440P12BE => ffi::AV_PIX_FMT_YUV440P12BE,
            Pixel::AYUV64LE => ffi::AV_PIX_FMT_AYUV64LE,
            Pixel::AYUV64BE => ffi::AV_PIX_FMT_AYUV64BE,

            Pixel::VIDEOTOOLBOX => ffi::AV_PIX_FMT_VIDEOTOOLBOX,

            // --- defaults
            // #[cfg(all(feature = "ffmpeg_4_0", not(feature = "ffmpeg7")))]
            // Pixel::XVMC => ffi::AV_PIX_FMT_XVMC,
            Pixel::RGB32 => ffi::AV_PIX_FMT_RGB32,
            Pixel::RGB32_1 => ffi::AV_PIX_FMT_RGB32_1,
            Pixel::BGR32 => ffi::AV_PIX_FMT_BGR32,
            Pixel::BGR32_1 => ffi::AV_PIX_FMT_BGR32_1,
            Pixel::ZRGB32 => ffi::AV_PIX_FMT_0RGB32,
            Pixel::ZBGR32 => ffi::AV_PIX_FMT_0BGR32,

            Pixel::GRAY16 => ffi::AV_PIX_FMT_GRAY16,
            Pixel::YA16 => ffi::AV_PIX_FMT_YA16,
            Pixel::RGB48 => ffi::AV_PIX_FMT_RGB48,
            Pixel::RGB565 => ffi::AV_PIX_FMT_RGB565,
            Pixel::RGB555 => ffi::AV_PIX_FMT_RGB555,
            Pixel::RGB444 => ffi::AV_PIX_FMT_RGB444,
            Pixel::BGR48 => ffi::AV_PIX_FMT_BGR48,
            Pixel::BGR565 => ffi::AV_PIX_FMT_BGR565,
            Pixel::BGR555 => ffi::AV_PIX_FMT_BGR555,
            Pixel::BGR444 => ffi::AV_PIX_FMT_BGR444,

            Pixel::YUV420P9 => ffi::AV_PIX_FMT_YUV420P9,
            Pixel::YUV422P9 => ffi::AV_PIX_FMT_YUV422P9,
            Pixel::YUV444P9 => ffi::AV_PIX_FMT_YUV444P9,
            Pixel::YUV420P10 => ffi::AV_PIX_FMT_YUV420P10,
            Pixel::YUV422P10 => ffi::AV_PIX_FMT_YUV422P10,
            Pixel::YUV440P10 => ffi::AV_PIX_FMT_YUV440P10,
            Pixel::YUV444P10 => ffi::AV_PIX_FMT_YUV444P10,
            Pixel::YUV420P12 => ffi::AV_PIX_FMT_YUV420P12,
            Pixel::YUV422P12 => ffi::AV_PIX_FMT_YUV422P12,
            Pixel::YUV440P12 => ffi::AV_PIX_FMT_YUV440P12,
            Pixel::YUV444P12 => ffi::AV_PIX_FMT_YUV444P12,
            Pixel::YUV420P14 => ffi::AV_PIX_FMT_YUV420P14,
            Pixel::YUV422P14 => ffi::AV_PIX_FMT_YUV422P14,
            Pixel::YUV444P14 => ffi::AV_PIX_FMT_YUV444P14,
            Pixel::YUV420P16 => ffi::AV_PIX_FMT_YUV420P16,
            Pixel::YUV422P16 => ffi::AV_PIX_FMT_YUV422P16,
            Pixel::YUV444P16 => ffi::AV_PIX_FMT_YUV444P16,

            Pixel::GBRP9 => ffi::AV_PIX_FMT_GBRP9,
            Pixel::GBRP10 => ffi::AV_PIX_FMT_GBRP10,
            Pixel::GBRP12 => ffi::AV_PIX_FMT_GBRP12,
            Pixel::GBRP14 => ffi::AV_PIX_FMT_GBRP14,
            Pixel::GBRP16 => ffi::AV_PIX_FMT_GBRP16,
            Pixel::GBRAP16 => ffi::AV_PIX_FMT_GBRAP16,

            Pixel::BAYER_BGGR16 => ffi::AV_PIX_FMT_BAYER_BGGR16,
            Pixel::BAYER_RGGB16 => ffi::AV_PIX_FMT_BAYER_RGGB16,
            Pixel::BAYER_GBRG16 => ffi::AV_PIX_FMT_BAYER_GBRG16,
            Pixel::BAYER_GRBG16 => ffi::AV_PIX_FMT_BAYER_GRBG16,

            Pixel::YUVA420P9 => ffi::AV_PIX_FMT_YUVA420P9,
            Pixel::YUVA422P9 => ffi::AV_PIX_FMT_YUVA422P9,
            Pixel::YUVA444P9 => ffi::AV_PIX_FMT_YUVA444P9,
            Pixel::YUVA420P10 => ffi::AV_PIX_FMT_YUVA420P10,
            Pixel::YUVA422P10 => ffi::AV_PIX_FMT_YUVA422P10,
            Pixel::YUVA444P10 => ffi::AV_PIX_FMT_YUVA444P10,
            Pixel::YUVA420P16 => ffi::AV_PIX_FMT_YUVA420P16,
            Pixel::YUVA422P16 => ffi::AV_PIX_FMT_YUVA422P16,
            Pixel::YUVA444P16 => ffi::AV_PIX_FMT_YUVA444P16,

            Pixel::XYZ12 => ffi::AV_PIX_FMT_XYZ12,
            Pixel::NV20 => ffi::AV_PIX_FMT_NV20,
            Pixel::AYUV64 => ffi::AV_PIX_FMT_AYUV64,

            Pixel::P010LE => ffi::AV_PIX_FMT_P010LE,
            Pixel::P010BE => ffi::AV_PIX_FMT_P010BE,
            Pixel::GBRAP12BE => ffi::AV_PIX_FMT_GBRAP12BE,
            Pixel::GBRAP12LE => ffi::AV_PIX_FMT_GBRAP12LE,
            Pixel::GBRAP10LE => ffi::AV_PIX_FMT_GBRAP10LE,
            Pixel::GBRAP10BE => ffi::AV_PIX_FMT_GBRAP10BE,
            Pixel::MEDIACODEC => ffi::AV_PIX_FMT_MEDIACODEC,
            Pixel::GRAY12BE => ffi::AV_PIX_FMT_GRAY12BE,
            Pixel::GRAY12LE => ffi::AV_PIX_FMT_GRAY12LE,
            Pixel::GRAY10BE => ffi::AV_PIX_FMT_GRAY10BE,
            Pixel::GRAY10LE => ffi::AV_PIX_FMT_GRAY10LE,
            Pixel::P016LE => ffi::AV_PIX_FMT_P016LE,
            Pixel::P016BE => ffi::AV_PIX_FMT_P016BE,

            Pixel::D3D11 => ffi::AV_PIX_FMT_D3D11,
            Pixel::GRAY9BE => ffi::AV_PIX_FMT_GRAY9BE,
            Pixel::GRAY9LE => ffi::AV_PIX_FMT_GRAY9LE,
            Pixel::GBRPF32BE => ffi::AV_PIX_FMT_GBRPF32BE,
            Pixel::GBRPF32LE => ffi::AV_PIX_FMT_GBRPF32LE,
            Pixel::GBRAPF32BE => ffi::AV_PIX_FMT_GBRAPF32BE,
            Pixel::GBRAPF32LE => ffi::AV_PIX_FMT_GBRAPF32LE,
            Pixel::DRM_PRIME => ffi::AV_PIX_FMT_DRM_PRIME,

            // #[cfg(feature = "ffmpeg_4_0")]
            Pixel::OPENCL => ffi::AV_PIX_FMT_OPENCL,

            // #[cfg(feature = "ffmpeg_4_1")]
            Pixel::GRAY14BE => ffi::AV_PIX_FMT_GRAY14BE,
            // #[cfg(feature = "ffmpeg_4_1")]
            Pixel::GRAY14LE => ffi::AV_PIX_FMT_GRAY14LE,
            // #[cfg(feature = "ffmpeg_4_1")]
            Pixel::GRAYF32BE => ffi::AV_PIX_FMT_GRAYF32BE,
            // #[cfg(feature = "ffmpeg_4_1")]
            Pixel::GRAYF32LE => ffi::AV_PIX_FMT_GRAYF32LE,

            // #[cfg(feature = "ffmpeg_4_2")]
            Pixel::YUVA422P12BE => ffi::AV_PIX_FMT_YUVA422P12BE,
            // #[cfg(feature = "ffmpeg_4_2")]
            Pixel::YUVA422P12LE => ffi::AV_PIX_FMT_YUVA422P12LE,
            // #[cfg(feature = "ffmpeg_4_2")]
            Pixel::YUVA444P12BE => ffi::AV_PIX_FMT_YUVA444P12BE,
            // #[cfg(feature = "ffmpeg_4_2")]
            Pixel::YUVA444P12LE => ffi::AV_PIX_FMT_YUVA444P12LE,
            // #[cfg(feature = "ffmpeg_4_2")]
            Pixel::NV24 => ffi::AV_PIX_FMT_NV24,
            // #[cfg(feature = "ffmpeg_4_2")]
            Pixel::NV42 => ffi::AV_PIX_FMT_NV42,

            // #[cfg(feature = "ffmpeg_4_3")]
            Pixel::VULKAN => ffi::AV_PIX_FMT_VULKAN,
            // #[cfg(feature = "ffmpeg_4_3")]
            Pixel::Y210BE => ffi::AV_PIX_FMT_Y210BE,
            // #[cfg(feature = "ffmpeg_4_3")]
            Pixel::Y210LE => ffi::AV_PIX_FMT_Y210LE,

            // #[cfg(feature = "ffmpeg_4_4")]
            Pixel::X2RGB10LE => ffi::AV_PIX_FMT_X2RGB10LE,
            // #[cfg(feature = "ffmpeg_4_4")]
            Pixel::X2RGB10BE => ffi::AV_PIX_FMT_X2RGB10BE,

            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::X2BGR10LE => ffi::AV_PIX_FMT_X2BGR10LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::X2BGR10BE => ffi::AV_PIX_FMT_X2BGR10BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P210BE => ffi::AV_PIX_FMT_P210BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P210LE => ffi::AV_PIX_FMT_P210LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P410BE => ffi::AV_PIX_FMT_P410BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P410LE => ffi::AV_PIX_FMT_P410LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P216BE => ffi::AV_PIX_FMT_P216BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P216LE => ffi::AV_PIX_FMT_P216LE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P416BE => ffi::AV_PIX_FMT_P416BE,
            // #[cfg(feature = "ffmpeg_5_0")]
            Pixel::P416LE => ffi::AV_PIX_FMT_P416LE,

            #[cfg(feature = "ffmpeg6")]
            Pixel::VUYA => ffi::AV_PIX_FMT_VUYA,
            #[cfg(feature = "ffmpeg6")]
            Pixel::RGBAF16BE => ffi::AV_PIX_FMT_RGBAF16BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::RGBAF16LE => ffi::AV_PIX_FMT_RGBAF16LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::VUYX => ffi::AV_PIX_FMT_VUYX,
            #[cfg(feature = "ffmpeg6")]
            Pixel::P012LE => ffi::AV_PIX_FMT_P012LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::P012BE => ffi::AV_PIX_FMT_P012BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::Y212BE => ffi::AV_PIX_FMT_Y212BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::Y212LE => ffi::AV_PIX_FMT_Y212LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::XV30BE => ffi::AV_PIX_FMT_XV30BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::XV30LE => ffi::AV_PIX_FMT_XV30LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::XV36BE => ffi::AV_PIX_FMT_XV36BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::XV36LE => ffi::AV_PIX_FMT_XV36LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::RGBF32BE => ffi::AV_PIX_FMT_RGBF32BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::RGBF32LE => ffi::AV_PIX_FMT_RGBF32LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::RGBAF32BE => ffi::AV_PIX_FMT_RGBAF32BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::RGBAF32LE => ffi::AV_PIX_FMT_RGBAF32LE,

            #[cfg(feature = "ffmpeg6")]
            Pixel::P212BE => ffi::AV_PIX_FMT_P212BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::P212LE => ffi::AV_PIX_FMT_P212LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::P412BE => ffi::AV_PIX_FMT_P412BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::P412LE => ffi::AV_PIX_FMT_P412LE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::GBRAP14BE => ffi::AV_PIX_FMT_GBRAP14BE,
            #[cfg(feature = "ffmpeg6")]
            Pixel::GBRAP14LE => ffi::AV_PIX_FMT_GBRAP14LE,

            #[cfg(feature = "ffmpeg7")]
            Pixel::D3D12 => ffi::AV_PIX_FMT_D3D12,

            // #[cfg(feature = "rpi")]
            // Pixel::SAND128 => ffi::AV_PIX_FMT_SAND128,
            // #[cfg(feature = "rpi")]
            // Pixel::SAND64_10 => ffi::AV_PIX_FMT_SAND64_10,
            // #[cfg(feature = "rpi")]
            // Pixel::SAND64_16 => ffi::AV_PIX_FMT_SAND64_16,
            // #[cfg(feature = "rpi")]
            // Pixel::RPI4_8 => ffi::AV_PIX_FMT_RPI4_8,
            // #[cfg(feature = "rpi")]
            // Pixel::RPI4_10 => ffi::AV_PIX_FMT_RPI4_10,
            _ => panic!("unknown pixel format"),
        }
    }
}

#[derive(Debug)]
pub enum ParsePixelError {
    NulError(NulError),
    UnknownFormat,
}

impl fmt::Display for ParsePixelError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ParsePixelError::NulError(ref e) => e.fmt(f),
            ParsePixelError::UnknownFormat => write!(f, "unknown pixel format"),
        }
    }
}

impl error::Error for ParsePixelError {
    fn cause(&self) -> Option<&dyn error::Error> {
        match *self {
            ParsePixelError::NulError(ref e) => Some(e),
            ParsePixelError::UnknownFormat => None,
        }
    }
}

impl From<NulError> for ParsePixelError {
    fn from(x: NulError) -> ParsePixelError {
        ParsePixelError::NulError(x)
    }
}

impl std::str::FromStr for Pixel {
    type Err = ParsePixelError;

    #[inline(always)]
    fn from_str(s: &str) -> Result<Pixel, ParsePixelError> {
        let cstring = CString::new(s)?;
        let format = unsafe { ffi::av_get_pix_fmt(cstring.as_ptr()) }.into();

        if format == Pixel::None {
            Err(ParsePixelError::UnknownFormat)
        } else {
            Ok(format)
        }
    }
}
