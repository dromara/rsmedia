pub mod decode;
pub mod encode;
pub mod error;
#[cfg(feature = "ndarray")]
pub mod frame;
#[cfg(feature = "ndarray")]
pub use frame::FrameArray;
mod ffi_hwaccel;
mod flags;
pub mod hwaccel;
pub mod init;
pub mod io;
pub mod location;
pub mod options;
pub mod packet;
pub mod rational;
pub mod resize;
pub mod stream;
pub mod time;

pub use decode::{Decoder, DecoderBuilder};
pub use encode::{Encoder, EncoderBuilder};
pub use init::init;
pub use io::{Reader, ReaderBuilder, Writer, WriterBuilder};
pub use location::{Location, Url};
pub use options::Options;
pub use packet::Packet;
pub use rational::Rational;
pub use resize::Resize;
pub use rsmpeg::ffi;
pub use time::Time;

/// Re-export internal `AvFrame` for caller to use.
pub type RawFrame = rsmpeg::avutil::AVFrame;

/// pixel format
pub const PIXEL_FORMAT_RGB24: ffi::AVPixelFormat = ffi::AV_PIX_FMT_RGB24;
pub const PIXEL_FORMAT_YUV420P: ffi::AVPixelFormat = ffi::AV_PIX_FMT_YUV420P;
/// NV12 pixel format with hardware acceleration
pub const PIXEL_FORMAT_NV12: ffi::AVPixelFormat = ffi::AV_PIX_FMT_NV12;
