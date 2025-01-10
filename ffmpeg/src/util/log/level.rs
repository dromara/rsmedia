use std::convert::TryFrom;

use libc::c_int;
use sys::ffi;

#[derive(Eq, PartialEq, Clone, Copy, Debug)]
pub enum Level {
    Quiet,
    Panic,
    Fatal,
    Error,
    Warning,
    Info,
    Verbose,
    Debug,
    Trace,
}

pub struct LevelError;

impl TryFrom<c_int> for Level {
    type Error = &'static str;

    fn try_from(value: c_int) -> Result<Self, &'static str> {
        match value {
            x if x == ffi::AV_LOG_QUIET => Ok(Level::Quiet),
            x if x == ffi::AV_LOG_PANIC as i32 => Ok(Level::Panic),
            x if x == ffi::AV_LOG_FATAL as i32 => Ok(Level::Fatal),
            x if x == ffi::AV_LOG_ERROR as i32 => Ok(Level::Error),
            x if x == ffi::AV_LOG_WARNING as i32 => Ok(Level::Warning),
            x if x == ffi::AV_LOG_INFO as i32 => Ok(Level::Info),
            x if x == ffi::AV_LOG_VERBOSE as i32 => Ok(Level::Verbose),
            x if x == ffi::AV_LOG_DEBUG as i32 => Ok(Level::Debug),
            x if x == ffi::AV_LOG_TRACE as i32 => Ok(Level::Trace),
            _ => Err("illegal log level"),
        }
    }
}

impl From<Level> for c_int {
    fn from(value: Level) -> c_int {
        match value {
            Level::Quiet => ffi::AV_LOG_QUIET,
            Level::Panic => ffi::AV_LOG_PANIC as i32,
            Level::Fatal => ffi::AV_LOG_FATAL as i32,
            Level::Error => ffi::AV_LOG_ERROR as i32,
            Level::Warning => ffi::AV_LOG_WARNING as i32,
            Level::Info => ffi::AV_LOG_INFO as i32,
            Level::Verbose => ffi::AV_LOG_VERBOSE as i32,
            Level::Debug => ffi::AV_LOG_DEBUG as i32,
            Level::Trace => ffi::AV_LOG_TRACE as i32,
        }
    }
}
