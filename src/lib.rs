#[macro_use]
mod macros;

/// 排空循环的迭代上限，供解码器、编码器与滤镜图三处的 EOF 排空共用。
///
/// 这些循环都在等 FFmpeg 报"结束"，而个别编解码器/滤镜图在收到 EOS 后可能一直
/// 回 EAGAIN 而不报 EOF；没有上限就是挂死（`Drop` 里更不能卡住），故统一收尾。
pub(crate) const MAX_DRAIN_ITERATIONS: usize = 1_000;

pub mod decode;
pub mod encode;
#[cfg(feature = "ndarray")]
pub mod frame;
#[cfg(feature = "ndarray")]
pub use frame::{ElementType, FrameData, FrameSideData, MediaFrame};
pub mod bsf;
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

pub use bsf::Bsf;
pub use codec::{CodecConfig, FormatInfo, Profile};
pub use colors::Color;
pub use decode::{Decoder, DecoderBuilder, thumbnail};
pub use encode::{Encoder, EncoderBuilder};
pub use error::{Result, RsmediaError};
pub use filter::Filter;
pub use fmt::{DataLayout, FrameFormat, SampleFormat};
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
