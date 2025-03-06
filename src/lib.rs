pub mod decode;
pub mod encode;
#[cfg(feature = "ndarray")]
pub mod frame;
#[cfg(feature = "ndarray")]
pub use frame::FrameArray;
pub mod colors;
mod flags;
pub mod hwaccel;
pub mod init;
pub mod io;
pub mod location;
pub mod mux;
pub mod options;
pub mod packet;
pub mod pixel;
pub mod rational;
pub mod resize;
pub mod stream;
pub mod time;
pub mod utils;

pub use decode::{Decoder, DecoderBuilder};
pub use encode::{Encoder, EncoderBuilder};
pub use init::init;
pub use io::{Reader, ReaderBuilder, StreamWriter, StreamWriterBuilder};
pub use location::{Location, Url};
pub use options::Options;
pub use packet::Packet;
pub use pixel::PixelFormat;
pub use rational::Rational;
pub use resize::Resize;
pub use time::Time;

/// Re-export internal `AvFrame` for caller to use.
pub type RawFrame = rsmpeg::avutil::AVFrame;
