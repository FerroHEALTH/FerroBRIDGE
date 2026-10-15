// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! The magnitude of a unit (UCUM §20.2): a decimal mantissa and a power of
//! ten, so the constants the essence defines from `1e-34` to `1e24` keep
//! their digits where a plain `Decimal` would lose them to its scale limit.

use std::cmp::Ordering;
use std::str::FromStr;

use rust_decimal::{Decimal, RoundingStrategy};

/// The largest scale a `Decimal` carries.
const MAX_SCALE: u32 = 28;

/// A value `mantissa × 10^exponent` whose mantissa is zero or has exactly
/// one digit before the decimal point.
// NOTE: no specification governs this: our own design; the mantissa keeps 28 decimal places, so
// a longer essence constant (`[pi]`) or a product too long rounds there, half to even.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Magnitude {
    mantissa: Decimal,
    exponent: i32,
}

impl Magnitude {
    /// The magnitude one.
    pub(crate) const ONE: Self = Self {
        mantissa: Decimal::ONE,
        exponent: 0,
    };

    /// The magnitude of `value × 10^exponent`, `None` when the exponent
    /// leaves the `i32` range.
    pub(crate) fn new(value: Decimal, exponent: i32) -> Option<Self> {
        if value.is_zero() {
            return Some(Self {
                mantissa: Decimal::ZERO,
                exponent: 0,
            });
        }
        let value = value.normalize();
        let digits = digit_count(value.mantissa().unsigned_abs());
        // `digits` is at most 29, the width of a 96-bit mantissa, so both
        // casts are exact.
        let shift = i32::try_from(digits).ok()? - 1 - i32::try_from(value.scale()).ok()?;
        let scale = digits.checked_sub(1)?;
        let mantissa = Decimal::try_from_i128_with_scale(value.mantissa(), scale).ok()?;
        Some(Self {
            mantissa,
            exponent: exponent.checked_add(shift)?,
        })
    }

    /// Reads a decimal the essence writes, `64.79891` or `6.62607015e-34`.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let (mantissa, exponent) = match text.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => (mantissa, exponent.parse::<i32>().ok()?),
            None => (text, 0),
        };
        if mantissa.is_empty() || !mantissa.bytes().all(|b| b.is_ascii_digit() || b == b'.') {
            return None;
        }
        // `from_str` rounds a mantissa longer than 28 digits, half to even.
        Self::new(Decimal::from_str(mantissa).ok()?, exponent)
    }

    /// The magnitude of a decimal.
    pub(crate) fn from_decimal(value: Decimal) -> Option<Self> {
        Self::new(value, 0)
    }

    /// The product, `None` on overflow.
    pub(crate) fn mul(self, other: Self) -> Option<Self> {
        let mantissa = self.mantissa.checked_mul(other.mantissa)?;
        Self::new(mantissa, self.exponent.checked_add(other.exponent)?)
    }

    /// The quotient, `None` on a zero divisor or overflow.
    pub(crate) fn div(self, other: Self) -> Option<Self> {
        let mantissa = self.mantissa.checked_div(other.mantissa)?;
        Self::new(mantissa, self.exponent.checked_sub(other.exponent)?)
    }

    /// The integer power, `None` on a zero base with a negative power or on
    /// overflow.
    pub(crate) fn pow(self, power: i32) -> Option<Self> {
        let mut result = Self::ONE;
        let mut base = self;
        let mut rest = power.unsigned_abs();
        while rest > 0 {
            if rest & 1 == 1 {
                result = result.mul(base)?;
            }
            rest >>= 1;
            if rest > 0 {
                base = base.mul(base)?;
            }
        }
        if power < 0 {
            Self::ONE.div(result)
        } else {
            Some(result)
        }
    }

    /// The magnitude as a decimal, `None` when it is out of the decimal's
    /// range or so small it rounds to zero.
    pub(crate) fn to_decimal(self) -> Option<Decimal> {
        if self.mantissa.is_zero() {
            return Some(Decimal::ZERO);
        }
        let mut value = self.mantissa;
        if self.exponent >= 0 {
            for _ in 0..self.exponent {
                value = value.checked_mul(Decimal::TEN)?;
            }
            return Some(value);
        }
        let down = self.exponent.unsigned_abs();
        if down > MAX_SCALE + 1 {
            return None;
        }
        let scale = value.scale().checked_add(down)?;
        let value = if scale <= MAX_SCALE {
            Decimal::try_from_i128_with_scale(value.mantissa(), scale).ok()?
        } else {
            let excess = scale - MAX_SCALE;
            let kept = Decimal::try_from_i128_with_scale(value.mantissa(), excess)
                .ok()?
                .round_dp_with_strategy(0, RoundingStrategy::MidpointNearestEven);
            Decimal::try_from_i128_with_scale(kept.mantissa(), MAX_SCALE).ok()?
        };
        (!value.is_zero()).then_some(value)
    }
}

impl PartialOrd for Magnitude {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Magnitude {
    /// Orders positive magnitudes by size, which is what a unit's magnitude
    /// always is.
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.mantissa.is_zero(), other.mantissa.is_zero()) {
            (true, true) => Ordering::Equal,
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => self
                .exponent
                .cmp(&other.exponent)
                .then_with(|| self.mantissa.cmp(&other.mantissa)),
        }
    }
}

/// A ratio of two magnitudes, divided only when a value is converted so a
/// definition such as `m/3937` stays exact until then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ratio {
    numerator: Magnitude,
    denominator: Magnitude,
}

impl Ratio {
    /// The ratio one.
    pub(crate) const ONE: Self = Self {
        numerator: Magnitude::ONE,
        denominator: Magnitude::ONE,
    };

    /// The ratio `magnitude / 1`.
    pub(crate) const fn new(magnitude: Magnitude) -> Self {
        Self {
            numerator: magnitude,
            denominator: Magnitude::ONE,
        }
    }

    /// The product, `None` on overflow.
    pub(crate) fn mul(self, other: Self) -> Option<Self> {
        Some(Self {
            numerator: self.numerator.mul(other.numerator)?,
            denominator: self.denominator.mul(other.denominator)?,
        })
    }

    /// The quotient, `None` on overflow.
    pub(crate) fn div(self, other: Self) -> Option<Self> {
        self.mul(other.inverse())
    }

    /// The reciprocal.
    pub(crate) const fn inverse(self) -> Self {
        Self {
            numerator: self.denominator,
            denominator: self.numerator,
        }
    }

    /// The integer power, `None` on overflow.
    pub(crate) fn pow(self, power: i32) -> Option<Self> {
        let times = power.checked_abs()?;
        let raised = Self {
            numerator: self.numerator.pow(times)?,
            denominator: self.denominator.pow(times)?,
        };
        Some(if power < 0 { raised.inverse() } else { raised })
    }

    /// `value × self`, multiplying before the one division; `None` when the
    /// result leaves the decimal range or rounds to zero.
    pub(crate) fn apply(self, value: Decimal) -> Option<Decimal> {
        Magnitude::from_decimal(value)?
            .mul(self.numerator)?
            .div(self.denominator)?
            .to_decimal()
    }

    /// Orders two positive ratios by size, `None` on overflow.
    pub(crate) fn compare(self, other: Self) -> Option<Ordering> {
        let left = self.numerator.mul(other.denominator)?;
        let right = other.numerator.mul(self.denominator)?;
        Some(left.cmp(&right))
    }
}

/// The number of decimal digits of a non-zero integer.
fn digit_count(mut value: u128) -> u32 {
    let mut digits = 0;
    while value > 0 {
        value /= 10;
        digits += 1;
    }
    digits
}

#[cfg(test)]
mod tests {
    use rust_decimal::Decimal;

    use super::{Magnitude, Ratio};

    fn d(text: &str) -> Decimal {
        text.parse().expect("a decimal")
    }

    #[test]
    fn the_essence_forms_read() {
        let gram = Magnitude::parse("64.79891").and_then(Magnitude::to_decimal);
        assert_eq!(gram, Some(d("64.79891")));
        let planck = Magnitude::parse("6.62607015e-34").expect("reads");
        let back = planck
            .mul(Magnitude::parse("1e34").expect("reads"))
            .and_then(Magnitude::to_decimal);
        assert_eq!(back, Some(d("6.62607015")));
        assert_eq!(
            Magnitude::parse("254e-2").and_then(Magnitude::to_decimal),
            Some(d("2.54"))
        );
        assert_eq!(Magnitude::parse("1.2.3"), None);
        assert_eq!(Magnitude::parse("-1"), None);
    }

    #[test]
    fn a_long_constant_rounds_to_28_decimal_places() {
        let pi =
            Magnitude::parse("3.1415926535897932384626433832795028841971693993751058209749445923")
                .and_then(Magnitude::to_decimal);
        assert_eq!(pi, Some(d("3.1415926535897932384626433833")));
    }

    #[test]
    fn powers_and_quotients_stay_exact_where_they_terminate() {
        let centi = Magnitude::parse("1e-2").expect("reads");
        assert_eq!(
            centi.pow(3).and_then(Magnitude::to_decimal),
            Some(d("0.000001"))
        );
        assert_eq!(
            centi.pow(-2).and_then(Magnitude::to_decimal),
            Some(d("10000"))
        );
        let ratio = Magnitude::parse("1e3")
            .zip(Magnitude::parse("1e-3"))
            .and_then(|(a, b)| a.div(b));
        assert_eq!(ratio.and_then(Magnitude::to_decimal), Some(d("1000000")));
    }

    #[test]
    fn out_of_range_answers_none() {
        assert_eq!(
            Magnitude::parse("1e40").and_then(Magnitude::to_decimal),
            None
        );
        assert_eq!(
            Magnitude::parse("1e-40").and_then(Magnitude::to_decimal),
            None
        );
        let tiny = Magnitude::parse("1e-28").and_then(Magnitude::to_decimal);
        assert_eq!(tiny, Some(d("0.0000000000000000000000000001")));
    }

    #[test]
    fn a_ratio_divides_once_so_a_survey_foot_converts_back_exactly() {
        let foot = Ratio::new(Magnitude::parse("1200").expect("reads"))
            .div(Ratio::new(Magnitude::parse("3937").expect("reads")))
            .expect("divides");
        assert_eq!(foot.apply(d("3937")), Some(d("1200")));
        assert_eq!(foot.inverse().apply(d("1200")), Some(d("3937")));
        assert_eq!(foot.compare(Ratio::ONE), Some(std::cmp::Ordering::Less));
    }

    #[test]
    fn magnitudes_order_by_size() {
        let milli = Magnitude::parse("1e-3").expect("reads");
        let inch = Magnitude::parse("2.54").expect("reads");
        assert!(milli < Magnitude::ONE);
        assert!(inch > Magnitude::ONE);
        assert!(Magnitude::parse("9").expect("reads") < Magnitude::parse("10").expect("reads"));
    }
}
