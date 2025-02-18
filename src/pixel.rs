use crate::error::MediaError;
use rsmpeg::ffi;

type Result<T> = std::result::Result<T, MediaError>;

/// Number of pixel formats
/// DO NOT USE THIS if you want to link with shared libav* because the number of formats might differ between versions
pub const AV_PIX_FMT_NB: i32 = 228;

/// Pixel format definitions in bindings.
#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    /// Invalid pixel format value
    None,

    /// planar YUV 4:2:0, 12bpp, (1 Cr & Cb sample per 2x2 Y samples)
    YUV420P,

    /// packed YUV 4:2:2, 16bpp, Y0 Cb Y1 Cr
    YUYV422,

    /// packed RGB 8:8:8, 24bpp, RGBRGB...
    RGB24,

    /// packed RGB 8:8:8, 24bpp, BGRBGR...
    BGR24,

    /// planar YUV 4:2:2, 16bpp, (1 Cr & Cb sample per 2x1 Y samples)
    YUV422P,

    /// planar YUV 4:4:4, 24bpp, (1 Cr & Cb sample per 1x1 Y samples)
    YUV444P,

    /// planar YUV 4:1:0,  9bpp, (1 Cr & Cb sample per 4x4 Y samples)
    YUV410P,

    /// planar YUV 4:1:1, 12bpp, (1 Cr & Cb sample per 4x1 Y samples)
    YUV411P,

    /// Y, 8bpp
    GRAY8,

    /// Y, 1bpp, 0 is white, 1 is black, in each byte pixels are ordered from the msb to the lsb
    MONOWHITE,

    /// Y, 1bpp, 0 is black, 1 is white, in each byte pixels are ordered from the msb to the lsb
    MONOBLACK,

    /// 8 bits with AV_PIX_FMT_RGB32 palette
    PAL8,

    /// planar YUV 4:2:0, 12bpp, full scale (JPEG), deprecated in favor of AV_PIX_FMT_YUV420P and setting color_range
    YUVJ420P,

    /// planar YUV 4:2:2, 16bpp, full scale (JPEG), deprecated in favor of AV_PIX_FMT_YUV422P and setting color_range
    YUVJ422P,

    /// planar YUV 4:4:4, 24bpp, full scale (JPEG), deprecated in favor of AV_PIX_FMT_YUV444P and setting color_range
    YUVJ444P,

    /// packed YUV 4:2:2, 16bpp, Cb Y0 Cr Y1
    UYVY422,

    /// packed YUV 4:1:1, 12bpp, Cb Y0 Y1 Cr Y2 Y3
    UYYVYY411,

    /// packed RGB 3:3:2, 8bpp, (msb)2B 3G 3R(lsb)
    BGR8,

    /// packed RGB 1:2:1 bitstream, 4bpp, (msb)1B 2G 1R(lsb), a byte contains two pixels, the first pixel in the byte is the one composed by the 4 msb bits
    BGR4,

    /// packed RGB 1:2:1, 8bpp, (msb)1B 2G 1R(lsb)
    BGR4_BYTE,

    /// packed RGB 3:3:2, 8bpp, (msb)3R 3G 2B(lsb)
    RGB8,

    /// packed RGB 1:2:1 bitstream, 4bpp, (msb)1R 2G 1B(lsb), a byte contains two pixels, the first pixel in the byte is the one composed by the 4 msb bits
    RGB4,

    /// packed RGB 1:2:1, 8bpp, (msb)1R 2G 1B(lsb)
    RGB4_BYTE,

    /// planar YUV 4:2:0, 12bpp, 1 plane for Y and 1 plane for the UV components, which are interleaved (first byte U and the following byte V)
    NV12,

    /// as above, but U and V bytes are swapped
    NV21,

    /// packed ARGB 8:8:8:8, 32bpp, ARGBARGB...
    ARGB,

    /// packed RGBA 8:8:8:8, 32bpp, RGBARGBA...
    RGBA,

    /// packed ABGR 8:8:8:8, 32bpp, ABGRABGR...
    ABGR,

    /// packed BGRA 8:8:8:8, 32bpp, BGRABGRA...
    BGRA,

    /// Y, 16bpp, big-endian
    GRAY16BE,

    /// Y, 16bpp, little-endian
    GRAY16LE,

    /// planar YUV 4:4:0 (1 Cr & Cb sample per 1x2 Y samples)
    YUV440P,

    /// planar YUV 4:4:0 full scale (JPEG), deprecated in favor of AV_PIX_FMT_YUV440P and setting color_range
    YUVJ440P,

    /// planar YUV 4:2:0, 20bpp, (1 Cr & Cb sample per 2x2 Y & A samples)
    YUVA420P,

    /// packed RGB 16:16:16, 48bpp, 16R, 16G, 16B, the 2-byte value for each R/G/B component is stored as big-endian
    RGB48BE,

    /// packed RGB 16:16:16, 48bpp, 16R, 16G, 16B, the 2-byte value for each R/G/B component is stored as little-endian
    RGB48LE,

    /// packed RGB 5:6:5, 16bpp, (msb) 5R 6G 5B(lsb), big-endian
    RGB565BE,

    /// packed RGB 5:6:5, 16bpp, (msb) 5R 6G 5B(lsb), little-endian
    RGB565LE,

    /// packed RGB 5:5:5, 16bpp, (msb)1X 5R 5G 5B(lsb), big-endian, X=unused/undefined
    RGB555BE,

    /// packed RGB 5:5:5, 16bpp, (msb)1X 5R 5G 5B(lsb), little-endian, X=unused/undefined
    RGB555LE,

    /// packed BGR 5:6:5, 16bpp, (msb) 5B 6G 5R(lsb), big-endian
    BGR565BE,

    /// packed BGR 5:6:5, 16bpp, (msb) 5B 6G 5R(lsb), little-endian
    BGR565LE,

    /// packed BGR 5:5:5, 16bpp, (msb)1X 5B 5G 5R(lsb), big-endian, X=unused/undefined
    BGR555BE,

    /// packed BGR 5:5:5, 16bpp, (msb)1X 5B 5G 5R(lsb), little-endian, X=unused/undefined
    BGR555LE,

    /// Hardware acceleration through VA-API, data[3] contains a VASurfaceID.
    VAAPI,

    /// planar YUV 4:2:0, 24bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian
    YUV420P16LE,

    /// planar YUV 4:2:0, 24bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian
    YUV420P16BE,

    /// planar YUV 4:2:2, 32bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian
    YUV422P16LE,

    /// planar YUV 4:2:2, 32bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian
    YUV422P16BE,

    /// planar YUV 4:4:4, 48bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian
    YUV444P16LE,

    /// planar YUV 4:4:4, 48bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian
    YUV444P16BE,

    /// HW decoding through DXVA2, Picture.data[3] contains a LPDIRECT3DSURFACE9 pointer
    DXVA2_VLD,

    /// packed RGB 4:4:4, 16bpp, (msb)4X 4R 4G 4B(lsb), little-endian, X=unused/undefined
    RGB444LE,

    /// packed RGB 4:4:4, 16bpp, (msb)4X 4R 4G 4B(lsb), big-endian, X=unused/undefined
    RGB444BE,

    /// packed BGR 4:4:4, 16bpp, (msb)4X 4B 4G 4R(lsb), little-endian, X=unused/undefined
    BGR444LE,

    /// packed BGR 4:4:4, 16bpp, (msb)4X 4B 4G 4R(lsb), big-endian, X=unused/undefined
    BGR444BE,

    /// 8 bits gray, 8 bits alpha
    YA8,

    /// alias YA8
    Y400A,

    /// alias YA8
    GRAY8A,

    /// packed RGB 16:16:16, 48bpp, 16B, 16G, 16R, the 2-byte value for each R/G/B component is stored as big-endian
    BGR48BE,

    /// packed RGB 16:16:16, 48bpp, 16B, 16G, 16R, the 2-byte value for each R/G/B component is stored as little-endian
    BGR48LE,

    /// planar YUV 4:2:0, 13.5bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian
    YUV420P9BE,

    /// planar YUV 4:2:0, 13.5bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian
    YUV420P9LE,

    /// planar YUV 4:2:0, 15bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian
    YUV420P10BE,

    /// planar YUV 4:2:0, 15bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian
    YUV420P10LE,

    /// planar YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian
    YUV422P10BE,

    /// planar YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian
    YUV422P10LE,

    /// planar YUV 4:4:4, 27bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian
    YUV444P9BE,

    /// planar YUV 4:4:4, 27bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian
    YUV444P9LE,

    /// planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian
    YUV444P10BE,

    /// planar YUV 4:4:4, 30bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian
    YUV444P10LE,

    /// planar YUV 4:2:2, 18bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian
    YUV422P9BE,

    /// planar YUV 4:2:2, 18bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian
    YUV422P9LE,

    /// planar GBR 4:4:4 24bpp
    GBRP,

    /// alias GBRP
    GBR24P,

    /// planar GBR 4:4:4 27bpp, big-endian
    GBRP9BE,

    /// planar GBR 4:4:4 27bpp, little-endian
    GBRP9LE,

    /// planar GBR 4:4:4 30bpp, big-endian
    GBRP10BE,

    /// planar GBR 4:4:4 30bpp, little-endian
    GBRP10LE,

    /// planar GBR 4:4:4 48bpp, big-endian
    GBRP16BE,

    /// planar GBR 4:4:4 48bpp, little-endian
    GBRP16LE,

    /// planar YUV 4:2:2 24bpp, (1 Cr & Cb sample per 2x1 Y & A samples)
    YUVA422P,

    /// planar YUV 4:4:4 32bpp, (1 Cr & Cb sample per 1x1 Y & A samples)
    YUVA444P,

    /// planar YUV 4:2:0 22.5bpp, (1 Cr & Cb sample per 2x2 Y & A samples), big-endian
    YUVA420P9BE,

    /// planar YUV 4:2:0 22.5bpp, (1 Cr & Cb sample per 2x2 Y & A samples), little-endian
    YUVA420P9LE,

    /// planar YUV 4:2:2 27bpp, (1 Cr & Cb sample per 2x1 Y & A samples), big-endian
    YUVA422P9BE,

    /// planar YUV 4:2:2 27bpp, (1 Cr & Cb sample per 2x1 Y & A samples), little-endian
    YUVA422P9LE,

    /// planar YUV 4:4:4 36bpp, (1 Cr & Cb sample per 1x1 Y & A samples), big-endian
    YUVA444P9BE,

    /// planar YUV 4:4:4 36bpp, (1 Cr & Cb sample per 1x1 Y & A samples), little-endian
    YUVA444P9LE,

    /// planar YUV 4:2:0 25bpp, (1 Cr & Cb sample per 2x2 Y & A samples, big-endian)
    YUVA420P10BE,

    /// planar YUV 4:2:0 25bpp, (1 Cr & Cb sample per 2x2 Y & A samples, little-endian)
    YUVA420P10LE,

    /// planar YUV 4:2:2 30bpp, (1 Cr & Cb sample per 2x1 Y & A samples, big-endian)
    YUVA422P10BE,

    /// planar YUV 4:2:2 30bpp, (1 Cr & Cb sample per 2x1 Y & A samples, little-endian)
    YUVA422P10LE,

    /// planar YUV 4:4:4 40bpp, (1 Cr & Cb sample per 1x1 Y & A samples, big-endian)
    YUVA444P10BE,

    /// planar YUV 4:4:4 40bpp, (1 Cr & Cb sample per 1x1 Y & A samples, little-endian)
    YUVA444P10LE,

    /// planar YUV 4:2:0 40bpp, (1 Cr & Cb sample per 2x2 Y & A samples, big-endian)
    YUVA420P16BE,

    /// planar YUV 4:2:0 40bpp, (1 Cr & Cb sample per 2x2 Y & A samples, little-endian)
    YUVA420P16LE,

    /// planar YUV 4:2:2 48bpp, (1 Cr & Cb sample per 2x1 Y & A samples, big-endian)
    YUVA422P16BE,

    /// planar YUV 4:2:2 48bpp, (1 Cr & Cb sample per 2x1 Y & A samples, little-endian)
    YUVA422P16LE,

    /// planar YUV 4:4:4 64bpp, (1 Cr & Cb sample per 1x1 Y & A samples, big-endian)
    YUVA444P16BE,

    /// planar YUV 4:4:4 64bpp, (1 Cr & Cb sample per 1x1 Y & A samples, little-endian)
    YUVA444P16LE,

    /// HW acceleration through VDPAU, Picture.data[3] contains a VdpVideoSurface
    VDPAU,

    /// packed XYZ 4:4:4, 36 bpp, (msb) 12X, 12Y, 12Z (lsb), the 2-byte value for each X/Y/Z is stored as little-endian, the 4 lower bits are set to 0
    XYZ12LE,

    /// packed XYZ 4:4:4, 36 bpp, (msb) 12X, 12Y, 12Z (lsb), the 2-byte value for each X/Y/Z is stored as big-endian, the 4 lower bits are set to 0
    XYZ12BE,

    /// interleaved chroma YUV 4:2:2, 16bpp, (1 Cr & Cb sample per 2x1 Y samples)
    NV16,

    /// interleaved chroma YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian
    NV20LE,

    /// interleaved chroma YUV 4:2:2, 20bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian
    NV20BE,

    /// packed RGBA 16:16:16:16, 64bpp, 16R, 16G, 16B, 16A, the 2-byte value for each R/G/B/A component is stored as big-endian
    RGBA64BE,

    /// packed RGBA 16:16:16:16, 64bpp, 16R, 16G, 16B, 16A, the 2-byte value for each R/G/B/A component is stored as little-endian
    RGBA64LE,

    /// packed RGBA 16:16:16:16, 64bpp, 16B, 16G, 16R, 16A, the 2-byte value for each R/G/B/A component is stored as big-endian
    BGRA64BE,

    /// packed RGBA 16:16:16:16, 64bpp, 16B, 16G, 16R, 16A, the 2-byte value for each R/G/B/A component is stored as little-endian
    BGRA64LE,

    /// packed YUV 4:2:2, 16bpp, Y0 Cr Y1 Cb
    YVYU422,

    /// 16 bits gray, 16 bits alpha (big-endian)
    YA16BE,

    /// 16 bits gray, 16 bits alpha (little-endian)
    YA16LE,

    /// planar GBRA 4:4:4:4 32bpp
    GBRAP,

    /// planar GBRA 4:4:4:4 64bpp, big-endian
    GBRAP16BE,

    /// planar GBRA 4:4:4:4 64bpp, little-endian
    GBRAP16LE,

    /// HW acceleration through QSV, data[3] contains a pointer to the mfxFrameSurface1 structure.
    ///
    /// Before FFmpeg 5.0:
    /// mfxFrameSurface1.Data.MemId contains a pointer when importing
    /// the following frames as QSV frames:
    ///
    /// VAAPI:
    /// mfxFrameSurface1.Data.MemId contains a pointer to VASurfaceID
    ///
    /// DXVA2:
    /// mfxFrameSurface1.Data.MemId contains a pointer to IDirect3DSurface9
    ///
    /// FFmpeg 5.0 and above:
    /// mfxFrameSurface1.Data.MemId contains a pointer to the mfxHDLPair
    /// structure when importing the following frames as QSV frames:
    ///
    /// VAAPI:
    /// mfxHDLPair.first contains a VASurfaceID pointer.
    /// mfxHDLPair.second is always MFX_INFINITE.
    ///
    /// DXVA2:
    /// mfxHDLPair.first contains IDirect3DSurface9 pointer.
    /// mfxHDLPair.second is always MFX_INFINITE.
    ///
    /// D3D11:
    /// mfxHDLPair.first contains a ID3D11Texture2D pointer.
    /// mfxHDLPair.second contains the texture array index of the frame if the
    /// ID3D11Texture2D is an array texture, or always MFX_INFINITE if it is a
    /// normal texture.
    QSV,

    /// HW acceleration though MMAL, data[3] contains a pointer to the
    /// MMAL_BUFFER_HEADER_T structure.
    MMAL,

    /// HW decoding through Direct3D11 via old API, Picture.data[3] contains a ID3D11VideoDecoderOutputView pointer
    D3D11VA_VLD,

    /// HW acceleration through CUDA. data[i] contain CUdeviceptr pointers
    /// exactly as for system memory frames.
    CUDA,

    /// packed RGB 8:8:8, 32bpp, XRGBXRGB... X=unused/undefined
    ZRGB,

    /// packed RGB 8:8:8, 32bpp, RGBXRGBX... X=unused/undefined
    RGBZ,

    /// packed BGR 8:8:8, 32bpp, XBGRXBGR... X=unused/undefined
    ZBGR,

    /// packed BGR 8:8:8, 32bpp, BGRXBGRX... X=unused/undefined
    BGRZ,

    /// planar YUV 4:2:0, 18bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian
    YUV420P12BE,

    /// planar YUV 4:2:0, 18bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian
    YUV420P12LE,

    /// planar YUV 4:2:0, 21bpp, (1 Cr & Cb sample per 2x2 Y samples), big-endian
    YUV420P14BE,

    /// planar YUV 4:2:0, 21bpp, (1 Cr & Cb sample per 2x2 Y samples), little-endian
    YUV420P14LE,

    /// planar YUV 4:2:2, 24bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian
    YUV422P12BE,

    /// planar YUV 4:2:2, 24bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian
    YUV422P12LE,

    /// planar YUV 4:2:2, 28bpp, (1 Cr & Cb sample per 2x1 Y samples), big-endian
    YUV422P14BE,

    /// planar YUV 4:2:2, 28bpp, (1 Cr & Cb sample per 2x1 Y samples), little-endian
    YUV422P14LE,

    /// planar YUV 4:4:4, 36bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian
    YUV444P12BE,

    /// planar YUV 4:4:4, 36bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian
    YUV444P12LE,

    /// planar YUV 4:4:4, 42bpp, (1 Cr & Cb sample per 1x1 Y samples), big-endian
    YUV444P14BE,

    /// planar YUV 4:4:4, 42bpp, (1 Cr & Cb sample per 1x1 Y samples), little-endian
    YUV444P14LE,

    /// planar GBR 4:4:4 36bpp, big-endian
    GBRP12BE,

    /// planar GBR 4:4:4 36bpp, little-endian
    GBRP12LE,

    /// planar GBR 4:4:4 42bpp, big-endian
    GBRP14BE,

    /// planar GBR 4:4:4 42bpp, little-endian
    GBRP14LE,

    /// planar YUV 4:1:1, 12bpp, (1 Cr & Cb sample per 4x1 Y samples) full scale (JPEG), deprecated in favor of AV_PIX_FMT_YUV411P and setting color_range
    YUVJ411P,

    /// bayer, BGBG..(odd line), GRGR..(even line), 8-bit samples
    BAYER_BGGR8,

    /// bayer, RGRG..(odd line), GBGB..(even line), 8-bit samples
    BAYER_RGGB8,

    /// bayer, GBGB..(odd line), RGRG..(even line), 8-bit samples
    BAYER_GBRG8,

    /// bayer, GRGR..(odd line), BGBG..(even line), 8-bit samples
    BAYER_GRBG8,

    /// bayer, BGBG..(odd line), GRGR..(even line), 16-bit samples, little-endian
    BAYER_BGGR16LE,

    /// bayer, BGBG..(odd line), GRGR..(even line), 16-bit samples, big-endian
    BAYER_BGGR16BE,

    /// bayer, RGRG..(odd line), GBGB..(even line), 16-bit samples, little-endian
    BAYER_RGGB16LE,

    /// bayer, RGRG..(odd line), GBGB..(even line), 16-bit samples, big-endian
    BAYER_RGGB16BE,

    /// bayer, GBGB..(odd line), RGRG..(even line), 16-bit samples, little-endian
    BAYER_GBRG16LE,

    /// bayer, GBGB..(odd line), RGRG..(even line), 16-bit samples, big-endian
    BAYER_GBRG16BE,

    /// bayer, GRGR..(odd line), BGBG..(even line), 16-bit samples, little-endian
    BAYER_GRBG16LE,

    /// bayer, GRGR..(odd line), BGBG..(even line), 16-bit samples, big-endian
    BAYER_GRBG16BE,

    /// planar YUV 4:4:0, 20bpp, (1 Cr & Cb sample per 1x2 Y samples), little-endian
    YUV440P10LE,

    /// planar YUV 4:4:0, 20bpp, (1 Cr & Cb sample per 1x2 Y samples), big-endian
    YUV440P10BE,

    /// planar YUV 4:4:0, 24bpp, (1 Cr & Cb sample per 1x2 Y samples), little-endian
    YUV440P12LE,

    /// planar YUV 4:4:0, 24bpp, (1 Cr & Cb sample per 1x2 Y samples), big-endian
    YUV440P12BE,

    /// packed AYUV 4:4:4, 64bpp (1 Cr & Cb sample per 1x1 Y & A samples), little-endian
    AYUV64LE,

    /// packed AYUV 4:4:4, 64bpp (1 Cr & Cb sample per 1x1 Y & A samples), big-endian
    AYUV64BE,

    /// hardware decoding through Videotoolbox
    VIDEOTOOLBOX,

    /// like NV12, with 10bpp per component, data in the high bits, zeros in the low bits, little-endian
    P010LE,

    /// like NV12, with 10bpp per component, data in the high bits, zeros in the low bits, big-endian
    P010BE,

    /// planar GBR 4:4:4:4 48bpp, big-endian
    GBRAP12BE,

    /// planar GBR 4:4:4:4 48bpp, little-endian
    GBRAP12LE,

    /// planar GBR 4:4:4:4 40bpp, big-endian
    GBRAP10BE,

    /// planar GBR 4:4:4:4 40bpp, little-endian
    GBRAP10LE,

    /// hardware decoding through MediaCodec
    MEDIACODEC,

    /// Y, 12bpp, big-endian
    GRAY12BE,

    /// Y, 12bpp, little-endian
    GRAY12LE,

    /// Y, 10bpp, big-endian
    GRAY10BE,

    /// Y, 10bpp, little-endian
    GRAY10LE,

    /// like NV12, with 16bpp per component, little-endian
    P016LE,

    /// like NV12, with 16bpp per component, big-endian
    P016BE,

    /// Hardware surfaces for Direct3D11.
    ///
    /// This is preferred over the legacy AV_PIX_FMT_D3D11VA_VLD. The new D3D11
    /// hwaccel API and filtering support AV_PIX_FMT_D3D11 only.
    ///
    /// data[0] contains a ID3D11Texture2D pointer, and data[1] contains the
    /// texture array index of the frame as intptr_t if the ID3D11Texture2D is
    /// an array texture (or always 0 if it's a normal texture).
    D3D11,

    /// Y, 9bpp, big-endian
    GRAY9BE,

    /// Y, 9bpp, little-endian
    GRAY9LE,

    /// IEEE-754 single precision planar GBR 4:4:4, 96bpp, big-endian
    GBRPF32BE,

    /// IEEE-754 single precision planar GBR 4:4:4, 96bpp, little-endian
    GBRPF32LE,

    /// IEEE-754 single precision planar GBRA 4:4:4:4, 128bpp, big-endian
    GBRAPF32BE,

    /// IEEE-754 single precision planar GBRA 4:4:4:4, 128bpp, little-endian
    GBRAPF32LE,

    /// DRM-managed buffers exposed through PRIME buffer sharing.
    ///
    /// data[0] points to an AVDRMFrameDescriptor.
    DRM_PRIME,

    /// Hardware surfaces for OpenCL.
    ///
    /// data[i] contain 2D image objects (typed in C as cl_mem, used
    /// in OpenCL as image2d_t) for each plane of the surface.
    OPENCL,

    /// Y, 14bpp, big-endian
    GRAY14BE,

    /// Y, 14bpp, little-endian
    GRAY14LE,

    /// IEEE-754 single precision Y, 32bpp, big-endian
    GRAYF32BE,

    /// IEEE-754 single precision Y, 32bpp, little-endian
    GRAYF32LE,

    /// planar YUV 4:2:2, 24bpp, (1 Cr & Cb sample per 2x1 Y samples), 12b alpha, big-endian
    YUVA422P12BE,

    /// planar YUV 4:2:2, 24bpp, (1 Cr & Cb sample per 2x1 Y samples), 12b alpha, little-endian
    YUVA422P12LE,

    /// planar YUV 4:4:4, 36bpp, (1 Cr & Cb sample per 1x1 Y samples), 12b alpha, big-endian
    YUVA444P12BE,

    /// planar YUV 4:4:4, 36bpp, (1 Cr & Cb sample per 1x1 Y samples), 12b alpha, little-endian
    YUVA444P12LE,

    /// planar YUV 4:4:4, 24bpp, 1 plane for Y and 1 plane for the UV components, which are interleaved (first byte U and the following byte V)
    NV24,

    /// as above, but U and V bytes are swapped
    NV42,

    /// Vulkan hardware images.
    ///
    /// data[0] points to an AVVkFrame
    VULKAN,

    /// packed YUV 4:2:2 like YUYV422, 20bpp, data in the high bits, big-endian
    Y210BE,

    /// packed YUV 4:2:2 like YUYV422, 20bpp, data in the high bits, little-endian
    Y210LE,

    /// packed RGB 10:10:10, 30bpp, (msb)2X 10R 10G 10B(lsb), little-endian, X=unused/undefined
    X2RGB10LE,

    /// packed RGB 10:10:10, 30bpp, (msb)2X 10R 10G 10B(lsb), big-endian, X=unused/undefined
    X2RGB10BE,

    /// packed BGR 10:10:10, 30bpp, (msb)2X 10B 10G 10R(lsb), little-endian, X=unused/undefined
    X2BGR10LE,

    /// packed BGR 10:10:10, 30bpp, (msb)2X 10B 10G 10R(lsb), big-endian, X=unused/undefined
    X2BGR10BE,

    /// interleaved chroma YUV 4:2:2, 20bpp, data in the high bits, big-endian
    P210BE,

    /// interleaved chroma YUV 4:2:2, 20bpp, data in the high bits, little-endian
    P210LE,

    /// interleaved chroma YUV 4:4:4, 30bpp, data in the high bits, big-endian
    P410BE,

    /// interleaved chroma YUV 4:4:4, 30bpp, data in the high bits, little-endian
    P410LE,

    /// interleaved chroma YUV 4:2:2, 32bpp, big-endian
    P216BE,

    /// interleaved chroma YUV 4:2:2, 32bpp, little-endian
    P216LE,

    /// interleaved chroma YUV 4:4:4, 48bpp, big-endian
    P416BE,

    /// interleaved chroma YUV 4:4:4, 48bpp, little-endian
    P416LE,

    /// packed VUYA 4:4:4, 32bpp, VUYAVUYA...
    VUYA,

    /// IEEE-754 half precision packed RGBA 16:16:16:16, 64bpp, RGBARGBA..., big-endian
    RGBAF16BE,

    /// IEEE-754 half precision packed RGBA 16:16:16:16, 64bpp, RGBARGBA..., little-endian
    RGBAF16LE,

    /// packed VUYX 4:4:4, 32bpp, Variant of VUYA where alpha channel is left undefined
    VUYX,

    /// like NV12, with 12bpp per component, data in the high bits, zeros in the low bits, little-endian
    P012LE,

    /// like NV12, with 12bpp per component, data in the high bits, zeros in the low bits, big-endian
    P012BE,

    /// packed YUV 4:2:2 like YUYV422, 24bpp, data in the high bits, zeros in the low bits, big-endian
    Y212BE,

    /// packed YUV 4:2:2 like YUYV422, 24bpp, data in the high bits, zeros in the low bits, little-endian
    Y212LE,

    /// packed XVYU 4:4:4, 32bpp, (msb)2X 10V 10Y 10U(lsb), big-endian, variant of Y410 where alpha channel is left undefined
    XV30BE,

    /// packed XVYU 4:4:4, 32bpp, (msb)2X 10V 10Y 10U(lsb), little-endian, variant of Y410 where alpha channel is left undefined
    XV30LE,

    /// packed XVYU 4:4:4, 48bpp, data in the high bits, zeros in the low bits, big-endian, variant of Y412 where alpha channel is left undefined
    XV36BE,

    /// packed XVYU 4:4:4, 48bpp, data in the high bits, zeros in the low bits, little-endian, variant of Y412 where alpha channel is left undefined
    XV36LE,

    /// IEEE-754 single precision packed RGB 32:32:32, 96bpp, RGBRGB..., big-endian
    RGBF32BE,

    /// IEEE-754 single precision packed RGB 32:32:32, 96bpp, RGBRGB..., little-endian
    RGBF32LE,

    /// IEEE-754 single precision packed RGBA 32:32:32:32, 128bpp, RGBARGBA..., big-endian
    RGBAF32BE,

    /// IEEE-754 single precision packed RGBA 32:32:32:32, 128bpp, RGBARGBA..., little-endian
    RGBAF32LE,

    /// interleaved chroma YUV 4:2:2, 24bpp, data in the high bits, big-endian
    P212BE,

    /// interleaved chroma YUV 4:2:2, 24bpp, data in the high bits, little-endian
    P212LE,

    /// interleaved chroma YUV 4:4:4, 36bpp, data in the high bits, big-endian
    P412BE,

    /// interleaved chroma YUV 4:4:4, 36bpp, data in the high bits, little-endian
    P412LE,

    /// planar GBR 4:4:4:4 56bpp, big-endian
    GBRAP14BE,

    /// planar GBR 4:4:4:4 56bpp, little-endian
    GBRAP14LE,

    /// Hardware surfaces for Direct3D 12.
    ///
    /// data[0] points to an AVD3D12VAFrame
    D3D12,
}

impl PixelFormat {
    /// Convert from FFmpeg's AV_PIX_FMT_* constants
    pub fn from_raw(raw: i32) -> Result<Self> {
        match raw {
            ffi::AV_PIX_FMT_NONE => Ok(Self::None),
            ffi::AV_PIX_FMT_YUV420P => Ok(Self::YUV420P),
            ffi::AV_PIX_FMT_YUYV422 => Ok(Self::YUYV422),
            ffi::AV_PIX_FMT_RGB24 => Ok(Self::RGB24),
            ffi::AV_PIX_FMT_BGR24 => Ok(Self::BGR24),
            ffi::AV_PIX_FMT_YUV422P => Ok(Self::YUV422P),
            ffi::AV_PIX_FMT_YUV444P => Ok(Self::YUV444P),
            ffi::AV_PIX_FMT_YUV410P => Ok(Self::YUV410P),
            ffi::AV_PIX_FMT_YUV411P => Ok(Self::YUV411P),
            ffi::AV_PIX_FMT_GRAY8 => Ok(Self::GRAY8),
            ffi::AV_PIX_FMT_MONOWHITE => Ok(Self::MONOWHITE),
            ffi::AV_PIX_FMT_MONOBLACK => Ok(Self::MONOBLACK),
            ffi::AV_PIX_FMT_PAL8 => Ok(Self::PAL8),
            ffi::AV_PIX_FMT_YUVJ420P => Ok(Self::YUVJ420P),
            ffi::AV_PIX_FMT_YUVJ422P => Ok(Self::YUVJ422P),
            ffi::AV_PIX_FMT_YUVJ444P => Ok(Self::YUVJ444P),
            ffi::AV_PIX_FMT_UYVY422 => Ok(Self::UYVY422),
            ffi::AV_PIX_FMT_UYYVYY411 => Ok(Self::UYYVYY411),
            ffi::AV_PIX_FMT_BGR8 => Ok(Self::BGR8),
            ffi::AV_PIX_FMT_BGR4 => Ok(Self::BGR4),
            ffi::AV_PIX_FMT_BGR4_BYTE => Ok(Self::BGR4_BYTE),
            ffi::AV_PIX_FMT_RGB8 => Ok(Self::RGB8),
            ffi::AV_PIX_FMT_RGB4 => Ok(Self::RGB4),
            ffi::AV_PIX_FMT_RGB4_BYTE => Ok(Self::RGB4_BYTE),
            ffi::AV_PIX_FMT_NV12 => Ok(Self::NV12),
            ffi::AV_PIX_FMT_NV21 => Ok(Self::NV21),
            ffi::AV_PIX_FMT_ARGB => Ok(Self::ARGB),
            ffi::AV_PIX_FMT_RGBA => Ok(Self::RGBA),
            ffi::AV_PIX_FMT_ABGR => Ok(Self::ABGR),
            ffi::AV_PIX_FMT_BGRA => Ok(Self::BGRA),
            ffi::AV_PIX_FMT_GRAY16BE => Ok(Self::GRAY16BE),
            ffi::AV_PIX_FMT_GRAY16LE => Ok(Self::GRAY16LE),
            ffi::AV_PIX_FMT_YUV440P => Ok(Self::YUV440P),
            ffi::AV_PIX_FMT_YUVJ440P => Ok(Self::YUVJ440P),
            ffi::AV_PIX_FMT_YUVA420P => Ok(Self::YUVA420P),
            ffi::AV_PIX_FMT_RGB48BE => Ok(Self::RGB48BE),
            ffi::AV_PIX_FMT_RGB48LE => Ok(Self::RGB48LE),
            ffi::AV_PIX_FMT_RGB565BE => Ok(Self::RGB565BE),
            ffi::AV_PIX_FMT_RGB565LE => Ok(Self::RGB565LE),
            ffi::AV_PIX_FMT_RGB555BE => Ok(Self::RGB555BE),
            ffi::AV_PIX_FMT_RGB555LE => Ok(Self::RGB555LE),
            ffi::AV_PIX_FMT_BGR565BE => Ok(Self::BGR565BE),
            ffi::AV_PIX_FMT_BGR565LE => Ok(Self::BGR565LE),
            ffi::AV_PIX_FMT_BGR555BE => Ok(Self::BGR555BE),
            ffi::AV_PIX_FMT_BGR555LE => Ok(Self::BGR555LE),
            ffi::AV_PIX_FMT_VAAPI => Ok(Self::VAAPI),
            ffi::AV_PIX_FMT_YUV420P16LE => Ok(Self::YUV420P16LE),
            ffi::AV_PIX_FMT_YUV420P16BE => Ok(Self::YUV420P16BE),
            ffi::AV_PIX_FMT_YUV422P16LE => Ok(Self::YUV422P16LE),
            ffi::AV_PIX_FMT_YUV422P16BE => Ok(Self::YUV422P16BE),
            ffi::AV_PIX_FMT_YUV444P16LE => Ok(Self::YUV444P16LE),
            ffi::AV_PIX_FMT_YUV444P16BE => Ok(Self::YUV444P16BE),
            ffi::AV_PIX_FMT_DXVA2_VLD => Ok(Self::DXVA2_VLD),
            ffi::AV_PIX_FMT_RGB444LE => Ok(Self::RGB444LE),
            ffi::AV_PIX_FMT_RGB444BE => Ok(Self::RGB444BE),
            ffi::AV_PIX_FMT_BGR444LE => Ok(Self::BGR444LE),
            ffi::AV_PIX_FMT_BGR444BE => Ok(Self::BGR444BE),
            ffi::AV_PIX_FMT_YA8 => Ok(Self::YA8),
            // alias for YA8
            // ffi::AV_PIX_FMT_Y400A => Ok(Self::Y400A),
            // ffi::AV_PIX_FMT_GRAY8A => Ok(Self::GRAY8A),
            ffi::AV_PIX_FMT_BGR48BE => Ok(Self::BGR48BE),
            ffi::AV_PIX_FMT_BGR48LE => Ok(Self::BGR48LE),
            ffi::AV_PIX_FMT_YUV420P9BE => Ok(Self::YUV420P9BE),
            ffi::AV_PIX_FMT_YUV420P9LE => Ok(Self::YUV420P9LE),
            ffi::AV_PIX_FMT_YUV420P10BE => Ok(Self::YUV420P10BE),
            ffi::AV_PIX_FMT_YUV420P10LE => Ok(Self::YUV420P10LE),
            ffi::AV_PIX_FMT_YUV422P10BE => Ok(Self::YUV422P10BE),
            ffi::AV_PIX_FMT_YUV422P10LE => Ok(Self::YUV422P10LE),
            ffi::AV_PIX_FMT_YUV444P9BE => Ok(Self::YUV444P9BE),
            ffi::AV_PIX_FMT_YUV444P9LE => Ok(Self::YUV444P9LE),
            ffi::AV_PIX_FMT_YUV444P10BE => Ok(Self::YUV444P10BE),
            ffi::AV_PIX_FMT_YUV444P10LE => Ok(Self::YUV444P10LE),
            ffi::AV_PIX_FMT_YUV422P9BE => Ok(Self::YUV422P9BE),
            ffi::AV_PIX_FMT_YUV422P9LE => Ok(Self::YUV422P9LE),
            ffi::AV_PIX_FMT_GBRP => Ok(Self::GBRP),
            // alias for GBRP
            // ffi::AV_PIX_FMT_GBR24P => Ok(Self::GBR24P),
            ffi::AV_PIX_FMT_GBRP9BE => Ok(Self::GBRP9BE),
            ffi::AV_PIX_FMT_GBRP9LE => Ok(Self::GBRP9LE),
            ffi::AV_PIX_FMT_GBRP10BE => Ok(Self::GBRP10BE),
            ffi::AV_PIX_FMT_GBRP10LE => Ok(Self::GBRP10LE),
            ffi::AV_PIX_FMT_GBRP16BE => Ok(Self::GBRP16BE),
            ffi::AV_PIX_FMT_GBRP16LE => Ok(Self::GBRP16LE),
            ffi::AV_PIX_FMT_YUVA422P => Ok(Self::YUVA422P),
            ffi::AV_PIX_FMT_YUVA444P => Ok(Self::YUVA444P),
            ffi::AV_PIX_FMT_YUVA420P9BE => Ok(Self::YUVA420P9BE),
            ffi::AV_PIX_FMT_YUVA420P9LE => Ok(Self::YUVA420P9LE),
            ffi::AV_PIX_FMT_YUVA422P9BE => Ok(Self::YUVA422P9BE),
            ffi::AV_PIX_FMT_YUVA422P9LE => Ok(Self::YUVA422P9LE),
            ffi::AV_PIX_FMT_YUVA444P9BE => Ok(Self::YUVA444P9BE),
            ffi::AV_PIX_FMT_YUVA444P9LE => Ok(Self::YUVA444P9LE),
            ffi::AV_PIX_FMT_YUVA420P10BE => Ok(Self::YUVA420P10BE),
            ffi::AV_PIX_FMT_YUVA420P10LE => Ok(Self::YUVA420P10LE),
            ffi::AV_PIX_FMT_YUVA422P10BE => Ok(Self::YUVA422P10BE),
            ffi::AV_PIX_FMT_YUVA422P10LE => Ok(Self::YUVA422P10LE),
            ffi::AV_PIX_FMT_YUVA444P10BE => Ok(Self::YUVA444P10BE),
            ffi::AV_PIX_FMT_YUVA444P10LE => Ok(Self::YUVA444P10LE),
            ffi::AV_PIX_FMT_YUVA420P16BE => Ok(Self::YUVA420P16BE),
            ffi::AV_PIX_FMT_YUVA420P16LE => Ok(Self::YUVA420P16LE),
            ffi::AV_PIX_FMT_YUVA422P16BE => Ok(Self::YUVA422P16BE),
            ffi::AV_PIX_FMT_YUVA422P16LE => Ok(Self::YUVA422P16LE),
            ffi::AV_PIX_FMT_YUVA444P16BE => Ok(Self::YUVA444P16BE),
            ffi::AV_PIX_FMT_YUVA444P16LE => Ok(Self::YUVA444P16LE),
            ffi::AV_PIX_FMT_VDPAU => Ok(Self::VDPAU),
            ffi::AV_PIX_FMT_XYZ12LE => Ok(Self::XYZ12LE),
            ffi::AV_PIX_FMT_XYZ12BE => Ok(Self::XYZ12BE),
            ffi::AV_PIX_FMT_NV16 => Ok(Self::NV16),
            ffi::AV_PIX_FMT_NV20LE => Ok(Self::NV20LE),
            ffi::AV_PIX_FMT_NV20BE => Ok(Self::NV20BE),
            ffi::AV_PIX_FMT_RGBA64BE => Ok(Self::RGBA64BE),
            ffi::AV_PIX_FMT_RGBA64LE => Ok(Self::RGBA64LE),
            ffi::AV_PIX_FMT_BGRA64BE => Ok(Self::BGRA64BE),
            ffi::AV_PIX_FMT_BGRA64LE => Ok(Self::BGRA64LE),
            ffi::AV_PIX_FMT_YVYU422 => Ok(Self::YVYU422),
            ffi::AV_PIX_FMT_YA16BE => Ok(Self::YA16BE),
            ffi::AV_PIX_FMT_YA16LE => Ok(Self::YA16LE),
            ffi::AV_PIX_FMT_GBRAP => Ok(Self::GBRAP),
            ffi::AV_PIX_FMT_GBRAP16BE => Ok(Self::GBRAP16BE),
            ffi::AV_PIX_FMT_GBRAP16LE => Ok(Self::GBRAP16LE),
            ffi::AV_PIX_FMT_QSV => Ok(Self::QSV),
            ffi::AV_PIX_FMT_MMAL => Ok(Self::MMAL),
            ffi::AV_PIX_FMT_D3D11VA_VLD => Ok(Self::D3D11VA_VLD),
            ffi::AV_PIX_FMT_CUDA => Ok(Self::CUDA),
            ffi::AV_PIX_FMT_0RGB => Ok(Self::ZRGB),
            ffi::AV_PIX_FMT_RGB0 => Ok(Self::RGBZ),
            ffi::AV_PIX_FMT_0BGR => Ok(Self::ZBGR),
            ffi::AV_PIX_FMT_BGR0 => Ok(Self::BGRZ),
            ffi::AV_PIX_FMT_YUV420P12BE => Ok(Self::YUV420P12BE),
            ffi::AV_PIX_FMT_YUV420P12LE => Ok(Self::YUV420P12LE),
            ffi::AV_PIX_FMT_YUV420P14BE => Ok(Self::YUV420P14BE),
            ffi::AV_PIX_FMT_YUV420P14LE => Ok(Self::YUV420P14LE),
            ffi::AV_PIX_FMT_YUV422P12BE => Ok(Self::YUV422P12BE),
            ffi::AV_PIX_FMT_YUV422P12LE => Ok(Self::YUV422P12LE),
            ffi::AV_PIX_FMT_YUV422P14BE => Ok(Self::YUV422P14BE),
            ffi::AV_PIX_FMT_YUV422P14LE => Ok(Self::YUV422P14LE),
            ffi::AV_PIX_FMT_YUV444P12BE => Ok(Self::YUV444P12BE),
            ffi::AV_PIX_FMT_YUV444P12LE => Ok(Self::YUV444P12LE),
            ffi::AV_PIX_FMT_YUV444P14BE => Ok(Self::YUV444P14BE),
            ffi::AV_PIX_FMT_YUV444P14LE => Ok(Self::YUV444P14LE),
            ffi::AV_PIX_FMT_GBRP12BE => Ok(Self::GBRP12BE),
            ffi::AV_PIX_FMT_GBRP12LE => Ok(Self::GBRP12LE),
            ffi::AV_PIX_FMT_GBRP14BE => Ok(Self::GBRP14BE),
            ffi::AV_PIX_FMT_GBRP14LE => Ok(Self::GBRP14LE),
            ffi::AV_PIX_FMT_YUVJ411P => Ok(Self::YUVJ411P),
            ffi::AV_PIX_FMT_BAYER_BGGR8 => Ok(Self::BAYER_BGGR8),
            ffi::AV_PIX_FMT_BAYER_RGGB8 => Ok(Self::BAYER_RGGB8),
            ffi::AV_PIX_FMT_BAYER_GBRG8 => Ok(Self::BAYER_GBRG8),
            ffi::AV_PIX_FMT_BAYER_GRBG8 => Ok(Self::BAYER_GRBG8),
            ffi::AV_PIX_FMT_BAYER_BGGR16LE => Ok(Self::BAYER_BGGR16LE),
            ffi::AV_PIX_FMT_BAYER_BGGR16BE => Ok(Self::BAYER_BGGR16BE),
            ffi::AV_PIX_FMT_BAYER_RGGB16LE => Ok(Self::BAYER_RGGB16LE),
            ffi::AV_PIX_FMT_BAYER_RGGB16BE => Ok(Self::BAYER_RGGB16BE),
            ffi::AV_PIX_FMT_BAYER_GBRG16LE => Ok(Self::BAYER_GBRG16LE),
            ffi::AV_PIX_FMT_BAYER_GBRG16BE => Ok(Self::BAYER_GBRG16BE),
            ffi::AV_PIX_FMT_BAYER_GRBG16LE => Ok(Self::BAYER_GRBG16LE),
            ffi::AV_PIX_FMT_BAYER_GRBG16BE => Ok(Self::BAYER_GRBG16BE),
            ffi::AV_PIX_FMT_YUV440P10LE => Ok(Self::YUV440P10LE),
            ffi::AV_PIX_FMT_YUV440P10BE => Ok(Self::YUV440P10BE),
            ffi::AV_PIX_FMT_YUV440P12LE => Ok(Self::YUV440P12LE),
            ffi::AV_PIX_FMT_YUV440P12BE => Ok(Self::YUV440P12BE),
            ffi::AV_PIX_FMT_AYUV64LE => Ok(Self::AYUV64LE),
            ffi::AV_PIX_FMT_AYUV64BE => Ok(Self::AYUV64BE),
            ffi::AV_PIX_FMT_VIDEOTOOLBOX => Ok(Self::VIDEOTOOLBOX),
            ffi::AV_PIX_FMT_P010LE => Ok(Self::P010LE),
            ffi::AV_PIX_FMT_P010BE => Ok(Self::P010BE),
            ffi::AV_PIX_FMT_GBRAP12BE => Ok(Self::GBRAP12BE),
            ffi::AV_PIX_FMT_GBRAP12LE => Ok(Self::GBRAP12LE),
            ffi::AV_PIX_FMT_GBRAP10BE => Ok(Self::GBRAP10BE),
            ffi::AV_PIX_FMT_GBRAP10LE => Ok(Self::GBRAP10LE),
            ffi::AV_PIX_FMT_MEDIACODEC => Ok(Self::MEDIACODEC),
            ffi::AV_PIX_FMT_GRAY12BE => Ok(Self::GRAY12BE),
            ffi::AV_PIX_FMT_GRAY12LE => Ok(Self::GRAY12LE),
            ffi::AV_PIX_FMT_GRAY10BE => Ok(Self::GRAY10BE),
            ffi::AV_PIX_FMT_GRAY10LE => Ok(Self::GRAY10LE),
            ffi::AV_PIX_FMT_P016LE => Ok(Self::P016LE),
            ffi::AV_PIX_FMT_P016BE => Ok(Self::P016BE),
            ffi::AV_PIX_FMT_D3D11 => Ok(Self::D3D11),
            ffi::AV_PIX_FMT_GRAY9BE => Ok(Self::GRAY9BE),
            ffi::AV_PIX_FMT_GRAY9LE => Ok(Self::GRAY9LE),
            ffi::AV_PIX_FMT_GBRPF32BE => Ok(Self::GBRPF32BE),
            ffi::AV_PIX_FMT_GBRPF32LE => Ok(Self::GBRPF32LE),
            ffi::AV_PIX_FMT_GBRAPF32BE => Ok(Self::GBRAPF32BE),
            ffi::AV_PIX_FMT_GBRAPF32LE => Ok(Self::GBRAPF32LE),
            ffi::AV_PIX_FMT_DRM_PRIME => Ok(Self::DRM_PRIME),
            ffi::AV_PIX_FMT_OPENCL => Ok(Self::OPENCL),
            ffi::AV_PIX_FMT_GRAY14BE => Ok(Self::GRAY14BE),
            ffi::AV_PIX_FMT_GRAY14LE => Ok(Self::GRAY14LE),
            ffi::AV_PIX_FMT_GRAYF32BE => Ok(Self::GRAYF32BE),
            ffi::AV_PIX_FMT_GRAYF32LE => Ok(Self::GRAYF32LE),
            ffi::AV_PIX_FMT_YUVA422P12BE => Ok(Self::YUVA422P12BE),
            ffi::AV_PIX_FMT_YUVA422P12LE => Ok(Self::YUVA422P12LE),
            ffi::AV_PIX_FMT_YUVA444P12BE => Ok(Self::YUVA444P12BE),
            ffi::AV_PIX_FMT_YUVA444P12LE => Ok(Self::YUVA444P12LE),
            ffi::AV_PIX_FMT_NV24 => Ok(Self::NV24),
            ffi::AV_PIX_FMT_NV42 => Ok(Self::NV42),
            ffi::AV_PIX_FMT_VULKAN => Ok(Self::VULKAN),
            ffi::AV_PIX_FMT_Y210BE => Ok(Self::Y210BE),
            ffi::AV_PIX_FMT_Y210LE => Ok(Self::Y210LE),
            ffi::AV_PIX_FMT_X2RGB10LE => Ok(Self::X2RGB10LE),
            ffi::AV_PIX_FMT_X2RGB10BE => Ok(Self::X2RGB10BE),
            ffi::AV_PIX_FMT_X2BGR10LE => Ok(Self::X2BGR10LE),
            ffi::AV_PIX_FMT_X2BGR10BE => Ok(Self::X2BGR10BE),
            ffi::AV_PIX_FMT_P210BE => Ok(Self::P210BE),
            ffi::AV_PIX_FMT_P210LE => Ok(Self::P210LE),
            ffi::AV_PIX_FMT_P410BE => Ok(Self::P410BE),
            ffi::AV_PIX_FMT_P410LE => Ok(Self::P410LE),
            ffi::AV_PIX_FMT_P216BE => Ok(Self::P216BE),
            ffi::AV_PIX_FMT_P216LE => Ok(Self::P216LE),
            ffi::AV_PIX_FMT_P416BE => Ok(Self::P416BE),
            ffi::AV_PIX_FMT_P416LE => Ok(Self::P416LE),
            ffi::AV_PIX_FMT_VUYA => Ok(Self::VUYA),
            ffi::AV_PIX_FMT_RGBAF16BE => Ok(Self::RGBAF16BE),
            ffi::AV_PIX_FMT_RGBAF16LE => Ok(Self::RGBAF16LE),
            ffi::AV_PIX_FMT_VUYX => Ok(Self::VUYX),
            ffi::AV_PIX_FMT_P012LE => Ok(Self::P012LE),
            ffi::AV_PIX_FMT_P012BE => Ok(Self::P012BE),
            ffi::AV_PIX_FMT_Y212BE => Ok(Self::Y212BE),
            ffi::AV_PIX_FMT_Y212LE => Ok(Self::Y212LE),
            ffi::AV_PIX_FMT_XV30BE => Ok(Self::XV30BE),
            ffi::AV_PIX_FMT_XV30LE => Ok(Self::XV30LE),
            ffi::AV_PIX_FMT_XV36BE => Ok(Self::XV36BE),
            ffi::AV_PIX_FMT_XV36LE => Ok(Self::XV36LE),
            ffi::AV_PIX_FMT_RGBF32BE => Ok(Self::RGBF32BE),
            ffi::AV_PIX_FMT_RGBF32LE => Ok(Self::RGBF32LE),
            ffi::AV_PIX_FMT_RGBAF32BE => Ok(Self::RGBAF32BE),
            ffi::AV_PIX_FMT_RGBAF32LE => Ok(Self::RGBAF32LE),
            ffi::AV_PIX_FMT_P212BE => Ok(Self::P212BE),
            ffi::AV_PIX_FMT_P212LE => Ok(Self::P212LE),
            ffi::AV_PIX_FMT_P412BE => Ok(Self::P412BE),
            ffi::AV_PIX_FMT_P412LE => Ok(Self::P412LE),
            ffi::AV_PIX_FMT_GBRAP14BE => Ok(Self::GBRAP14BE),
            ffi::AV_PIX_FMT_GBRAP14LE => Ok(Self::GBRAP14LE),
            ffi::AV_PIX_FMT_D3D12 => Ok(Self::D3D12),

            // unsupported pixel formats not included in ffmpeg
            _ => Err(MediaError::InvalidPixelFormat),
        }
    }

    /// Convert to FFmpeg's AV_PIX_FMT_* constants
    pub fn into_raw(self) -> i32 {
        match self {
            Self::None => ffi::AV_PIX_FMT_NONE,
            Self::YUV420P => ffi::AV_PIX_FMT_YUV420P,
            Self::YUYV422 => ffi::AV_PIX_FMT_YUYV422,
            Self::RGB24 => ffi::AV_PIX_FMT_RGB24,
            Self::BGR24 => ffi::AV_PIX_FMT_BGR24,
            Self::YUV422P => ffi::AV_PIX_FMT_YUV422P,
            Self::YUV444P => ffi::AV_PIX_FMT_YUV444P,
            Self::YUV410P => ffi::AV_PIX_FMT_YUV410P,
            Self::YUV411P => ffi::AV_PIX_FMT_YUV411P,
            Self::GRAY8 => ffi::AV_PIX_FMT_GRAY8,
            Self::MONOWHITE => ffi::AV_PIX_FMT_MONOWHITE,
            Self::MONOBLACK => ffi::AV_PIX_FMT_MONOBLACK,
            Self::PAL8 => ffi::AV_PIX_FMT_PAL8,
            Self::YUVJ420P => ffi::AV_PIX_FMT_YUVJ420P,
            Self::YUVJ422P => ffi::AV_PIX_FMT_YUVJ422P,
            Self::YUVJ444P => ffi::AV_PIX_FMT_YUVJ444P,
            Self::UYVY422 => ffi::AV_PIX_FMT_UYVY422,
            Self::UYYVYY411 => ffi::AV_PIX_FMT_UYYVYY411,
            Self::BGR8 => ffi::AV_PIX_FMT_BGR8,
            Self::BGR4 => ffi::AV_PIX_FMT_BGR4,
            Self::BGR4_BYTE => ffi::AV_PIX_FMT_BGR4_BYTE,
            Self::RGB8 => ffi::AV_PIX_FMT_RGB8,
            Self::RGB4 => ffi::AV_PIX_FMT_RGB4,
            Self::RGB4_BYTE => ffi::AV_PIX_FMT_RGB4_BYTE,
            Self::NV12 => ffi::AV_PIX_FMT_NV12,
            Self::NV21 => ffi::AV_PIX_FMT_NV21,
            Self::ARGB => ffi::AV_PIX_FMT_ARGB,
            Self::RGBA => ffi::AV_PIX_FMT_RGBA,
            Self::ABGR => ffi::AV_PIX_FMT_ABGR,
            Self::BGRA => ffi::AV_PIX_FMT_BGRA,
            Self::GRAY16BE => ffi::AV_PIX_FMT_GRAY16BE,
            Self::GRAY16LE => ffi::AV_PIX_FMT_GRAY16LE,
            Self::YUV440P => ffi::AV_PIX_FMT_YUV440P,
            Self::YUVJ440P => ffi::AV_PIX_FMT_YUVJ440P,
            Self::YUVA420P => ffi::AV_PIX_FMT_YUVA420P,
            Self::RGB48BE => ffi::AV_PIX_FMT_RGB48BE,
            Self::RGB48LE => ffi::AV_PIX_FMT_RGB48LE,
            Self::RGB565BE => ffi::AV_PIX_FMT_RGB565BE,
            Self::RGB565LE => ffi::AV_PIX_FMT_RGB565LE,
            Self::RGB555BE => ffi::AV_PIX_FMT_RGB555BE,
            Self::RGB555LE => ffi::AV_PIX_FMT_RGB555LE,
            Self::BGR565BE => ffi::AV_PIX_FMT_BGR565BE,
            Self::BGR565LE => ffi::AV_PIX_FMT_BGR565LE,
            Self::BGR555BE => ffi::AV_PIX_FMT_BGR555BE,
            Self::BGR555LE => ffi::AV_PIX_FMT_BGR555LE,
            Self::VAAPI => ffi::AV_PIX_FMT_VAAPI,
            Self::YUV420P16LE => ffi::AV_PIX_FMT_YUV420P16LE,
            Self::YUV420P16BE => ffi::AV_PIX_FMT_YUV420P16BE,
            Self::YUV422P16LE => ffi::AV_PIX_FMT_YUV422P16LE,
            Self::YUV422P16BE => ffi::AV_PIX_FMT_YUV422P16BE,
            Self::YUV444P16LE => ffi::AV_PIX_FMT_YUV444P16LE,
            Self::YUV444P16BE => ffi::AV_PIX_FMT_YUV444P16BE,
            Self::DXVA2_VLD => ffi::AV_PIX_FMT_DXVA2_VLD,
            Self::RGB444LE => ffi::AV_PIX_FMT_RGB444LE,
            Self::RGB444BE => ffi::AV_PIX_FMT_RGB444BE,
            Self::BGR444LE => ffi::AV_PIX_FMT_BGR444LE,
            Self::BGR444BE => ffi::AV_PIX_FMT_BGR444BE,
            Self::YA8 => ffi::AV_PIX_FMT_YA8,
            Self::Y400A => ffi::AV_PIX_FMT_Y400A,
            Self::GRAY8A => ffi::AV_PIX_FMT_GRAY8A,
            Self::BGR48BE => ffi::AV_PIX_FMT_BGR48BE,
            Self::BGR48LE => ffi::AV_PIX_FMT_BGR48LE,
            Self::YUV420P9BE => ffi::AV_PIX_FMT_YUV420P9BE,
            Self::YUV420P9LE => ffi::AV_PIX_FMT_YUV420P9LE,
            Self::YUV420P10BE => ffi::AV_PIX_FMT_YUV420P10BE,
            Self::YUV420P10LE => ffi::AV_PIX_FMT_YUV420P10LE,
            Self::YUV422P10BE => ffi::AV_PIX_FMT_YUV422P10BE,
            Self::YUV422P10LE => ffi::AV_PIX_FMT_YUV422P10LE,
            Self::YUV444P9BE => ffi::AV_PIX_FMT_YUV444P9BE,
            Self::YUV444P9LE => ffi::AV_PIX_FMT_YUV444P9LE,
            Self::YUV444P10BE => ffi::AV_PIX_FMT_YUV444P10BE,
            Self::YUV444P10LE => ffi::AV_PIX_FMT_YUV444P10LE,
            Self::YUV422P9BE => ffi::AV_PIX_FMT_YUV422P9BE,
            Self::YUV422P9LE => ffi::AV_PIX_FMT_YUV422P9LE,
            Self::GBRP => ffi::AV_PIX_FMT_GBRP,
            Self::GBR24P => ffi::AV_PIX_FMT_GBR24P,
            Self::GBRP9BE => ffi::AV_PIX_FMT_GBRP9BE,
            Self::GBRP9LE => ffi::AV_PIX_FMT_GBRP9LE,
            Self::GBRP10BE => ffi::AV_PIX_FMT_GBRP10BE,
            Self::GBRP10LE => ffi::AV_PIX_FMT_GBRP10LE,
            Self::GBRP16BE => ffi::AV_PIX_FMT_GBRP16BE,
            Self::GBRP16LE => ffi::AV_PIX_FMT_GBRP16LE,
            Self::YUVA422P => ffi::AV_PIX_FMT_YUVA422P,
            Self::YUVA444P => ffi::AV_PIX_FMT_YUVA444P,
            Self::YUVA420P9BE => ffi::AV_PIX_FMT_YUVA420P9BE,
            Self::YUVA420P9LE => ffi::AV_PIX_FMT_YUVA420P9LE,
            Self::YUVA422P9BE => ffi::AV_PIX_FMT_YUVA422P9BE,
            Self::YUVA422P9LE => ffi::AV_PIX_FMT_YUVA422P9LE,
            Self::YUVA444P9BE => ffi::AV_PIX_FMT_YUVA444P9BE,
            Self::YUVA444P9LE => ffi::AV_PIX_FMT_YUVA444P9LE,
            Self::YUVA420P10BE => ffi::AV_PIX_FMT_YUVA420P10BE,
            Self::YUVA420P10LE => ffi::AV_PIX_FMT_YUVA420P10LE,
            Self::YUVA422P10BE => ffi::AV_PIX_FMT_YUVA422P10BE,
            Self::YUVA422P10LE => ffi::AV_PIX_FMT_YUVA422P10LE,
            Self::YUVA444P10BE => ffi::AV_PIX_FMT_YUVA444P10BE,
            Self::YUVA444P10LE => ffi::AV_PIX_FMT_YUVA444P10LE,
            Self::YUVA420P16BE => ffi::AV_PIX_FMT_YUVA420P16BE,
            Self::YUVA420P16LE => ffi::AV_PIX_FMT_YUVA420P16LE,
            Self::YUVA422P16BE => ffi::AV_PIX_FMT_YUVA422P16BE,
            Self::YUVA422P16LE => ffi::AV_PIX_FMT_YUVA422P16LE,
            Self::YUVA444P16BE => ffi::AV_PIX_FMT_YUVA444P16BE,
            Self::YUVA444P16LE => ffi::AV_PIX_FMT_YUVA444P16LE,
            Self::VDPAU => ffi::AV_PIX_FMT_VDPAU,
            Self::XYZ12LE => ffi::AV_PIX_FMT_XYZ12LE,
            Self::XYZ12BE => ffi::AV_PIX_FMT_XYZ12BE,
            Self::NV16 => ffi::AV_PIX_FMT_NV16,
            Self::NV20LE => ffi::AV_PIX_FMT_NV20LE,
            Self::NV20BE => ffi::AV_PIX_FMT_NV20BE,
            Self::RGBA64BE => ffi::AV_PIX_FMT_RGBA64BE,
            Self::RGBA64LE => ffi::AV_PIX_FMT_RGBA64LE,
            Self::BGRA64BE => ffi::AV_PIX_FMT_BGRA64BE,
            Self::BGRA64LE => ffi::AV_PIX_FMT_BGRA64LE,
            Self::YVYU422 => ffi::AV_PIX_FMT_YVYU422,
            Self::YA16BE => ffi::AV_PIX_FMT_YA16BE,
            Self::YA16LE => ffi::AV_PIX_FMT_YA16LE,
            Self::GBRAP => ffi::AV_PIX_FMT_GBRAP,
            Self::GBRAP16BE => ffi::AV_PIX_FMT_GBRAP16BE,
            Self::GBRAP16LE => ffi::AV_PIX_FMT_GBRAP16LE,
            Self::QSV => ffi::AV_PIX_FMT_QSV,
            Self::MMAL => ffi::AV_PIX_FMT_MMAL,
            Self::D3D11VA_VLD => ffi::AV_PIX_FMT_D3D11VA_VLD,
            Self::CUDA => ffi::AV_PIX_FMT_CUDA,
            Self::ZRGB => ffi::AV_PIX_FMT_0RGB,
            Self::RGBZ => ffi::AV_PIX_FMT_RGB0,
            Self::ZBGR => ffi::AV_PIX_FMT_0BGR,
            Self::BGRZ => ffi::AV_PIX_FMT_BGR0,
            Self::YUV420P12BE => ffi::AV_PIX_FMT_YUV420P12BE,
            Self::YUV420P12LE => ffi::AV_PIX_FMT_YUV420P12LE,
            Self::YUV420P14BE => ffi::AV_PIX_FMT_YUV420P14BE,
            Self::YUV420P14LE => ffi::AV_PIX_FMT_YUV420P14LE,
            Self::YUV422P12BE => ffi::AV_PIX_FMT_YUV422P12BE,
            Self::YUV422P12LE => ffi::AV_PIX_FMT_YUV422P12LE,
            Self::YUV422P14BE => ffi::AV_PIX_FMT_YUV422P14BE,
            Self::YUV422P14LE => ffi::AV_PIX_FMT_YUV422P14LE,
            Self::YUV444P12BE => ffi::AV_PIX_FMT_YUV444P12BE,
            Self::YUV444P12LE => ffi::AV_PIX_FMT_YUV444P12LE,
            Self::YUV444P14BE => ffi::AV_PIX_FMT_YUV444P14BE,
            Self::YUV444P14LE => ffi::AV_PIX_FMT_YUV444P14LE,
            Self::GBRP12BE => ffi::AV_PIX_FMT_GBRP12BE,
            Self::GBRP12LE => ffi::AV_PIX_FMT_GBRP12LE,
            Self::GBRP14BE => ffi::AV_PIX_FMT_GBRP14BE,
            Self::GBRP14LE => ffi::AV_PIX_FMT_GBRP14LE,
            Self::YUVJ411P => ffi::AV_PIX_FMT_YUVJ411P,
            Self::BAYER_BGGR8 => ffi::AV_PIX_FMT_BAYER_BGGR8,
            Self::BAYER_RGGB8 => ffi::AV_PIX_FMT_BAYER_RGGB8,
            Self::BAYER_GBRG8 => ffi::AV_PIX_FMT_BAYER_GBRG8,
            Self::BAYER_GRBG8 => ffi::AV_PIX_FMT_BAYER_GRBG8,
            Self::BAYER_BGGR16LE => ffi::AV_PIX_FMT_BAYER_BGGR16LE,
            Self::BAYER_BGGR16BE => ffi::AV_PIX_FMT_BAYER_BGGR16BE,
            Self::BAYER_RGGB16LE => ffi::AV_PIX_FMT_BAYER_RGGB16LE,
            Self::BAYER_RGGB16BE => ffi::AV_PIX_FMT_BAYER_RGGB16BE,
            Self::BAYER_GBRG16LE => ffi::AV_PIX_FMT_BAYER_GBRG16LE,
            Self::BAYER_GBRG16BE => ffi::AV_PIX_FMT_BAYER_GBRG16BE,
            Self::BAYER_GRBG16LE => ffi::AV_PIX_FMT_BAYER_GRBG16LE,
            Self::BAYER_GRBG16BE => ffi::AV_PIX_FMT_BAYER_GRBG16BE,
            Self::YUV440P10LE => ffi::AV_PIX_FMT_YUV440P10LE,
            Self::YUV440P10BE => ffi::AV_PIX_FMT_YUV440P10BE,
            Self::YUV440P12LE => ffi::AV_PIX_FMT_YUV440P12LE,
            Self::YUV440P12BE => ffi::AV_PIX_FMT_YUV440P12BE,
            Self::AYUV64LE => ffi::AV_PIX_FMT_AYUV64LE,
            Self::AYUV64BE => ffi::AV_PIX_FMT_AYUV64BE,
            Self::VIDEOTOOLBOX => ffi::AV_PIX_FMT_VIDEOTOOLBOX,
            Self::P010LE => ffi::AV_PIX_FMT_P010LE,
            Self::P010BE => ffi::AV_PIX_FMT_P010BE,
            Self::GBRAP12BE => ffi::AV_PIX_FMT_GBRAP12BE,
            Self::GBRAP12LE => ffi::AV_PIX_FMT_GBRAP12LE,
            Self::GBRAP10BE => ffi::AV_PIX_FMT_GBRAP10BE,
            Self::GBRAP10LE => ffi::AV_PIX_FMT_GBRAP10LE,
            Self::MEDIACODEC => ffi::AV_PIX_FMT_MEDIACODEC,
            Self::GRAY12BE => ffi::AV_PIX_FMT_GRAY12BE,
            Self::GRAY12LE => ffi::AV_PIX_FMT_GRAY12LE,
            Self::GRAY10BE => ffi::AV_PIX_FMT_GRAY10BE,
            Self::GRAY10LE => ffi::AV_PIX_FMT_GRAY10LE,
            Self::P016LE => ffi::AV_PIX_FMT_P016LE,
            Self::P016BE => ffi::AV_PIX_FMT_P016BE,
            Self::D3D11 => ffi::AV_PIX_FMT_D3D11,
            Self::GRAY9BE => ffi::AV_PIX_FMT_GRAY9BE,
            Self::GRAY9LE => ffi::AV_PIX_FMT_GRAY9LE,
            Self::GBRPF32BE => ffi::AV_PIX_FMT_GBRPF32BE,
            Self::GBRPF32LE => ffi::AV_PIX_FMT_GBRPF32LE,
            Self::GBRAPF32BE => ffi::AV_PIX_FMT_GBRAPF32BE,
            Self::GBRAPF32LE => ffi::AV_PIX_FMT_GBRAPF32LE,
            Self::DRM_PRIME => ffi::AV_PIX_FMT_DRM_PRIME,
            Self::OPENCL => ffi::AV_PIX_FMT_OPENCL,
            Self::GRAY14BE => ffi::AV_PIX_FMT_GRAY14BE,
            Self::GRAY14LE => ffi::AV_PIX_FMT_GRAY14LE,
            Self::GRAYF32BE => ffi::AV_PIX_FMT_GRAYF32BE,
            Self::GRAYF32LE => ffi::AV_PIX_FMT_GRAYF32LE,
            Self::YUVA422P12BE => ffi::AV_PIX_FMT_YUVA422P12BE,
            Self::YUVA422P12LE => ffi::AV_PIX_FMT_YUVA422P12LE,
            Self::YUVA444P12BE => ffi::AV_PIX_FMT_YUVA444P12BE,
            Self::YUVA444P12LE => ffi::AV_PIX_FMT_YUVA444P12LE,
            Self::NV24 => ffi::AV_PIX_FMT_NV24,
            Self::NV42 => ffi::AV_PIX_FMT_NV42,
            Self::VULKAN => ffi::AV_PIX_FMT_VULKAN,
            Self::Y210BE => ffi::AV_PIX_FMT_Y210BE,
            Self::Y210LE => ffi::AV_PIX_FMT_Y210LE,
            Self::X2RGB10LE => ffi::AV_PIX_FMT_X2RGB10LE,
            Self::X2RGB10BE => ffi::AV_PIX_FMT_X2RGB10BE,
            Self::X2BGR10LE => ffi::AV_PIX_FMT_X2BGR10LE,
            Self::X2BGR10BE => ffi::AV_PIX_FMT_X2BGR10BE,
            Self::P210BE => ffi::AV_PIX_FMT_P210BE,
            Self::P210LE => ffi::AV_PIX_FMT_P210LE,
            Self::P410BE => ffi::AV_PIX_FMT_P410BE,
            Self::P410LE => ffi::AV_PIX_FMT_P410LE,
            Self::P216BE => ffi::AV_PIX_FMT_P216BE,
            Self::P216LE => ffi::AV_PIX_FMT_P216LE,
            Self::P416BE => ffi::AV_PIX_FMT_P416BE,
            Self::P416LE => ffi::AV_PIX_FMT_P416LE,
            Self::VUYA => ffi::AV_PIX_FMT_VUYA,
            Self::RGBAF16BE => ffi::AV_PIX_FMT_RGBAF16BE,
            Self::RGBAF16LE => ffi::AV_PIX_FMT_RGBAF16LE,
            Self::VUYX => ffi::AV_PIX_FMT_VUYX,
            Self::P012LE => ffi::AV_PIX_FMT_P012LE,
            Self::P012BE => ffi::AV_PIX_FMT_P012BE,
            Self::Y212BE => ffi::AV_PIX_FMT_Y212BE,
            Self::Y212LE => ffi::AV_PIX_FMT_Y212LE,
            Self::XV30BE => ffi::AV_PIX_FMT_XV30BE,
            Self::XV30LE => ffi::AV_PIX_FMT_XV30LE,
            Self::XV36BE => ffi::AV_PIX_FMT_XV36BE,
            Self::XV36LE => ffi::AV_PIX_FMT_XV36LE,
            Self::RGBF32BE => ffi::AV_PIX_FMT_RGBF32BE,
            Self::RGBF32LE => ffi::AV_PIX_FMT_RGBF32LE,
            Self::RGBAF32BE => ffi::AV_PIX_FMT_RGBAF32BE,
            Self::RGBAF32LE => ffi::AV_PIX_FMT_RGBAF32LE,
            Self::P212BE => ffi::AV_PIX_FMT_P212BE,
            Self::P212LE => ffi::AV_PIX_FMT_P212LE,
            Self::P412BE => ffi::AV_PIX_FMT_P412BE,
            Self::P412LE => ffi::AV_PIX_FMT_P412LE,
            Self::GBRAP14BE => ffi::AV_PIX_FMT_GBRAP14BE,
            Self::GBRAP14LE => ffi::AV_PIX_FMT_GBRAP14LE,
            Self::D3D12 => ffi::AV_PIX_FMT_D3D12,
        }
    }
}
