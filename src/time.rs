use rsmpeg::avutil;
use rsmpeg::ffi;
use rsmpeg::ffi::AVRational;

use std::time::Duration;

/// Represents a time or duration.
///
/// [`Time`] may represent a PTS (presentation timestamp), DTS (decoder timestamp) or a duration,
/// depending on the function that returns it.
///
/// [`Time`] may represent a non-existing time, in which case [`Time::has_value`] will return
/// `false`, and conversions to seconds will return `0.0`.
///
/// A [`Time`] object may be aligned with another [`Time`] object, which produces an [`Aligned`]
/// object, on which arithmetic operations can be performed.
#[derive(Debug, Copy, Clone)]
pub struct Time {
    pub time: Option<i64>,
    pub time_base: AVRational,
}

impl Time {
    /// Create a new time by its time value and time base in which the time is expressed.
    ///
    /// # Arguments
    ///
    /// * `time` - Relative time in `time_base` units.
    /// * `time_base` - Time base of source.
    pub fn new(time: Option<i64>, time_base: AVRational) -> Time {
        Self { time, time_base }
    }

    /// Creates a new timestamp that reprsents `nth` of a second.
    ///
    /// # Arguments
    ///
    /// * `nth` - Denominator of the time in seconds as in `1 / nth`.
    pub fn from_nth_of_a_second(nth: usize) -> Self {
        Self {
            time: Some(1),
            time_base: new_rational(1, nth as i32),
        }
    }

    /// Creates a new timestamp from a number of seconds.
    ///
    /// # Arguments
    ///
    /// * `secs` - Number of seconds.
    pub fn from_secs(secs: f32) -> Self {
        Self {
            time: Some((secs * TIME_BASE.den as f32).round() as i64),
            time_base: TIME_BASE,
        }
    }

    /// Creates a new timestamp from a number of seconds.
    ///
    /// # Arguments
    ///
    /// * `secs` - Number of seconds.
    pub fn from_secs_f64(secs: f64) -> Self {
        Self {
            time: Some((secs * TIME_BASE.den as f64).round() as i64),
            time_base: TIME_BASE,
        }
    }

    /// Creates a new timestamp with `time` time units, each represents one / `base_den` seconds.
    ///
    /// # Arguments
    ///
    /// * `time` - Relative time in `time_base` units.
    /// * `base_den` - Time base denominator i.e. time base is `1 / base_den`.
    pub fn from_units(time: usize, base_den: usize) -> Self {
        Self {
            time: Some(time as i64),
            time_base: new_rational(1, base_den as i32),
        }
    }

    /// Create a new zero-valued timestamp.
    ///
    /// 时间基取 [`TIME_BASE`]，因此与 `Time::new(Some(0), TIME_BASE)`（以及
    /// `Time::from_secs(0.0)`）完全相等 —— 早先这里硬编码 `1/90000`，会出现
    /// "同为 0 秒却不相等" 的荒谬结果。
    pub fn zero() -> Self {
        Time {
            time: Some(0),
            time_base: TIME_BASE,
        }
    }

    /// Whether the [`Time`] carries a usable value at all.
    ///
    /// Both "no time" spellings report `false`: no value whatsoever (`None`), and
    /// the `AV_NOPTS_VALUE` sentinel FFmpeg writes when a stream simply has no
    /// timestamp. This is the predicate to branch on before converting to seconds
    /// — converting a NOPTS would yield ≈ -9.2e13 seconds.
    pub fn has_value(&self) -> bool {
        self.time.is_some_and(|time| time != ffi::AV_NOPTS_VALUE)
    }

    /// Align the timestamp with another timestamp, which will convert the `rhs` timestamp to the
    /// same time base, such that operations can be performed upon the aligned timestamps.
    ///
    /// # Arguments
    ///
    /// * `rhs` - Right-hand side timestamp.
    ///
    /// # Return value
    ///
    /// Two timestamps that are aligned.
    pub fn aligned_with(&self, rhs: Time) -> Aligned {
        Aligned {
            lhs: self.time,
            rhs: rhs
                .time
                .map(|rhs_time| rhs_time.rescale(rhs.time_base, self.time_base)),
            time_base: self.time_base,
        }
    }

    /// Get number of seconds as floating point value.
    pub fn as_secs(&self) -> f32 {
        // time_base 无效（num/den 为 0）时回退为 0.0，避免产生 NaN，
        // 否则下游 Duration::from_secs_f64(NaN) 等会直接 panic
        if self.time_base.num == 0 || self.time_base.den == 0 {
            return 0.0;
        }
        if let Some(time) = self.time {
            (time as f32) * (self.time_base.num as f32 / self.time_base.den as f32)
        } else {
            0.0
        }
    }

    /// Get number of seconds as floating point value.
    pub fn as_secs_f64(&self) -> f64 {
        if self.time_base.num == 0 || self.time_base.den == 0 {
            return 0.0;
        }
        if let Some(time) = self.time {
            (time as f64) * (self.time_base.num as f64 / self.time_base.den as f64)
        } else {
            0.0
        }
    }

    /// Convert to underlying time to `i64` (the number of time units).
    ///
    /// Assumes that the caller knows the time base and applies it correctly when doing arithmetic
    /// operations on the time value.
    pub fn into_value(self) -> Option<i64> {
        self.time
    }

    /// Align the timestamp along another `time_base`.
    ///
    /// # Arguments
    ///
    /// * `time_base` - Target time base.
    pub fn aligned_with_rational(&self, time_base: AVRational) -> Time {
        Time {
            time: self
                .time
                .map(|time| time.rescale(self.time_base, time_base)),
            time_base,
        }
    }
}

/////////////////////////////////
/////////////////////////////////

pub const TIME_BASE: AVRational = avutil::ra(ffi::AV_TIME_BASE_Q.num, ffi::AV_TIME_BASE_Q.den);

pub trait Rescale {
    fn rescale<S, D>(&self, source: S, destination: D) -> i64
    where
        S: Into<AVRational>,
        D: Into<AVRational>;

    fn rescale_with<S, D>(&self, source: S, destination: D, rounding: ffi::AVRounding) -> i64
    where
        S: Into<AVRational>,
        D: Into<AVRational>;
}

impl<T: Into<i64> + Clone> Rescale for T {
    fn rescale<S, D>(&self, source: S, destination: D) -> i64
    where
        S: Into<AVRational>,
        D: Into<AVRational>,
    {
        avutil::av_rescale_q(self.clone().into(), source.into(), destination.into())
    }

    fn rescale_with<S, D>(&self, source: S, destination: D, rounding: ffi::AVRounding) -> i64
    where
        S: Into<AVRational>,
        D: Into<AVRational>,
    {
        avutil::av_rescale_q_rnd(
            self.clone().into(),
            source.into(),
            destination.into(),
            rounding as _,
        )
    }
}

#[inline(always)]
pub fn new_rational(num: i32, den: i32) -> AVRational {
    avutil::ra(num, den)
}

#[inline(always)]
pub fn av_rational_eq(a: &AVRational, b: &AVRational) -> bool {
    a.num == b.num && a.den == b.den
}

impl PartialEq for Time {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time
            && self.time_base.num == other.time_base.num
            && self.time_base.den == other.time_base.den
    }
}

impl Eq for Time {}

impl PartialOrd for Time {
    /// 与 [`PartialEq`] 的契约保持一致：只有**可比且相等**时才返回 `Equal`。
    ///
    /// 两个时间基不同的值不可比较（返回 `None`），这在两个都有值时已经成立；
    /// 两个都无值时同样如此，否则会出现 `a != b` 却 `partial_cmp(a, b) == Equal`
    /// 的矛盾（`PartialEq` 把时间基也算作相等的一部分）。
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // 时间基不同即不可比较，与有无值无关。
        if !av_rational_eq(&self.time_base, &other.time_base) {
            return None;
        }

        match (self.time, other.time) {
            // "无值" 排在 "有值" 之前，两者不会判等。
            (None, None) => Some(std::cmp::Ordering::Equal),
            (None, Some(_)) => Some(std::cmp::Ordering::Less),
            (Some(_), None) => Some(std::cmp::Ordering::Greater),
            (Some(t1), Some(t2)) => t1.partial_cmp(&t2),
        }
    }
}

impl From<Duration> for Time {
    /// Convert from a [`Duration`] to [`Time`].
    #[inline]
    fn from(duration: Duration) -> Self {
        Time::from_secs_f64(duration.as_secs_f64())
    }
}

impl From<Time> for Duration {
    /// Convert from a [`Time`] to a Rust-native [`Duration`].
    fn from(timestamp: Time) -> Self {
        Duration::from_secs_f64(timestamp.as_secs_f64().max(0.0))
    }
}

impl std::fmt::Display for Time {
    /// Format [`Time`] as follows:
    ///
    /// * If the inner value is not `None`: `time/time_base`.
    /// * If the inner value is `None`: `none`.
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(time) = self.time {
            let num = self.time_base.num as i64 * time;
            let den = self.time_base.den;
            write!(f, "{num}/{den} secs")
        } else {
            write!(f, "none")
        }
    }
}

/// This is a virtual object that represents two aligned times.
///
/// On this object, arthmetic operations can be performed that operate on the two contained times.
/// This virtual object ensures that the interface to these operations is safe.
#[derive(Debug, Clone)]
pub struct Aligned {
    lhs: Option<i64>,
    rhs: Option<i64>,
    time_base: AVRational,
}

impl Aligned {
    /// Add two timestamps together.
    pub fn add(self) -> Time {
        self.apply(|lhs, rhs| lhs + rhs)
    }

    /// Subtract the right-hand side timestamp from the left-hand side timestamp.
    pub fn subtract(self) -> Time {
        self.apply(|lhs, rhs| lhs - rhs)
    }

    /// Apply operation `f` on aligned timestamps.
    ///
    /// The closure operates on the numerator of two aligned times.
    ///
    /// # Arguments
    ///
    /// * `f` - Function to apply on the two aligned time numerator values.
    fn apply<F>(self, f: F) -> Time
    where
        F: FnOnce(i64, i64) -> i64,
    {
        match (self.lhs, self.rhs) {
            (Some(lhs_time), Some(rhs_time)) => Time {
                time: Some(f(lhs_time, rhs_time)),
                time_base: self.time_base,
            },
            _ => Time {
                time: None,
                time_base: self.time_base,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let time = Time::new(Some(2), new_rational(3, 9));
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 2.0 / 3.0);
        assert_eq!(time.into_value(), Some(2));
    }

    #[test]
    fn test_aligned_with_rational() {
        let time = Time::new(Some(2), new_rational(3, 9));
        assert_eq!(time.as_secs(), 2.0 / 3.0);
        let time = time.aligned_with_rational(new_rational(1, 9));
        assert_eq!(time.as_secs(), 2.0 / 3.0);
        assert_eq!(time.into_value(), Some(6));
    }

    #[test]
    fn test_from_nth_of_a_second() {
        let time = Time::from_nth_of_a_second(4);
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 0.25);
        assert_eq!(time.as_secs_f64(), 0.25);
        assert_eq!(Duration::from(time), Duration::from_millis(250));
    }

    #[test]
    fn test_from_secs() {
        let time = Time::from_secs(2.5);
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 2.5);
        assert_eq!(time.as_secs_f64(), 2.5);
        assert_eq!(Duration::from(time), Duration::from_millis(2500));
    }

    #[test]
    fn test_from_secs_f64() {
        let time = Time::from_secs(4.0);
        assert!(time.has_value());
        assert_eq!(time.as_secs_f64(), 4.0);
    }

    #[test]
    fn test_from_units() {
        let time = Time::from_units(3, 5);
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 3.0 / 5.0);
        assert_eq!(Duration::from(time), Duration::from_millis(600));
    }

    #[test]
    fn test_zero() {
        let time = Time::zero();
        assert!(time.has_value());
        assert_eq!(time.as_secs(), 0.0);
        assert_eq!(time.as_secs_f64(), 0.0);
        assert_eq!(Duration::from(time), Duration::ZERO);
        let time = Time::zero();
        assert_eq!(time.into_value(), Some(0));
    }

    /// `Time::zero()` 与同样表示 0 秒的值完全相等（含时间基）。
    #[test]
    fn test_zero_equals_other_zero_values() {
        assert_eq!(Time::zero(), Time::new(Some(0), TIME_BASE));
        assert_eq!(Time::zero(), Time::from_secs(0.0));
        assert_eq!(Time::zero(), Time::from_secs_f64(0.0));
    }

    /// "无值" 与 `AV_NOPTS_VALUE` 都表示"没有可用时间戳"，`has_value` 一律为 false。
    #[test]
    fn test_has_value_rejects_missing_and_nopts() {
        let missing = Time::new(None, TIME_BASE);
        let nopts = Time::new(Some(ffi::AV_NOPTS_VALUE), TIME_BASE);

        assert!(!missing.has_value());
        assert!(!nopts.has_value());
        assert!(!missing.has_value());
        assert!(!nopts.has_value());

        assert!(Time::new(Some(0), TIME_BASE).has_value());
    }

    /// `partial_cmp` 与 `PartialEq` 保持一致：时间基不同即不可比较，且不会报 `Equal`。
    #[test]
    fn test_partial_cmp_is_consistent_with_eq() {
        let a = Time::new(None, new_rational(1, 2));
        let b = Time::new(None, new_rational(1, 4));
        assert_ne!(a, b);
        assert_eq!(
            a.partial_cmp(&b),
            None,
            "different time bases must be unordered"
        );

        let same = Time::new(None, new_rational(1, 2));
        assert_eq!(a, same);
        assert_eq!(a.partial_cmp(&same), Some(std::cmp::Ordering::Equal));

        let later = Time::new(Some(3), new_rational(1, 2));
        assert_eq!(a.partial_cmp(&later), Some(std::cmp::Ordering::Less));
        assert_eq!(later.partial_cmp(&a), Some(std::cmp::Ordering::Greater));
    }

    #[test]
    fn test_aligned_with() {
        let a = Time::from_units(3, 16);
        let b = Time::from_units(1, 8);
        let aligned = a.aligned_with(b);
        assert_eq!(aligned.lhs, Some(3));
        assert_eq!(aligned.rhs, Some(2));
    }

    #[test]
    fn test_into_aligned_with() {
        let a = Time::from_units(2, 7);
        let b = Time::from_units(2, 3);
        let aligned = a.aligned_with(b);
        assert_eq!(aligned.lhs, Some(2));
        assert_eq!(aligned.rhs, Some(5));
    }

    #[test]
    fn test_as_secs() {
        let time = Time::from_nth_of_a_second(4);
        assert_eq!(time.as_secs(), 0.25);
        let time = Time::from_secs(0.3);
        assert_eq!(time.as_secs(), 0.3);
        let time = Time::new(None, new_rational(0, 0));
        assert_eq!(time.as_secs(), 0.0);
    }

    #[test]
    fn test_as_secs_f64() {
        let time = Time::from_nth_of_a_second(4);
        assert_eq!(time.as_secs_f64(), 0.25);
        let time = Time::from_secs_f64(0.3);
        assert_eq!(time.as_secs_f64(), 0.3);
        let time = Time::new(None, new_rational(0, 0));
        assert_eq!(time.as_secs_f64(), 0.0);
    }

    #[test]
    fn test_into_value_none() {
        let time = Time::new(None, new_rational(0, 0));
        assert_eq!(time.into_value(), None);
    }

    #[test]
    fn test_add() {
        let a = Time::from_secs(0.2);
        let b = Time::from_secs(0.3);
        assert_eq!(a.aligned_with(b).add(), Time::from_secs(0.5));
    }

    #[test]
    fn test_subtract() {
        let a = Time::from_secs(0.8);
        let b = Time::from_secs(0.4);
        assert_eq!(a.aligned_with(b).subtract(), Time::from_secs(0.4));
    }

    #[test]
    fn test_apply() {
        let a = Time::from_secs(2.0);
        let b = Time::from_secs(0.25);
        assert_eq!(
            a.aligned_with(b).apply(|x, y| (2 * x) + (3 * y)),
            Time::from_secs(4.75)
        );
    }

    #[test]
    fn test_apply_different_time_bases() {
        let a = Time::new(Some(3), new_rational(2, 32));
        let b = Time::from_nth_of_a_second(4);
        assert!(
            (a.aligned_with(b).apply(|x, y| x + y).as_secs()
                - Time::from_secs(7.0 / 16.0).as_secs())
            .abs()
                < 0.001
        );
    }

    #[test]
    fn test_negative_into_duration_clamps() {
        assert_eq!(
            Duration::from(Time::new(Some(-100), new_rational(0, 0))),
            Duration::ZERO,
        )
    }

    #[test]
    fn test_av_no_pts_value() {
        let nopts = Time::new(Some(ffi::AV_NOPTS_VALUE), new_rational(0, 0));
        assert_eq!(nopts.into_value(), Some(ffi::AV_NOPTS_VALUE));
        assert_eq!(Duration::from(nopts).as_secs_f32(), 0.0);
    }
}
