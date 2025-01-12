#![allow(non_camel_case_types)]
#![allow(clippy::missing_safety_doc)]
#![allow(clippy::module_inception)]
#![allow(clippy::too_many_arguments)]

#[macro_use]
extern crate bitflags;
extern crate libc;

pub extern crate rusty_ffmpeg as sys;
pub use sys::ffi;

#[cfg(feature = "image")]
extern crate image;

#[macro_use]
pub mod util;
pub use util::{
    channel_layout::{self, ChannelLayout},
    chroma, color, dictionary, log,
    dictionary::{Mut as DictionaryMut, Owned as Dictionary, Ref as DictionaryRef},
    error::Error,
    frame::{self, Frame},
    mathematics::{self, rescale, Rescale, Rounding},
    media, option, picture,
    rational::{self, Rational},
    time,
};

// #[cfg(feature = "format")]
pub mod format;
// #[cfg(feature = "format")]
pub use format::chapter::{Chapter, ChapterMut};
// #[cfg(feature = "format")]
pub use format::format::Format;
// #[cfg(feature = "format")]
pub use format::stream::{Stream, StreamMut};

// #[cfg(feature = "codec")]
pub mod codec;
// #[cfg(feature = "codec")]
pub use codec::audio_service::AudioService;
// #[cfg(feature = "codec")]
pub use codec::codec::Codec;
// #[cfg(feature = "codec")]
pub use codec::discard::Discard;
// #[cfg(feature = "codec")]
pub use codec::field_order::FieldOrder;
// #[cfg(feature = "codec")]
pub use codec::packet::{self, Packet};
// #[cfg(all(feature = "codec", not(feature = "ffmpeg5")))]
// pub use codec::picture::Picture;
// #[cfg(feature = "codec")]
pub use codec::subtitle::{self, Subtitle};
// #[cfg(feature = "codec")]
pub use codec::threading;
// #[cfg(feature = "codec")]
pub use codec::{decoder, encoder};

// #[cfg(feature = "device")]
pub mod device;

// #[cfg(feature = "filter")]
pub mod filter;
// #[cfg(feature = "filter")]
pub use filter::Filter;

pub mod software;

fn init_error() {
    util::error::register_all();
}

// #[cfg(all(feature = "format", not(feature = "ffmpeg5")))]
fn init_format() {
    format::register_all();
}

// #[cfg(feature = "device")]
fn init_device() {
    device::register_all();
}

// #[cfg(not(feature = "device"))]
// fn init_device() {}

// #[cfg(all(feature = "filter", not(feature = "ffmpeg5")))]
fn init_filter() {
    filter::register_all();
}

pub fn init() -> Result<(), Error> {
    init_error();
    // #[cfg(not(feature = "ffmpeg5"))]
    init_format();
    init_device();
    // #[cfg(not(feature = "ffmpeg5"))]
    init_filter();

    Ok(())
}
