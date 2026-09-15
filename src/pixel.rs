use crate::error::{Result, RsmediaError};
use crate::fmt::DataLayout;

use rsmpeg::avutil::AVPixFmtDescriptorRef;
use rsmpeg::ffi;

ffi_enum!(
    /// ===== AV_PIX_FMT_FLAG_* 像素格式描述标志 =====
    AVPixFmtFlag, u32 {
    BE => ffi::AV_PIX_FMT_FLAG_BE;
    PAL => ffi::AV_PIX_FMT_FLAG_PAL;
    BITSTREAM => ffi::AV_PIX_FMT_FLAG_BITSTREAM;
    HWACCEL => ffi::AV_PIX_FMT_FLAG_HWACCEL;
    PLANAR => ffi::AV_PIX_FMT_FLAG_PLANAR;
    RGB => ffi::AV_PIX_FMT_FLAG_RGB;
    ALPHA => ffi::AV_PIX_FMT_FLAG_ALPHA;
    BAYER => ffi::AV_PIX_FMT_FLAG_BAYER;
    FLOAT => ffi::AV_PIX_FMT_FLAG_FLOAT;
    #[cfg(any(feature = "ffmpeg7", feature = "ffmpeg8", feature = "ffmpeg9"))]
    XYZ => ffi::AV_PIX_FMT_FLAG_XYZ;
});

ffi_enum_wrap_from!(
    #[allow(non_camel_case_types)]
    PixelFormat => ffi::AVPixelFormat,
    repr = i32,
    fallback = panic {
    NONE => ffi::AV_PIX_FMT_NONE;
    YUV420P => ffi::AV_PIX_FMT_YUV420P;
    YUYV422 => ffi::AV_PIX_FMT_YUYV422;
    RGB24 => ffi::AV_PIX_FMT_RGB24;
    BGR24 => ffi::AV_PIX_FMT_BGR24;
    YUV422P => ffi::AV_PIX_FMT_YUV422P;
    YUV444P => ffi::AV_PIX_FMT_YUV444P;
    YUV410P => ffi::AV_PIX_FMT_YUV410P;
    YUV411P => ffi::AV_PIX_FMT_YUV411P;
    GRAY8 => ffi::AV_PIX_FMT_GRAY8;
    MONOWHITE => ffi::AV_PIX_FMT_MONOWHITE;
    MONOBLACK => ffi::AV_PIX_FMT_MONOBLACK;
    PAL8 => ffi::AV_PIX_FMT_PAL8;
    YUVJ420P => ffi::AV_PIX_FMT_YUVJ420P;
    YUVJ422P => ffi::AV_PIX_FMT_YUVJ422P;
    YUVJ444P => ffi::AV_PIX_FMT_YUVJ444P;
    UYVY422 => ffi::AV_PIX_FMT_UYVY422;
    UYYVYY411 => ffi::AV_PIX_FMT_UYYVYY411;
    BGR8 => ffi::AV_PIX_FMT_BGR8;
    BGR4 => ffi::AV_PIX_FMT_BGR4;
    BGR4_BYTE => ffi::AV_PIX_FMT_BGR4_BYTE;
    RGB8 => ffi::AV_PIX_FMT_RGB8;
    RGB4 => ffi::AV_PIX_FMT_RGB4;
    RGB4_BYTE => ffi::AV_PIX_FMT_RGB4_BYTE;
    NV12 => ffi::AV_PIX_FMT_NV12;
    NV21 => ffi::AV_PIX_FMT_NV21;
    ARGB => ffi::AV_PIX_FMT_ARGB;
    RGBA => ffi::AV_PIX_FMT_RGBA;
    ABGR => ffi::AV_PIX_FMT_ABGR;
    BGRA => ffi::AV_PIX_FMT_BGRA;
    GRAY16BE => ffi::AV_PIX_FMT_GRAY16BE;
    GRAY16LE => ffi::AV_PIX_FMT_GRAY16LE;
    YUV440P => ffi::AV_PIX_FMT_YUV440P;
    YUVJ440P => ffi::AV_PIX_FMT_YUVJ440P;
    YUVA420P => ffi::AV_PIX_FMT_YUVA420P;
    RGB48BE => ffi::AV_PIX_FMT_RGB48BE;
    RGB48LE => ffi::AV_PIX_FMT_RGB48LE;
    RGB565BE => ffi::AV_PIX_FMT_RGB565BE;
    RGB565LE => ffi::AV_PIX_FMT_RGB565LE;
    RGB555BE => ffi::AV_PIX_FMT_RGB555BE;
    RGB555LE => ffi::AV_PIX_FMT_RGB555LE;
    BGR565BE => ffi::AV_PIX_FMT_BGR565BE;
    BGR565LE => ffi::AV_PIX_FMT_BGR565LE;
    BGR555BE => ffi::AV_PIX_FMT_BGR555BE;
    BGR555LE => ffi::AV_PIX_FMT_BGR555LE;
    VAAPI => ffi::AV_PIX_FMT_VAAPI;
    YUV420P16LE => ffi::AV_PIX_FMT_YUV420P16LE;
    YUV420P16BE => ffi::AV_PIX_FMT_YUV420P16BE;
    YUV422P16LE => ffi::AV_PIX_FMT_YUV422P16LE;
    YUV422P16BE => ffi::AV_PIX_FMT_YUV422P16BE;
    YUV444P16LE => ffi::AV_PIX_FMT_YUV444P16LE;
    YUV444P16BE => ffi::AV_PIX_FMT_YUV444P16BE;
    DXVA2_VLD => ffi::AV_PIX_FMT_DXVA2_VLD;
    RGB444LE => ffi::AV_PIX_FMT_RGB444LE;
    RGB444BE => ffi::AV_PIX_FMT_RGB444BE;
    BGR444LE => ffi::AV_PIX_FMT_BGR444LE;
    BGR444BE => ffi::AV_PIX_FMT_BGR444BE;
    YA8 => ffi::AV_PIX_FMT_YA8;
    BGR48BE => ffi::AV_PIX_FMT_BGR48BE;
    BGR48LE => ffi::AV_PIX_FMT_BGR48LE;
    YUV420P9BE => ffi::AV_PIX_FMT_YUV420P9BE;
    YUV420P9LE => ffi::AV_PIX_FMT_YUV420P9LE;
    YUV420P10BE => ffi::AV_PIX_FMT_YUV420P10BE;
    YUV420P10LE => ffi::AV_PIX_FMT_YUV420P10LE;
    YUV422P10BE => ffi::AV_PIX_FMT_YUV422P10BE;
    YUV422P10LE => ffi::AV_PIX_FMT_YUV422P10LE;
    YUV444P9BE => ffi::AV_PIX_FMT_YUV444P9BE;
    YUV444P9LE => ffi::AV_PIX_FMT_YUV444P9LE;
    YUV444P10BE => ffi::AV_PIX_FMT_YUV444P10BE;
    YUV444P10LE => ffi::AV_PIX_FMT_YUV444P10LE;
    YUV422P9BE => ffi::AV_PIX_FMT_YUV422P9BE;
    YUV422P9LE => ffi::AV_PIX_FMT_YUV422P9LE;
    GBRP => ffi::AV_PIX_FMT_GBRP;
    GBRP9BE => ffi::AV_PIX_FMT_GBRP9BE;
    GBRP9LE => ffi::AV_PIX_FMT_GBRP9LE;
    GBRP10BE => ffi::AV_PIX_FMT_GBRP10BE;
    GBRP10LE => ffi::AV_PIX_FMT_GBRP10LE;
    GBRP16BE => ffi::AV_PIX_FMT_GBRP16BE;
    GBRP16LE => ffi::AV_PIX_FMT_GBRP16LE;
    YUVA422P => ffi::AV_PIX_FMT_YUVA422P;
    YUVA444P => ffi::AV_PIX_FMT_YUVA444P;
    YUVA420P9BE => ffi::AV_PIX_FMT_YUVA420P9BE;
    YUVA420P9LE => ffi::AV_PIX_FMT_YUVA420P9LE;
    YUVA422P9BE => ffi::AV_PIX_FMT_YUVA422P9BE;
    YUVA422P9LE => ffi::AV_PIX_FMT_YUVA422P9LE;
    YUVA444P9BE => ffi::AV_PIX_FMT_YUVA444P9BE;
    YUVA444P9LE => ffi::AV_PIX_FMT_YUVA444P9LE;
    YUVA420P10BE => ffi::AV_PIX_FMT_YUVA420P10BE;
    YUVA420P10LE => ffi::AV_PIX_FMT_YUVA420P10LE;
    YUVA422P10BE => ffi::AV_PIX_FMT_YUVA422P10BE;
    YUVA422P10LE => ffi::AV_PIX_FMT_YUVA422P10LE;
    YUVA444P10BE => ffi::AV_PIX_FMT_YUVA444P10BE;
    YUVA444P10LE => ffi::AV_PIX_FMT_YUVA444P10LE;
    YUVA420P16BE => ffi::AV_PIX_FMT_YUVA420P16BE;
    YUVA420P16LE => ffi::AV_PIX_FMT_YUVA420P16LE;
    YUVA422P16BE => ffi::AV_PIX_FMT_YUVA422P16BE;
    YUVA422P16LE => ffi::AV_PIX_FMT_YUVA422P16LE;
    YUVA444P16BE => ffi::AV_PIX_FMT_YUVA444P16BE;
    YUVA444P16LE => ffi::AV_PIX_FMT_YUVA444P16LE;
    VDPAU => ffi::AV_PIX_FMT_VDPAU;
    XYZ12LE => ffi::AV_PIX_FMT_XYZ12LE;
    XYZ12BE => ffi::AV_PIX_FMT_XYZ12BE;
    NV16 => ffi::AV_PIX_FMT_NV16;
    NV20LE => ffi::AV_PIX_FMT_NV20LE;
    NV20BE => ffi::AV_PIX_FMT_NV20BE;
    RGBA64BE => ffi::AV_PIX_FMT_RGBA64BE;
    RGBA64LE => ffi::AV_PIX_FMT_RGBA64LE;
    BGRA64BE => ffi::AV_PIX_FMT_BGRA64BE;
    BGRA64LE => ffi::AV_PIX_FMT_BGRA64LE;
    YVYU422 => ffi::AV_PIX_FMT_YVYU422;
    YA16BE => ffi::AV_PIX_FMT_YA16BE;
    YA16LE => ffi::AV_PIX_FMT_YA16LE;
    GBRAP => ffi::AV_PIX_FMT_GBRAP;
    GBRAP16BE => ffi::AV_PIX_FMT_GBRAP16BE;
    GBRAP16LE => ffi::AV_PIX_FMT_GBRAP16LE;
    QSV => ffi::AV_PIX_FMT_QSV;
    MMAL => ffi::AV_PIX_FMT_MMAL;
    D3D11VA_VLD => ffi::AV_PIX_FMT_D3D11VA_VLD;
    CUDA => ffi::AV_PIX_FMT_CUDA;
    XRGB => ffi::AV_PIX_FMT_0RGB;
    RGB0 => ffi::AV_PIX_FMT_RGB0;
    XBGR => ffi::AV_PIX_FMT_0BGR;
    BGR0 => ffi::AV_PIX_FMT_BGR0;
    YUV420P12BE => ffi::AV_PIX_FMT_YUV420P12BE;
    YUV420P12LE => ffi::AV_PIX_FMT_YUV420P12LE;
    YUV420P14BE => ffi::AV_PIX_FMT_YUV420P14BE;
    YUV420P14LE => ffi::AV_PIX_FMT_YUV420P14LE;
    YUV422P12BE => ffi::AV_PIX_FMT_YUV422P12BE;
    YUV422P12LE => ffi::AV_PIX_FMT_YUV422P12LE;
    YUV422P14BE => ffi::AV_PIX_FMT_YUV422P14BE;
    YUV422P14LE => ffi::AV_PIX_FMT_YUV422P14LE;
    YUV444P12BE => ffi::AV_PIX_FMT_YUV444P12BE;
    YUV444P12LE => ffi::AV_PIX_FMT_YUV444P12LE;
    YUV444P14BE => ffi::AV_PIX_FMT_YUV444P14BE;
    YUV444P14LE => ffi::AV_PIX_FMT_YUV444P14LE;
    GBRP12BE => ffi::AV_PIX_FMT_GBRP12BE;
    GBRP12LE => ffi::AV_PIX_FMT_GBRP12LE;
    GBRP14BE => ffi::AV_PIX_FMT_GBRP14BE;
    GBRP14LE => ffi::AV_PIX_FMT_GBRP14LE;
    YUVJ411P => ffi::AV_PIX_FMT_YUVJ411P;
    BAYER_BGGR8 => ffi::AV_PIX_FMT_BAYER_BGGR8;
    BAYER_RGGB8 => ffi::AV_PIX_FMT_BAYER_RGGB8;
    BAYER_GBRG8 => ffi::AV_PIX_FMT_BAYER_GBRG8;
    BAYER_GRBG8 => ffi::AV_PIX_FMT_BAYER_GRBG8;
    BAYER_BGGR16LE => ffi::AV_PIX_FMT_BAYER_BGGR16LE;
    BAYER_BGGR16BE => ffi::AV_PIX_FMT_BAYER_BGGR16BE;
    BAYER_RGGB16LE => ffi::AV_PIX_FMT_BAYER_RGGB16LE;
    BAYER_RGGB16BE => ffi::AV_PIX_FMT_BAYER_RGGB16BE;
    BAYER_GBRG16LE => ffi::AV_PIX_FMT_BAYER_GBRG16LE;
    BAYER_GBRG16BE => ffi::AV_PIX_FMT_BAYER_GBRG16BE;
    BAYER_GRBG16LE => ffi::AV_PIX_FMT_BAYER_GRBG16LE;
    BAYER_GRBG16BE => ffi::AV_PIX_FMT_BAYER_GRBG16BE;
    #[cfg(feature = "ffmpeg6")]
    XVMC => ffi::AV_PIX_FMT_XVMC;
    YUV440P10LE => ffi::AV_PIX_FMT_YUV440P10LE;
    YUV440P10BE => ffi::AV_PIX_FMT_YUV440P10BE;
    YUV440P12LE => ffi::AV_PIX_FMT_YUV440P12LE;
    YUV440P12BE => ffi::AV_PIX_FMT_YUV440P12BE;
    AYUV64LE => ffi::AV_PIX_FMT_AYUV64LE;
    AYUV64BE => ffi::AV_PIX_FMT_AYUV64BE;
    VIDEOTOOLBOX => ffi::AV_PIX_FMT_VIDEOTOOLBOX;
    P010LE => ffi::AV_PIX_FMT_P010LE;
    P010BE => ffi::AV_PIX_FMT_P010BE;
    GBRAP12BE => ffi::AV_PIX_FMT_GBRAP12BE;
    GBRAP12LE => ffi::AV_PIX_FMT_GBRAP12LE;
    GBRAP10BE => ffi::AV_PIX_FMT_GBRAP10BE;
    GBRAP10LE => ffi::AV_PIX_FMT_GBRAP10LE;
    MEDIACODEC => ffi::AV_PIX_FMT_MEDIACODEC;
    GRAY12BE => ffi::AV_PIX_FMT_GRAY12BE;
    GRAY12LE => ffi::AV_PIX_FMT_GRAY12LE;
    GRAY10BE => ffi::AV_PIX_FMT_GRAY10BE;
    GRAY10LE => ffi::AV_PIX_FMT_GRAY10LE;
    P016LE => ffi::AV_PIX_FMT_P016LE;
    P016BE => ffi::AV_PIX_FMT_P016BE;
    D3D11 => ffi::AV_PIX_FMT_D3D11;
    GRAY9BE => ffi::AV_PIX_FMT_GRAY9BE;
    GRAY9LE => ffi::AV_PIX_FMT_GRAY9LE;
    GBRPF32BE => ffi::AV_PIX_FMT_GBRPF32BE;
    GBRPF32LE => ffi::AV_PIX_FMT_GBRPF32LE;
    GBRAPF32BE => ffi::AV_PIX_FMT_GBRAPF32BE;
    GBRAPF32LE => ffi::AV_PIX_FMT_GBRAPF32LE;
    DRM_PRIME => ffi::AV_PIX_FMT_DRM_PRIME;
    OPENCL => ffi::AV_PIX_FMT_OPENCL;
    GRAY14BE => ffi::AV_PIX_FMT_GRAY14BE;
    GRAY14LE => ffi::AV_PIX_FMT_GRAY14LE;
    GRAYF32BE => ffi::AV_PIX_FMT_GRAYF32BE;
    GRAYF32LE => ffi::AV_PIX_FMT_GRAYF32LE;
    YUVA422P12BE => ffi::AV_PIX_FMT_YUVA422P12BE;
    YUVA422P12LE => ffi::AV_PIX_FMT_YUVA422P12LE;
    YUVA444P12BE => ffi::AV_PIX_FMT_YUVA444P12BE;
    YUVA444P12LE => ffi::AV_PIX_FMT_YUVA444P12LE;
    NV24 => ffi::AV_PIX_FMT_NV24;
    NV42 => ffi::AV_PIX_FMT_NV42;
    VULKAN => ffi::AV_PIX_FMT_VULKAN;
    Y210BE => ffi::AV_PIX_FMT_Y210BE;
    Y210LE => ffi::AV_PIX_FMT_Y210LE;
    X2RGB10LE => ffi::AV_PIX_FMT_X2RGB10LE;
    X2RGB10BE => ffi::AV_PIX_FMT_X2RGB10BE;
    X2BGR10LE => ffi::AV_PIX_FMT_X2BGR10LE;
    X2BGR10BE => ffi::AV_PIX_FMT_X2BGR10BE;
    P210BE => ffi::AV_PIX_FMT_P210BE;
    P210LE => ffi::AV_PIX_FMT_P210LE;
    P410BE => ffi::AV_PIX_FMT_P410BE;
    P410LE => ffi::AV_PIX_FMT_P410LE;
    P216BE => ffi::AV_PIX_FMT_P216BE;
    P216LE => ffi::AV_PIX_FMT_P216LE;
    P416BE => ffi::AV_PIX_FMT_P416BE;
    P416LE => ffi::AV_PIX_FMT_P416LE;
    VUYA => ffi::AV_PIX_FMT_VUYA;
    RGBAF16BE => ffi::AV_PIX_FMT_RGBAF16BE;
    RGBAF16LE => ffi::AV_PIX_FMT_RGBAF16LE;
    VUYX => ffi::AV_PIX_FMT_VUYX;
    P012LE => ffi::AV_PIX_FMT_P012LE;
    P012BE => ffi::AV_PIX_FMT_P012BE;
    Y212BE => ffi::AV_PIX_FMT_Y212BE;
    Y212LE => ffi::AV_PIX_FMT_Y212LE;
    XV30BE => ffi::AV_PIX_FMT_XV30BE;
    XV30LE => ffi::AV_PIX_FMT_XV30LE;
    XV36BE => ffi::AV_PIX_FMT_XV36BE;
    XV36LE => ffi::AV_PIX_FMT_XV36LE;
    RGBF32BE => ffi::AV_PIX_FMT_RGBF32BE;
    RGBF32LE => ffi::AV_PIX_FMT_RGBF32LE;
    RGBAF32BE => ffi::AV_PIX_FMT_RGBAF32BE;
    RGBAF32LE => ffi::AV_PIX_FMT_RGBAF32LE;
    P212BE => ffi::AV_PIX_FMT_P212BE;
    P212LE => ffi::AV_PIX_FMT_P212LE;
    P412BE => ffi::AV_PIX_FMT_P412BE;
    P412LE => ffi::AV_PIX_FMT_P412LE;
    GBRAP14BE => ffi::AV_PIX_FMT_GBRAP14BE;
    GBRAP14LE => ffi::AV_PIX_FMT_GBRAP14LE;
    #[cfg(any(feature = "ffmpeg7", feature = "ffmpeg8", feature = "ffmpeg9"))]
    D3D12 => ffi::AV_PIX_FMT_D3D12;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    AYUV => ffi::AV_PIX_FMT_AYUV;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    UYVA => ffi::AV_PIX_FMT_UYVA;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    VYU444 => ffi::AV_PIX_FMT_VYU444;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    V30XBE => ffi::AV_PIX_FMT_V30XBE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    V30XLE => ffi::AV_PIX_FMT_V30XLE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    RGBF16BE => ffi::AV_PIX_FMT_RGBF16BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    RGBF16LE => ffi::AV_PIX_FMT_RGBF16LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    RGBA128BE => ffi::AV_PIX_FMT_RGBA128BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    RGBA128LE => ffi::AV_PIX_FMT_RGBA128LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    RGB96BE => ffi::AV_PIX_FMT_RGB96BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    RGB96LE => ffi::AV_PIX_FMT_RGB96LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    Y216BE => ffi::AV_PIX_FMT_Y216BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    Y216LE => ffi::AV_PIX_FMT_Y216LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    XV48BE => ffi::AV_PIX_FMT_XV48BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    XV48LE => ffi::AV_PIX_FMT_XV48LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRPF16BE => ffi::AV_PIX_FMT_GBRPF16BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRPF16LE => ffi::AV_PIX_FMT_GBRPF16LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRAPF16BE => ffi::AV_PIX_FMT_GBRAPF16BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRAPF16LE => ffi::AV_PIX_FMT_GBRAPF16LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GRAYF16BE => ffi::AV_PIX_FMT_GRAYF16BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GRAYF16LE => ffi::AV_PIX_FMT_GRAYF16LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    AMF_SURFACE => ffi::AV_PIX_FMT_AMF_SURFACE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GRAY32BE => ffi::AV_PIX_FMT_GRAY32BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GRAY32LE => ffi::AV_PIX_FMT_GRAY32LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YAF32BE => ffi::AV_PIX_FMT_YAF32BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YAF32LE => ffi::AV_PIX_FMT_YAF32LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YAF16BE => ffi::AV_PIX_FMT_YAF16BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YAF16LE => ffi::AV_PIX_FMT_YAF16LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRAP32BE => ffi::AV_PIX_FMT_GBRAP32BE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRAP32LE => ffi::AV_PIX_FMT_GBRAP32LE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YUV444P10MSBBE => ffi::AV_PIX_FMT_YUV444P10MSBBE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YUV444P10MSBLE => ffi::AV_PIX_FMT_YUV444P10MSBLE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YUV444P12MSBBE => ffi::AV_PIX_FMT_YUV444P12MSBBE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    YUV444P12MSBLE => ffi::AV_PIX_FMT_YUV444P12MSBLE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRP10MSBBE => ffi::AV_PIX_FMT_GBRP10MSBBE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRP10MSBLE => ffi::AV_PIX_FMT_GBRP10MSBLE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRP12MSBBE => ffi::AV_PIX_FMT_GBRP12MSBBE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    GBRP12MSBLE => ffi::AV_PIX_FMT_GBRP12MSBLE;
    #[cfg(any(feature = "ffmpeg8", feature = "ffmpeg9"))]
    OHCODEC => ffi::AV_PIX_FMT_OHCODEC;
});

//////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////

/// Formats whose samples cannot be expressed as whole sample planes.
///
/// Bitstream formats pack components into sub-byte fields, paletted formats
/// address a separate palette, and hardware formats keep their samples on the
/// device. Both [`PixelFormat::data_layout`] and
/// [`PixelFormat::is_plane_storable`] turn on this one predicate, so the two
/// cannot disagree about which formats are storable.
fn has_no_sample_planes(desc: &AVPixFmtDescriptorRef) -> bool {
    const UNSUPPORTED: u32 =
        ffi::AV_PIX_FMT_FLAG_BITSTREAM | ffi::AV_PIX_FMT_FLAG_PAL | ffi::AV_PIX_FMT_FLAG_HWACCEL;
    desc.flags as u32 & UNSUPPORTED != 0
}

impl PixelFormat {
    /// 获取像素格式描述符；未知/无效格式返回错误而非 panic。
    pub fn descriptor(&self) -> Result<AVPixFmtDescriptorRef> {
        AVPixFmtDescriptorRef::get((*self).into()).ok_or_else(|| {
            RsmediaError::msg(format!(
                "No pix_fmt descriptor for {}",
                self.get_pix_fmt_name()
            ))
        })
    }

    /// 获取像素格式名称（FFmpeg 返回静态字符串，借用即可，避免每次分配 String）
    pub fn get_pix_fmt_name(&self) -> String {
        unsafe {
            let name = ffi::av_get_pix_fmt_name((*self).into());
            if name.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(name)
                    .to_string_lossy()
                    .into_owned()
            }
        }
    }

    /// get number of planes in pix_fmt
    pub fn count_planes(&self) -> Result<i32> {
        let cnt = unsafe { ffi::av_pix_fmt_count_planes((*self).into()) };
        if cnt < 0 {
            return Err(RsmediaError::msg(format!(
                "Failed to get plane count:{cnt}"
            )));
        }
        Ok(cnt)
    }

    /// Whether this format can be stored as whole sample planes at all.
    ///
    /// The size-independent counterpart of [`Self::data_layout`], for callers
    /// judging a format before a frame size is known: bitstream, paletted and
    /// hardware formats have no host samples per plane whatever the size, while
    /// every other format does at every non-zero size.
    pub fn is_plane_storable(self) -> bool {
        AVPixFmtDescriptorRef::get(self.into()).is_some_and(|desc| !has_no_sample_planes(&desc))
    }

    /// The data layout this pixel format uses at `width` x `height`.
    ///
    /// Derived entirely from FFmpeg's pixel-format descriptor, so this method
    /// knows no format by name and every format FFmpeg describes gets a layout
    /// for free:
    ///
    /// * a format **without** `AV_PIX_FMT_FLAG_PLANAR` is interleaved — one
    ///   `(height, width, n)` array whose per-pixel element run `n` follows from
    ///   the component steps and chroma subsampling (`rgb24` → 3, `rgba` → 4,
    ///   `yuyv422` → 2, `gray8` → 1);
    /// * a **planar** format is one array per plane, each chroma plane carrying
    ///   its own subsampled size, taken from `log2_chroma_w` / `log2_chroma_h`.
    ///
    /// `None` means the format cannot be expressed as whole sample arrays at
    /// this size: bitstream, paletted and hardware formats (whose components are
    /// not whole samples), or a zero dimension.
    pub fn data_layout(self, width: usize, height: usize) -> Option<DataLayout> {
        if width == 0 || height == 0 {
            return None;
        }
        let desc = AVPixFmtDescriptorRef::get(self.into())?;
        if has_no_sample_planes(&desc) {
            return None;
        }

        let components = desc.nb_components as usize;
        if components == 0 {
            return None;
        }
        let element_bytes = element_bytes(&desc)?;

        // FFmpeg models planes 1 and 2 as the chroma planes (`av_image_fill_plane_sizes`
        // does the same) and reports the chroma shifts as 0 for RGB formats, so this
        // predicate is a no-op for them.
        let w_shift = |plane: usize| match plane {
            1 | 2 => desc.log2_chroma_w as u32,
            _ => 0,
        };
        let h_shift = |plane: usize| match plane {
            1 | 2 => desc.log2_chroma_h as u32,
            _ => 0,
        };
        let ceil_shift = |value: usize, shift: u32| (value + (1usize << shift) - 1) >> shift;

        if desc.flags & ffi::AV_PIX_FMT_FLAG_PLANAR as u64 == 0 {
            // Interleaved: one plane, `bytes_per_pixel` elements per pixel. The
            // largest component step spans one pixel per `2^log2_chroma_w` columns
            // for a horizontally subsampled format (`yuyv422`: 4 >> 1 = 2 bytes).
            let max_step = (0..components)
                .map(|c| desc.comp[c].step.max(0) as usize)
                .max()?;
            let bytes_per_pixel = max_step >> desc.log2_chroma_w;
            if bytes_per_pixel == 0 || !bytes_per_pixel.is_multiple_of(element_bytes) {
                return None;
            }
            Some(DataLayout::Interleaved {
                rows: height,
                cols: width,
                components: bytes_per_pixel / element_bytes,
            })
        } else {
            let plane_count = self.count_planes().ok()? as usize;
            let mut planes = Vec::with_capacity(plane_count);
            for plane in 0..plane_count {
                // Elements this plane stores per luma column, e.g. 1 for a planar
                // Y/U/V plane and 2 for a semi-planar `NV12` chroma plane.
                let plane_bytes: usize = (0..components)
                    .filter(|&c| desc.comp[c].plane.max(0) as usize == plane)
                    .map(|c| component_bytes(&desc, c))
                    .sum();
                if plane_bytes == 0 || !plane_bytes.is_multiple_of(element_bytes) {
                    return None;
                }
                planes.push((
                    ceil_shift(height, h_shift(plane)),
                    ceil_shift(width, w_shift(plane)) * (plane_bytes / element_bytes),
                ));
            }
            Some(DataLayout::Planar(planes))
        }
    }

    /// Bytes one component of this format occupies in memory.
    ///
    /// Components are whole bytes in every format this crate models, so 8-bit
    /// formats give 1 and 9..16-bit ones give 2 (`YUV420P10LE`, `P016LE`, ...).
    /// This is the element size a [`MediaFrame`](crate::frame::MediaFrame)'s type
    /// parameter has to match.
    ///
    /// `None` for formats without host samples (hardware formats).
    pub fn bytes_per_component(self) -> Option<usize> {
        let desc = AVPixFmtDescriptorRef::get(self.into())?;
        element_bytes(&desc)
    }
}

/// Bytes a component of `desc` occupies, rounded up to whole bytes: 10-bit and
/// 12-bit samples live in 16-bit elements.
fn component_bytes(desc: &ffi::AVPixFmtDescriptor, component: usize) -> usize {
    (desc.comp[component].depth.max(0) as usize).div_ceil(8)
}

/// Bytes one element of `desc`'s format occupies, i.e. its widest component.
fn element_bytes(desc: &ffi::AVPixFmtDescriptor) -> Option<usize> {
    (0..desc.nb_components as usize)
        .map(|component| component_bytes(desc, component))
        .max()
        .filter(|&bytes| bytes > 0)
}

/// 返回最佳像素格式，或错误
pub fn find_best_pix_fmt(
    dst_pix_fmt1: PixelFormat,
    dst_pix_fmt2: PixelFormat,
    src_pix_fmt: PixelFormat,
    has_alpha: bool,
) -> Result<PixelFormat> {
    let alpha = if has_alpha { 1 } else { 0 };

    // 返回的是**选中的像素格式**（`AV_PIX_FMT_NONE` 表示无法选择）。`loss_ptr`
    // 才承载"会损失什么"的位掩码，这里不需要，故传 NULL。
    let best = unsafe {
        ffi::av_find_best_pix_fmt_of_2(
            dst_pix_fmt1.into(),
            dst_pix_fmt2.into(),
            src_pix_fmt.into(),
            alpha,
            std::ptr::null_mut(),
        )
    };

    match PixelFormat::from_ffi_checked(best) {
        // 返回 `AV_PIX_FMT_NONE`（或本 crate 未收录的值）都表示"没有可用的目标格式"。
        None | Some(PixelFormat::NONE) => Err(RsmediaError::msg(format!(
            "Failed to find a best pixel format among the candidates (got {best})"
        ))),
        Some(fmt) => Ok(fmt),
    }
}

/// Find the best pixel format to convert to given a certain source pixel format.
/// this function searches which of the given pixel formats should be used to suffer the least amount of loss.
/// The pixel formats from which it chooses one, are determined by the pix_fmt_list parameter.
pub fn find_codec_best_pix_fmt(
    pix_fmt_list: &[PixelFormat],
    src_pix_fmt: PixelFormat,
    has_alpha: bool,
) -> Result<PixelFormat> {
    let pix_fmts = pix_fmt_list.as_ptr() as *const _;
    let alpha = if has_alpha { 1 } else { 0 };
    let ret = unsafe {
        ffi::avcodec_find_best_pix_fmt_of_list(
            pix_fmts,
            src_pix_fmt.into(),
            alpha,
            std::ptr::null_mut(),
        )
    };
    if ret < 0 {
        return Err(RsmediaError::msg(format!(
            "Failed to find codec best pix fmt, ret: {ret}"
        )));
    }
    Ok(PixelFormat::from(ret))
}

/// 计算像素格式转换的损失值（封装 av_get_pix_fmt_loss）
///
/// # 参数
/// - `dst_pix_fmt`: 目标像素格式
/// - `src_pix_fmt`: 源像素格式
/// - `has_alpha`:   是否考虑 alpha 通道
///
/// # 返回值
/// 返回非负损失值，或错误
pub fn get_pix_fmt_loss(
    dst_pix_fmt: PixelFormat,
    src_pix_fmt: PixelFormat,
    has_alpha: bool,
) -> Result<i32> {
    let loss = unsafe {
        ffi::av_get_pix_fmt_loss(dst_pix_fmt.into(), src_pix_fmt.into(), has_alpha as i32)
    };

    if loss < 0 {
        return Err(RsmediaError::msg(format!(
            "Failed to get pix fmt loss, ret: {loss}"
        )));
    }

    Ok(loss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pixel_format() -> Result<()> {
        // 9. 测试最佳像素格式查找
        let best_fmt = find_best_pix_fmt(
            PixelFormat::RGB24,
            PixelFormat::BGR24,
            PixelFormat::YUV420P,
            false,
        )?;
        assert_ne!(best_fmt, PixelFormat::NONE);
        // 在候选集中选出最佳格式，其结果一定属于候选集
        assert!(
            matches!(
                best_fmt,
                PixelFormat::RGB24 | PixelFormat::BGR24 | PixelFormat::YUV420P
            ),
            "best pixel format should be one of the candidates, got {best_fmt:?}"
        );

        // 10. 测试像素格式损失计算
        let loss = get_pix_fmt_loss(PixelFormat::RGB24, PixelFormat::YUV420P, false)?;
        assert!(loss >= 0, "pixel format loss should be non-negative");

        // 相同的格式转换应无损失
        let same_loss = get_pix_fmt_loss(PixelFormat::RGB24, PixelFormat::RGB24, false)?;
        assert_eq!(same_loss, 0, "same format conversion should have zero loss");

        Ok(())
    }

    #[test]
    fn test_format_conversion() -> Result<()> {
        let formats = vec![
            PixelFormat::RGB24,
            PixelFormat::BGR24,
            PixelFormat::YUV420P,
            PixelFormat::RGBA,
        ];

        // 测试所有格式组合的转换
        for &src_fmt in &formats {
            for &dst_fmt in &formats {
                if src_fmt != dst_fmt {
                    let loss = get_pix_fmt_loss(dst_fmt, src_fmt, true)?;
                    assert!(
                        loss >= 0,
                        "loss for {src_fmt:?}->{dst_fmt:?} should be non-negative"
                    );
                } else {
                    // 不同格式但相等的情况不存在；此处保证自身转换无损失
                    let loss = get_pix_fmt_loss(dst_fmt, src_fmt, true)?;
                    assert_eq!(loss, 0, "self conversion should have zero loss");
                }
            }
        }

        Ok(())
    }
}
