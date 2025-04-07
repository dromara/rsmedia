pub mod decode;
pub mod encode;
#[cfg(feature = "ndarray")]
pub mod frame;
#[cfg(feature = "ndarray")]
pub use frame::MediaFrame;
pub mod codec;
pub mod colors;
pub mod filter;
pub mod flags;
pub mod hwaccel;
pub mod imgutils;
pub mod init;
pub mod io;
pub mod location;
pub mod mux;
pub mod options;
pub mod pixel;
pub mod resize;
pub mod stream;
pub mod swctx;
pub mod time;
pub mod utils;

pub use decode::{Decoder, DecoderBuilder};
pub use encode::{Encoder, EncoderBuilder};
pub use flags::{MediaType, SampleFormat};
pub use init::init;
pub use io::{Reader, Writer};
pub use io::{StreamReader, StreamReaderBuilder, StreamWriter, StreamWriterBuilder};
pub use location::{Location, Url};
pub use options::Options;
pub use pixel::PixelFormat;
pub use resize::Resize;
pub use time::Time;

/// Re-export internal `AVRational` for caller to use.
pub use rsmpeg::ffi::AVRational;

/// Re-export internal `AvFrame` for caller to use.
pub type RawFrame = rsmpeg::avutil::AVFrame;
