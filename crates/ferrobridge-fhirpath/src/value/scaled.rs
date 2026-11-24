// SPDX-FileCopyrightText: Cadasto B.V.
// SPDX-License-Identifier: BUSL-1.1

//! A decimal the exact decimal cannot hold with its stated precision.
//!
//! It is kept in the form a FHIR R4 `decimal` is written in, exponent included (FHIR R4
//! datatypes.html §2.24.0.1), so `1e999999999` is never written out in full.

use std::cmp::Ordering;
use std::fmt;

use rust_decimal::Decimal;

/// A decimal as its sign, its significand's digits and the power of ten of
/// the last digit: `-1.50e3` is `-`, `150` and `1`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scaled {
    /// Whether the value is below zero.
    negative: bool,
    /// The significand's digits without leading zeros, `0` for zero.
    digits: String,
    /// The power of ten of the last digit.
    exponent: i64,
}

impl Scaled {
    /// Reads a decimal in the FHIR R4 lexical form
    /// `-?(0|[1-9][0-9]*)(\.[0-9]+)?([eE][+-]?[0-9]+)?`, `None` for any other
    /// text or an exponent past the 64-bit range.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let (negative, unsigned) = match text.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, text),
        };
        let (mantissa, exponent) = match unsigned.split_once(['e', 'E']) {
            Some((mantissa, exponent)) => {
                let digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
                if !all_digits(digits) {
                    return None;
                }
                (mantissa, exponent.parse::<i64>().ok()?)
            }
            None => (unsigned, 0),
        };
        let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
        if !all_digits(whole) || (mantissa.contains('.') && !all_digits(fraction)) {
            return None;
        }
        let places = i64::try_from(fraction.len()).ok()?;
        Some(Self::new(
            negative,
            &format!("{whole}{fraction}"),
            exponent.checked_sub(places)?,
        ))
    }

    /// The value with leading zeros dropped from `digits`.
    fn new(negative: bool, digits: &str, exponent: i64) -> Self {
        let trimmed = digits.trim_start_matches('0');
        let digits = if trimmed.is_empty() {
            String::from("0")
        } else {
            String::from(trimmed)
        };
        Self {
            negative,
            digits,
            exponent,
        }
    }

    /// The exact decimal with the same value and stated precision, `None`
    /// when it holds neither.
    #[must_use]
    pub fn to_decimal(&self) -> Option<Decimal> {
        let mantissa = self.digits.parse::<i128>().ok()?;
        let mantissa = if self.negative { -mantissa } else { mantissa };
        if self.exponent >= 0 {
            let power = 10_i128.checked_pow(u32::try_from(self.exponent).ok()?)?;
            Decimal::try_from_i128_with_scale(mantissa.checked_mul(power)?, 0).ok()
        } else {
            let scale = u32::try_from(self.exponent.checked_neg()?).ok()?;
            Decimal::try_from_i128_with_scale(mantissa, scale).ok()
        }
    }

    /// The exact decimal equal to the value once trailing zeros are dropped,
    /// `None` when it has none.
    #[must_use]
    pub fn normalized(&self) -> Option<Decimal> {
        self.trimmed().to_decimal()
    }

    /// The value with the precision of `decimal`.
    #[must_use]
    pub fn from_decimal(decimal: Decimal) -> Self {
        let mantissa = decimal.mantissa();
        Self::new(
            mantissa < 0,
            &mantissa.unsigned_abs().to_string(),
            -i64::from(decimal.scale()),
        )
    }

    /// The value with its sign inverted.
    #[must_use]
    pub fn negated(&self) -> Self {
        Self {
            negative: !self.negative,
            ..self.clone()
        }
    }

    /// Whether the value is zero.
    fn is_zero(&self) -> bool {
        self.digits == "0"
    }

    /// The power of ten of the leading digit, `0` for zero.
    #[must_use]
    pub fn magnitude(&self) -> i128 {
        if self.is_zero() {
            return 0;
        }
        i128::from(self.exponent) + i128::from(self.digit_count()) - 1
    }

    /// The number of digits in the significand.
    fn digit_count(&self) -> i64 {
        i64::try_from(self.digits.len()).unwrap_or(i64::MAX)
    }

    /// The digits after the decimal point the value states, `0` when its
    /// last digit is at or above the units.
    #[must_use]
    pub fn places(&self) -> i128 {
        (-i128::from(self.exponent)).max(0)
    }

    /// The fixed-point form, `1.50` or `0.001`, when it takes at most `limit`
    /// digits; otherwise the number of digits it takes.
    ///
    /// # Errors
    ///
    /// Returns the digits the form takes when they are more than `limit`.
    pub fn fixed(&self, limit: u128) -> Result<String, u128> {
        let length = i128::try_from(self.digits.len()).unwrap_or(i128::MAX);
        let exponent = i128::from(self.exponent);
        let digits = if exponent >= 0 {
            length + exponent
        } else {
            length.max(-exponent + 1)
        };
        let digits = digits.unsigned_abs();
        if digits > limit {
            return Err(digits);
        }
        let sign = if self.negative && !self.is_zero() {
            "-"
        } else {
            ""
        };
        let places = usize::try_from(-exponent).unwrap_or(0);
        let text = if exponent >= 0 {
            let zeros = usize::try_from(exponent).unwrap_or(0);
            format!("{sign}{}{}", self.digits, "0".repeat(zeros))
        } else if self.digits.len() > places {
            let (whole, fraction) = self.digits.split_at(self.digits.len() - places);
            format!("{sign}{whole}.{fraction}")
        } else {
            let zeros = "0".repeat(places - self.digits.len());
            format!("{sign}0.{zeros}{}", self.digits)
        };
        Ok(text)
    }

    /// The value without trailing zeros: `1.50` is `1.5`, and zero is `0`.
    fn trimmed(&self) -> Self {
        if self.is_zero() {
            return Self::new(false, "0", self.exponent.max(0));
        }
        let digits = self.digits.trim_end_matches('0');
        let dropped = i64::try_from(self.digits.len() - digits.len()).unwrap_or(i64::MAX);
        Self::new(self.negative, digits, self.exponent.saturating_add(dropped))
    }

    /// The order of two values.
    #[must_use]
    pub fn compare(&self, other: &Self) -> Ordering {
        let sign = |value: &Self| match (value.is_zero(), value.negative) {
            (true, _) => 0,
            (false, true) => -1,
            (false, false) => 1,
        };
        let (a, b) = (sign(self), sign(other));
        if a != b || a == 0 {
            return a.cmp(&b);
        }
        let order = self.magnitude().cmp(&other.magnitude()).then_with(|| {
            let width = self.digits.len().max(other.digits.len());
            padded(&self.digits, width).cmp(padded(&other.digits, width))
        });
        if a < 0 { order.reverse() } else { order }
    }

    /// Whether two values are equivalent: equal once each is rounded to the
    /// precision of the less precise, trailing zeros not counting (N1
    /// §6.1.2).
    #[must_use]
    pub fn equivalent(&self, other: &Self) -> bool {
        let (a, b) = (self.trimmed(), other.trimmed());
        let place = a.exponent.max(b.exponent);
        a.rounded(place).compare(&b.rounded(place)) == Ordering::Equal
    }

    /// The value rounded to the power of ten `place`, the midpoint to the
    /// even digit as `rust_decimal`'s `round_dp` does.
    fn rounded(&self, place: i64) -> Self {
        let dropped = i128::from(place) - i128::from(self.exponent);
        if dropped <= 0 {
            return self.clone();
        }
        let length = self.digits.len();
        let Some(kept) = usize::try_from(dropped)
            .ok()
            .and_then(|dropped| length.checked_sub(dropped))
        else {
            return Self::new(false, "0", place);
        };
        let (head, tail) = self.digits.split_at(kept);
        let last_odd = head
            .bytes()
            .last()
            .is_some_and(|digit| (digit - b'0') % 2 == 1);
        let up = match tail.bytes().next() {
            Some(b'6'..=b'9') => true,
            Some(b'5') => tail.bytes().skip(1).any(|digit| digit != b'0') || last_odd,
            _ => false,
        };
        let head = if head.is_empty() { "0" } else { head };
        let digits = if up {
            incremented(head)
        } else {
            String::from(head)
        };
        Self::new(self.negative, &digits, place)
    }
}

/// The digits of `digits` with zeros after them, `width` in all.
fn padded(digits: &str, width: usize) -> impl Iterator<Item = u8> + '_ {
    digits.bytes().chain(std::iter::repeat(b'0')).take(width)
}

/// Whether `text` is one or more ASCII digits.
fn all_digits(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A string of decimal digits plus one.
fn incremented(digits: &str) -> String {
    let mut bytes = digits.as_bytes().to_vec();
    for byte in bytes.iter_mut().rev() {
        if *byte == b'9' {
            *byte = b'0';
        } else {
            *byte += 1;
            return String::from_utf8_lossy(&bytes).into_owned();
        }
    }
    format!("1{}", String::from_utf8_lossy(&bytes))
}

impl fmt::Display for Scaled {
    /// The scientific form, `1E+999999999` or `-1.5E-3`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negative && !self.is_zero() {
            f.write_str("-")?;
        }
        let (lead, rest) = self.digits.split_at(1);
        f.write_str(lead)?;
        if !rest.is_empty() {
            write!(f, ".{rest}")?;
        }
        let magnitude = self.magnitude();
        let sign = if magnitude < 0 { '-' } else { '+' };
        write!(f, "E{sign}{}", magnitude.unsigned_abs())
    }
}

#[cfg(test)]
mod tests {
    use std::cmp::Ordering;

    use rust_decimal::Decimal;

    use super::Scaled;

    fn scaled(text: &str) -> Scaled {
        Scaled::parse(text).expect("the text should be an R4 decimal")
    }

    #[test]
    fn the_lexical_form_is_read_and_the_rest_refused() {
        assert_eq!(scaled("1e999999999").to_string(), "1E+999999999");
        assert_eq!(scaled("-1.50e-3").to_string(), "-1.50E-3");
        assert_eq!(scaled("0.0").to_string(), "0E+0");
        for refused in ["", "1.", ".5", "1e", "1e+", "--1", "1x", "1e1.5"] {
            assert_eq!(Scaled::parse(refused), None, "{refused}");
        }
    }

    #[test]
    fn a_value_the_exact_decimal_holds_converts_with_its_precision() {
        assert_eq!(scaled("1.50").to_decimal(), Some(Decimal::new(150, 2)));
        assert_eq!(scaled("9.5e2").to_decimal(), Some(Decimal::from(950)));
        assert_eq!(scaled("1e-3").to_decimal(), Some(Decimal::new(1, 3)));
        assert_eq!(scaled("1e999999999").to_decimal(), None);
        assert_eq!(scaled("1e-999999999").to_decimal(), None);
        let zeros = format!("1.{}", "0".repeat(30));
        assert_eq!(scaled(&zeros).to_decimal(), None);
        assert_eq!(scaled(&zeros).normalized(), Some(Decimal::ONE));
    }

    #[test]
    fn values_order_exactly() {
        let huge = scaled("1e999999999");
        let one = Scaled::from_decimal(Decimal::ONE);
        assert_eq!(huge.compare(&one), Ordering::Greater);
        assert_eq!(huge.negated().compare(&one), Ordering::Less);
        assert_eq!(
            scaled("1e-999999999").compare(&Scaled::from_decimal(Decimal::ZERO)),
            Ordering::Greater
        );
        assert_eq!(scaled("1.50").compare(&scaled("15e-1")), Ordering::Equal);
        assert_eq!(scaled("-2").compare(&scaled("-10")), Ordering::Greater);
    }

    #[test]
    fn the_fixed_point_form_keeps_the_stated_digits_within_its_limit() {
        assert_eq!(scaled("1.50").fixed(10), Ok(String::from("1.50")));
        assert_eq!(scaled("-1e-3").fixed(10), Ok(String::from("-0.001")));
        assert_eq!(scaled("9.5e2").fixed(10), Ok(String::from("950")));
        assert_eq!(scaled("1e999999999").fixed(10), Err(1_000_000_000));
        assert_eq!(scaled("1e-999999999").fixed(10), Err(1_000_000_000));
    }

    #[test]
    fn equivalence_rounds_to_the_less_precise() {
        let zero = Scaled::from_decimal(Decimal::ZERO);
        let one = Scaled::from_decimal(Decimal::ONE);
        assert!(scaled("1e-999999999").equivalent(&zero));
        assert!(!scaled("1e999999999").equivalent(&one));
        assert!(scaled("1.26").equivalent(&scaled("1.3")));
        assert!(!scaled("1.24").equivalent(&scaled("1.3")));
        assert!(scaled("0.6").equivalent(&one));
    }
}
