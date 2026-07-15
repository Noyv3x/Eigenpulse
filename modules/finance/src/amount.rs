use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(feature = "ssr")]
use std::borrow::Cow;

/// Exact signed minor-unit amount.
///
/// The value is intentionally serialized as a decimal string so server-fn and
/// OpenAPI responses do not pass large crypto amounts through JSON numbers.
#[derive(Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MinorAmount(i128);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParseMinorAmountError;

impl fmt::Display for ParseMinorAmountError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid minor amount")
    }
}

impl std::error::Error for ParseMinorAmountError {}

impl MinorAmount {
    pub const ZERO: Self = Self(0);

    /// Largest absolute amount accepted from forms, JSON APIs, and imports.
    ///
    /// With the maximum supported precision of 18 decimal places this still
    /// permits one trillion major units. Keeping an explicit business bound
    /// at ingress also leaves ample headroom for checked aggregate math.
    pub const MAX_INPUT_ABS: Self = Self(1_000_000_000_000_000_000_000_000_000_000);

    pub fn new(amount: i128) -> Self {
        Self::try_new(amount).expect("MinorAmount cannot hold i128::MIN")
    }

    /// Construct an amount without panicking. `i128::MIN` is excluded so
    /// [`MinorAmount::abs`] remains total for every representable value.
    pub const fn try_new(amount: i128) -> Option<Self> {
        if amount == i128::MIN {
            None
        } else {
            Some(Self(amount))
        }
    }

    pub const fn as_i128(self) -> i128 {
        self.0
    }

    pub fn is_positive(self) -> bool {
        self.0 > 0
    }

    pub fn is_negative(self) -> bool {
        self.0 < 0
    }

    pub fn abs(self) -> Self {
        Self::new(self.0.abs())
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).and_then(Self::try_new)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).and_then(Self::try_new)
    }

    pub fn checked_neg(self) -> Option<Self> {
        self.0.checked_neg().and_then(Self::try_new)
    }

    /// Checked aggregation for monetary values. Unlike `Iterator::sum`, an
    /// overflow is represented as `None` and can be mapped to a domain error
    /// instead of aborting the release process.
    pub fn try_sum(iter: impl IntoIterator<Item = Self>) -> Option<Self> {
        iter.into_iter()
            .try_fold(Self::ZERO, |total, amount| total.checked_add(amount))
    }

    /// Whether this amount is within the supported boundary for untrusted
    /// input. Internal aggregates may exceed this limit, but must still use
    /// checked arithmetic.
    pub fn is_within_input_limit(self) -> bool {
        self.abs() <= Self::MAX_INPUT_ABS
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64
    }
}

impl fmt::Debug for MinorAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "MinorAmount({})", self.0)
    }
}

impl fmt::Display for MinorAmount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i64> for MinorAmount {
    fn from(value: i64) -> Self {
        Self(i128::from(value))
    }
}

impl From<i32> for MinorAmount {
    fn from(value: i32) -> Self {
        Self(i128::from(value))
    }
}

impl FromStr for MinorAmount {
    type Err = ParseMinorAmountError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() || s == "-" || s == "+" {
            return Err(ParseMinorAmountError);
        }
        if s.contains('.') {
            return Err(ParseMinorAmountError);
        }
        let value: i128 = s.parse().map_err(|_| ParseMinorAmountError)?;
        if value == i128::MIN {
            return Err(ParseMinorAmountError);
        }
        Ok(Self(value))
    }
}

impl Serialize for MinorAmount {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_string())
    }
}

impl<'de> Deserialize<'de> for MinorAmount {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = MinorAmount;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a signed integer minor-unit amount encoded as a string")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                value.parse().map_err(|_| {
                    E::invalid_value(serde::de::Unexpected::Str(value), &"signed integer string")
                })
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(MinorAmount::from(value))
            }

            fn visit_i128<E>(self, value: i128) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                if value == i128::MIN {
                    Err(E::custom("i128::MIN is not a valid MinorAmount"))
                } else {
                    Ok(MinorAmount::new(value))
                }
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(MinorAmount::new(i128::from(value)))
            }

            fn visit_u128<E>(self, value: u128) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                let value = i128::try_from(value)
                    .map_err(|_| E::custom("u128 amount does not fit in i128"))?;
                Ok(MinorAmount::new(value))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

impl PartialEq<i64> for MinorAmount {
    fn eq(&self, other: &i64) -> bool {
        self.0 == i128::from(*other)
    }
}

impl PartialOrd<i64> for MinorAmount {
    fn partial_cmp(&self, other: &i64) -> Option<std::cmp::Ordering> {
        self.0.partial_cmp(&i128::from(*other))
    }
}

#[cfg(feature = "ssr")]
impl sqlx::Type<sqlx::Sqlite> for MinorAmount {
    fn type_info() -> sqlx::sqlite::SqliteTypeInfo {
        <String as sqlx::Type<sqlx::Sqlite>>::type_info()
    }

    fn compatible(ty: &sqlx::sqlite::SqliteTypeInfo) -> bool {
        <String as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
            || <i64 as sqlx::Type<sqlx::Sqlite>>::compatible(ty)
    }
}

#[cfg(feature = "ssr")]
impl<'q> sqlx::Encode<'q, sqlx::Sqlite> for MinorAmount {
    fn encode_by_ref(
        &self,
        args: &mut Vec<sqlx::sqlite::SqliteArgumentValue<'q>>,
    ) -> Result<sqlx::encode::IsNull, sqlx::error::BoxDynError> {
        args.push(sqlx::sqlite::SqliteArgumentValue::Text(Cow::Owned(
            self.0.to_string(),
        )));
        Ok(sqlx::encode::IsNull::No)
    }
}

#[cfg(feature = "ssr")]
impl<'r> sqlx::Decode<'r, sqlx::Sqlite> for MinorAmount {
    fn decode(value: sqlx::sqlite::SqliteValueRef<'r>) -> Result<Self, sqlx::error::BoxDynError> {
        let text = <&str as sqlx::Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(text.parse()?)
    }
}

fn thousands_sep(value: &str) -> String {
    let (integer, fraction) = value
        .split_once('.')
        .map_or((value, None), |(integer, fraction)| {
            (integer, Some(fraction))
        });
    let (sign, digits) = integer
        .strip_prefix('-')
        .map_or(("", integer), |digits| ("-", digits));
    let mut reversed = String::with_capacity(value.len() + value.len() / 3);
    for (index, digit) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            reversed.push(',');
        }
        reversed.push(digit);
    }
    let mut formatted = String::with_capacity(reversed.len() + sign.len() + 1);
    formatted.push_str(sign);
    formatted.extend(reversed.chars().rev());
    if let Some(fraction) = fraction {
        formatted.push('.');
        formatted.push_str(fraction);
    }
    formatted
}

/// Format an exact minor-unit amount with the currency's declared precision.
pub fn fmt_minor(amount: impl Into<MinorAmount>, decimals: u8) -> String {
    thousands_sep(&fmt_minor_raw(amount, decimals))
}

/// Format a minor-unit amount without separators for an HTML number input.
pub fn fmt_minor_raw(amount: impl Into<MinorAmount>, decimals: u8) -> String {
    let amount = amount.into();
    if decimals == 0 {
        return amount.to_string();
    }
    let negative = amount.is_negative();
    let magnitude = amount.abs().as_i128() as u128;
    let Some(scale) = 10_u128.checked_pow(u32::from(decimals)) else {
        return amount.to_string();
    };
    let integer = magnitude / scale;
    let fraction = magnitude % scale;
    let sign = if negative { "-" } else { "" };
    format!(
        "{sign}{integer}.{fraction:0width$}",
        width = usize::from(decimals)
    )
}

/// Parse a decimal major-unit value into exact minor units, rounding excess
/// fractional digits half-up.
#[cfg(any(feature = "ssr", test))]
pub fn parse_minor(input: &str, decimals: u8) -> Option<MinorAmount> {
    let input = input.trim();
    let (negative, body) = input
        .strip_prefix('-')
        .map_or((false, input), |body| (true, body));
    let (integer, fraction) = body.split_once('.').unwrap_or((body, ""));
    if integer.is_empty() && fraction.is_empty() {
        return None;
    }
    if !integer.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let decimals = usize::from(decimals);
    let scale = 10_i128.checked_pow(u32::try_from(decimals).ok()?)?;
    let integer: i128 = if integer.is_empty() {
        0
    } else {
        integer.parse().ok()?
    };
    let fraction = if fraction.len() <= decimals {
        let padded = format!("{fraction:0<decimals$}");
        if padded.is_empty() {
            0
        } else {
            padded.parse().ok()?
        }
    } else {
        let kept: i128 = if decimals == 0 {
            0
        } else {
            fraction[..decimals].parse().ok()?
        };
        kept.checked_add(i128::from(fraction.as_bytes()[decimals] >= b'5'))?
    };
    let amount = integer.checked_mul(scale)?.checked_add(fraction)?;
    MinorAmount::try_new(if negative {
        amount.checked_neg()?
    } else {
        amount
    })
}

/// Scale an integer major-unit amount to the currency's minor units.
#[cfg(any(feature = "ssr", test))]
pub fn major_to_minor(major: i64, decimals: u8) -> Option<MinorAmount> {
    let scale = 10_i128.checked_pow(u32::from(decimals))?;
    i128::from(major)
        .checked_mul(scale)
        .and_then(MinorAmount::try_new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_uses_strings_for_json_precision() {
        let amount = MinorAmount::new(12_345_000_000_000_000_000_001);
        assert_eq!(
            serde_json::to_string(&amount).unwrap(),
            "\"12345000000000000000001\""
        );
        assert_eq!(
            serde_json::from_str::<MinorAmount>("\"12345000000000000000001\"").unwrap(),
            amount
        );
    }

    #[test]
    fn rejects_i128_min_so_abs_stays_total() {
        assert!(i128::MIN.to_string().parse::<MinorAmount>().is_err());
        assert_eq!(MinorAmount::from(-42).abs(), MinorAmount::from(42));
    }

    #[test]
    fn checked_operations_reject_i128_min_without_panicking() {
        let near_min = MinorAmount::new(i128::MIN + 1);
        assert_eq!(near_min.checked_add(MinorAmount::from(-1)), None);
        assert_eq!(near_min.checked_sub(MinorAmount::from(1)), None);
        assert_eq!(MinorAmount::try_new(i128::MIN), None);
        assert_eq!(
            MinorAmount::try_sum([near_min, MinorAmount::from(-1)]),
            None
        );
    }

    #[test]
    fn input_limit_leaves_aggregate_headroom() {
        let max = MinorAmount::MAX_INPUT_ABS;
        assert!(max.is_within_input_limit());
        assert!(max.checked_neg().unwrap().is_within_input_limit());
        assert!(!max
            .checked_add(MinorAmount::from(1))
            .unwrap()
            .is_within_input_limit());
    }

    #[test]
    fn formatting_preserves_precision_sign_and_input_shape() {
        assert_eq!(fmt_minor(1_840_000, 2), "18,400.00");
        assert_eq!(fmt_minor(-4_250, 2), "-42.50");
        assert_eq!(fmt_minor(150_000_000, 8), "1.50000000");
        assert_eq!(fmt_minor_raw(123_456, 2), "1234.56");
        assert_eq!(fmt_minor_raw(-4_250, 2), "-42.50");
    }

    #[test]
    fn parsing_scales_rounds_and_rejects_invalid_shapes() {
        assert_eq!(parse_minor("42.5", 2), Some(MinorAmount::from(4_250)));
        assert_eq!(parse_minor("-42.50", 2), Some(MinorAmount::from(-4_250)));
        assert_eq!(parse_minor("0.567", 2), Some(MinorAmount::from(57)));
        assert_eq!(parse_minor("1.999", 2), Some(MinorAmount::from(200)));
        assert_eq!(
            parse_minor("12345.000000000000000001", 18),
            Some(MinorAmount::new(12_345_000_000_000_000_000_001))
        );
        for invalid in ["", "-", "1,234.56", "1e3", "abc", "4..2", "."] {
            assert_eq!(parse_minor(invalid, 2), None, "invalid={invalid}");
        }
    }

    #[test]
    fn scaling_rejects_unsupported_precision() {
        assert_eq!(major_to_minor(500, 2), Some(MinorAmount::from(50_000)));
        assert_eq!(major_to_minor(i64::MAX, u8::MAX), None);
        assert_eq!(parse_minor("1", u8::MAX), None);
    }
}
