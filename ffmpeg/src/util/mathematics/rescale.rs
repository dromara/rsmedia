use crate::{Rational, Rounding};
use rsmpeg::ffi;

pub const TIME_BASE: Rational = Rational(ffi::AV_TIME_BASE_Q.num, ffi::AV_TIME_BASE_Q.den);

pub trait Rescale {
    fn rescale<S, D>(&self, source: S, destination: D) -> i64
    where
        S: Into<Rational>,
        D: Into<Rational>;

    fn rescale_with<S, D>(&self, source: S, destination: D, rounding: Rounding) -> i64
    where
        S: Into<Rational>,
        D: Into<Rational>;
}

impl<T: Into<i64> + Clone> Rescale for T {
    fn rescale<S, D>(&self, source: S, destination: D) -> i64
    where
        S: Into<Rational>,
        D: Into<Rational>,
    {
        unsafe {
            ffi::av_rescale_q(
                self.clone().into(),
                source.into().into(),
                destination.into().into(),
            )
        }
    }

    fn rescale_with<S, D>(&self, source: S, destination: D, rounding: Rounding) -> i64
    where
        S: Into<Rational>,
        D: Into<Rational>,
    {
        unsafe {
            ffi::av_rescale_q_rnd(
                self.clone().into(),
                source.into().into(),
                destination.into().into(),
                rounding.into(),
            )
        }
    }
}
