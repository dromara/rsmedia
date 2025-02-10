use rsmpeg::ffi;
use rsmpeg::avutil::AVPixelFormat;
use rsmpeg::avutil::AVFrame;

/// Re-export internal `AvPixel` as `PixelFormat` for callers.
pub type PixelFormat = AVPixelFormat;

/// Re-export internal `AvFrame` for caller to use.
pub type RawFrame = AVFrame;

/// Re-export frame type as ndarray.
#[cfg(feature = "ndarray")]
pub type Frame = crate::ffi::FrameArray;

/// Default frame pixel format.
pub(crate) const FRAME_PIXEL_FORMAT: AVPixelFormat = ffi::AV_PIX_FMT_RGB24;
