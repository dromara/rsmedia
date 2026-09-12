//! Unified error type for the rsmedia library.
//!
//! All public APIs return [`Result`] with [`RsmediaError`], so downstream code
//! can match on error variants instead of string-matching `anyhow` messages.
//!
//! ```no_run
//! use rsmedia::{RsmediaError, Result};
//!
//! fn handle(res: Result<()>) {
//!     match res {
//!         Err(RsmediaError::CodecNotFound(name)) => eprintln!("codec unavailable: {name}"),
//!         Err(e) => eprintln!("error: {e}"),
//!         Ok(()) => {}
//!     }
//! }
//! ```

use std::error::Error as StdError;
use std::fmt;

use rsmpeg::error::RsmpegError;

/// Unified error type for all rsmedia APIs.
#[derive(Debug)]
#[non_exhaustive]
pub enum RsmediaError {
    /// Underlying FFmpeg (rsmpeg) error.
    Ffmpeg(RsmpegError),
    /// I/O error, typically from custom AVIO callbacks or file access.
    Io(std::io::Error),
    /// The requested codec/encoder/decoder does not exist in this FFmpeg build.
    CodecNotFound(String),
    /// The requested container format does not exist in this FFmpeg build.
    FormatNotFound(String),
    /// The operation is not supported on this platform, build or codec.
    Unsupported(String),
    /// Invalid or contradictory configuration.
    InvalidConfig(String),
    /// Any other error with a human readable message.
    Other(String),
    /// An error with additional context attached (produced by [`Context`]).
    Context {
        context: String,
        source: Box<RsmediaError>,
    },
}

impl RsmediaError {
    /// Attach a context message to this error, preserving the source chain.
    pub fn with_context(mut self, context: impl Into<String>) -> Self {
        // Nested contexts collapse into a single message chain: keep the
        // innermost error and join the context strings outermost-first.
        if let RsmediaError::Context {
            context: inner,
            source,
        } = self
        {
            self = *source;
            self.with_context(format!("{}: {inner}", context.into()))
        } else {
            RsmediaError::Context {
                context: context.into(),
                source: Box::new(self),
            }
        }
    }

    /// Build an [`RsmediaError::Other`] from any displayable value.
    pub fn custom(message: impl Into<String>) -> Self {
        RsmediaError::Other(message.into())
    }

    /// Build an [`RsmediaError::CodecNotFound`].
    pub fn codec_not_found(name: impl Into<String>) -> Self {
        RsmediaError::CodecNotFound(name.into())
    }

    /// Build an [`RsmediaError::FormatNotFound`].
    pub fn format_not_found(name: impl Into<String>) -> Self {
        RsmediaError::FormatNotFound(name.into())
    }

    /// Build an [`RsmediaError::Unsupported`].
    pub fn unsupported(reason: impl Into<String>) -> Self {
        RsmediaError::Unsupported(reason.into())
    }

    /// Build an [`RsmediaError::InvalidConfig`].
    pub fn invalid_config(reason: impl Into<String>) -> Self {
        RsmediaError::InvalidConfig(reason.into())
    }
}

impl fmt::Display for RsmediaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RsmediaError::Ffmpeg(e) => write!(f, "FFmpeg error: {e}"),
            RsmediaError::Io(e) => write!(f, "I/O error: {e}"),
            RsmediaError::CodecNotFound(name) => {
                write!(f, "codec not found in this FFmpeg build: '{name}'")
            }
            RsmediaError::FormatNotFound(name) => {
                write!(f, "format not found in this FFmpeg build: '{name}'")
            }
            RsmediaError::Unsupported(reason) => write!(f, "unsupported operation: {reason}"),
            RsmediaError::InvalidConfig(reason) => write!(f, "invalid configuration: {reason}"),
            RsmediaError::Other(message) => write!(f, "{message}"),
            RsmediaError::Context { context, source } => write!(f, "{context}: {source}"),
        }
    }
}

impl StdError for RsmediaError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            RsmediaError::Ffmpeg(e) => Some(e),
            RsmediaError::Io(e) => Some(e),
            RsmediaError::Context { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

impl From<RsmpegError> for RsmediaError {
    fn from(e: RsmpegError) -> Self {
        RsmediaError::Ffmpeg(e)
    }
}

impl From<std::io::Error> for RsmediaError {
    fn from(e: std::io::Error) -> Self {
        RsmediaError::Io(e)
    }
}

impl From<std::ffi::NulError> for RsmediaError {
    fn from(e: std::ffi::NulError) -> Self {
        RsmediaError::InvalidConfig(format!(
            "string passed to FFmpeg contains interior NUL byte: {e}"
        ))
    }
}

impl From<std::str::Utf8Error> for RsmediaError {
    fn from(e: std::str::Utf8Error) -> Self {
        RsmediaError::Other(format!("FFmpeg returned a non-UTF8 string: {e}"))
    }
}

impl From<image::ImageError> for RsmediaError {
    fn from(e: image::ImageError) -> Self {
        RsmediaError::Other(format!("image processing error: {e}"))
    }
}

/// Library-level `Result` alias used by all public APIs.
pub type Result<T> = std::result::Result<T, RsmediaError>;

/// Alias so call sites written as `Result<T, Error>` keep working, and so
/// downstream users can `use rsmedia::Error`.
pub type Error = RsmediaError;

/// Internal convenience macro: build an [`RsmediaError::Other`] with
/// `format!`-style arguments (migrates `anyhow!` call sites).
macro_rules! format_err {
    ($($arg:tt)*) => {
        $crate::error::RsmediaError::Other(format!($($arg)*))
    };
}
pub(crate) use format_err;

/// A drop-in replacement for `anyhow::Context`, implemented for `Option` and
/// `Result` whose error converts into [`RsmediaError`].
pub trait Context<T> {
    /// Attach a context message to the error (or to a missing `Option` value).
    fn context<C: Into<String>>(self, context: C) -> Result<T>;

    /// Lazily attach a context message to the error (or to a missing value).
    fn with_context<C: Into<String>, F: FnOnce() -> C>(self, f: F) -> Result<T>;
}

impl<T> Context<T> for Option<T> {
    fn context<C: Into<String>>(self, context: C) -> Result<T> {
        self.ok_or_else(|| RsmediaError::Other(context.into()))
    }

    fn with_context<C: Into<String>, F: FnOnce() -> C>(self, f: F) -> Result<T> {
        self.ok_or_else(|| RsmediaError::Other(f().into()))
    }
}

impl<T, E> Context<T> for std::result::Result<T, E>
where
    E: Into<RsmediaError>,
{
    fn context<C: Into<String>>(self, context: C) -> Result<T> {
        self.map_err(|e| e.into().with_context(context))
    }

    fn with_context<C: Into<String>, F: FnOnce() -> C>(self, f: F) -> Result<T> {
        self.map_err(|e| e.into().with_context(f()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_variants() {
        assert_eq!(
            RsmediaError::codec_not_found("libx265").to_string(),
            "codec not found in this FFmpeg build: 'libx265'"
        );
        assert_eq!(
            RsmediaError::format_not_found("mkv").to_string(),
            "format not found in this FFmpeg build: 'mkv'"
        );
        assert_eq!(
            RsmediaError::unsupported("qsv on this platform").to_string(),
            "unsupported operation: qsv on this platform"
        );
    }

    #[test]
    fn test_context_chain() {
        let err: std::result::Result<(), RsmpegError> = Err(RsmpegError::DecoderDrainError);
        let wrapped = err.context("Failed to drain decoder").unwrap_err();
        assert_eq!(
            wrapped.to_string(),
            "Failed to drain decoder: FFmpeg error: Decoder have no frame currently, Try send new input."
        );
        // source() must walk down to the rsmpeg error.
        let src = wrapped.source().expect("context has a source");
        assert_eq!(
            src.to_string(),
            "FFmpeg error: Decoder have no frame currently, Try send new input."
        );
    }

    #[test]
    fn test_context_option() {
        let missing: Option<u8> = None;
        assert_eq!(
            missing.context("no device found").unwrap_err().to_string(),
            "no device found"
        );
    }

    #[test]
    fn test_nested_context_joins() {
        let err = RsmediaError::custom("inner failure");
        let wrapped = err.with_context("outer").with_context("outermost");
        assert_eq!(wrapped.to_string(), "outermost: outer: inner failure");
    }
}
