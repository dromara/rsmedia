#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::module_inception)]
#![allow(clippy::too_many_arguments)]

#[macro_use]
extern crate bitflags;

#[cfg(feature = "image")]
extern crate image;

extern crate libc;

pub extern crate rusty_ffmpeg as sys;
pub use sys::ffi;

#[macro_use]
pub mod util;
pub use util::channel_layout::{self, ChannelLayout};
pub use util::chroma;
pub use util::color;
pub use util::dictionary;
pub use util::dictionary::Mut as DictionaryMut;
pub use util::dictionary::Owned as Dictionary;
pub use util::dictionary::Ref as DictionaryRef;
pub use util::error::{self, Error};
pub use util::frame::{self, Frame};
pub use util::log;
pub use util::mathematics::{self, rescale, Rescale, Rounding};
pub use util::media;
pub use util::option;
pub use util::picture;
pub use util::rational::{self, Rational};
pub use util::time;

pub mod format;
pub use format::chapter::{Chapter, ChapterMut};
pub use format::format::Format;
pub use format::stream::{Stream, StreamMut};

pub mod codec;
pub use codec::audio_service::AudioService;
pub use codec::codec::Codec;
pub use codec::discard::Discard;
pub use codec::field_order::FieldOrder;
pub use codec::packet::{self, Packet};
#[cfg(not(feature = "ffmpeg7"))]
pub use codec::picture::Picture;
pub use codec::subtitle::{self, Subtitle};
pub use codec::threading;
pub use codec::{decoder, encoder};

pub mod device;

pub mod filter;
pub use filter::Filter;

pub mod software;

fn init_error() {
    util::error::register_all();
}

// #[cfg(all(feature = "format", not(feature = "ffmpeg_5_0")))]
fn init_format() {
    format::register_all();
}

// #[cfg(feature = "device")]
fn init_device() {
    device::register_all();
}

// #[cfg(all(feature = "filter", not(feature = "ffmpeg_5_0")))]
fn init_filter() {
    filter::register_all();
}

pub fn init() -> Result<(), Error> {
    init_error();
    // #[cfg(not(feature = "ffmpeg_5_0"))]
    init_format();
    init_device();
    // #[cfg(not(feature = "ffmpeg_5_0"))]
    init_filter();

    Ok(())
}
