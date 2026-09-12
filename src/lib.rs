#[macro_use]
mod macros;

pub mod decode;
pub mod encode;
#[cfg(feature = "ndarray")]
pub mod frame;
#[cfg(feature = "ndarray")]
pub use frame::{FrameSideData, MediaFrame, MediaFrameType};
pub mod codec;
pub mod colors;
pub mod error;
pub mod filter;
pub mod fmt;
pub mod hwaccel;
pub mod imgutils;
pub mod init;
pub mod io;
pub mod location;
pub mod mux;
pub mod options;
pub mod pcm;
pub mod pixel;
pub mod resample;
pub mod resize;
pub mod scale;
pub mod stream;
pub mod strutils;
pub mod subtitle;
pub mod time;

pub use codec::{CodecConfig, FormatInfo, Profile};
pub use colors::Color;
pub use decode::{Decoder, DecoderBuilder, thumbnail};
pub use encode::{Encoder, EncoderBuilder};
pub use error::{Error, Result, RsmediaError};
pub use filter::Filter;
pub use fmt::{FrameFormat, SampleFormat};
pub use hwaccel::{HWDeviceConfig, HWDeviceType};
pub use init::init;
pub use io::{AVSeekFlag, Reader, Seekable, Writer};
pub use io::{StreamReader, StreamReaderBuilder, StreamWriter, StreamWriterBuilder};
pub use location::{Location, Url};
pub use mux::{Chapter, Demuxer, Muxer};
pub use options::{Metadata, Options, Quality, VideoProfile};
pub use pcm::{PcmSink, PcmSpec};
pub use pixel::PixelFormat;
pub use resample::Resampler;
pub use resize::Resize;
pub use scale::{ScaleAlgorithm, ScaleQuality, Scaler};
pub use stream::MediaType;
pub use subtitle::SubtitleSegment;
pub use time::Time;

pub use rsmpeg::avutil;

/// Unit-test helpers
#[cfg(test)]
pub mod test_support;
