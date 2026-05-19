use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};
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
    pub const ONE: Self = Self(1);

    pub fn new(amount: i128) -> Self {
        assert!(amount != i128::MIN, "MinorAmount cannot hold i128::MIN");
        Self(amount)
    }

    pub const fn zero() -> Self {
        Self::ZERO
    }

    pub const fn as_i128(self) -> i128 {
        self.0
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
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

    pub fn max(self, other: Self) -> Self {
        if self >= other {
            self
        } else {
            other
        }
    }

    pub fn min(self, other: Self) -> Self {
        if self <= other {
            self
        } else {
            other
        }
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        self.0.checked_add(rhs.0).map(Self::new)
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        self.0.checked_sub(rhs.0).map(Self::new)
    }

    pub fn checked_mul_i128(self, rhs: i128) -> Option<Self> {
        self.0.checked_mul(rhs).map(Self::new)
    }

    pub fn checked_div_i128(self, rhs: i128) -> Option<Self> {
        self.0.checked_div(rhs).map(Self::new)
    }

    pub fn to_f64(self) -> f64 {
        self.0 as f64
    }

    pub fn to_db_string(self) -> String {
        self.0.to_string()
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

impl From<i128> for MinorAmount {
    fn from(value: i128) -> Self {
        Self::new(value)
    }
}

impl TryFrom<MinorAmount> for i64 {
    type Error = std::num::TryFromIntError;

    fn try_from(value: MinorAmount) -> Result<Self, Self::Error> {
        i64::try_from(value.0)
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

impl Add for MinorAmount {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        self.checked_add(rhs)
            .expect("MinorAmount addition overflow")
    }
}

impl AddAssign for MinorAmount {
    fn add_assign(&mut self, rhs: Self) {
        *self = *self + rhs;
    }
}

impl Sub for MinorAmount {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        self.checked_sub(rhs)
            .expect("MinorAmount subtraction overflow")
    }
}

impl SubAssign for MinorAmount {
    fn sub_assign(&mut self, rhs: Self) {
        *self = *self - rhs;
    }
}

impl Neg for MinorAmount {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(self.0.checked_neg().expect("MinorAmount negation overflow"))
    }
}

impl Mul<i128> for MinorAmount {
    type Output = Self;

    fn mul(self, rhs: i128) -> Self::Output {
        self.checked_mul_i128(rhs)
            .expect("MinorAmount multiplication overflow")
    }
}

impl Div<i128> for MinorAmount {
    type Output = Self;

    fn div(self, rhs: i128) -> Self::Output {
        self.checked_div_i128(rhs)
            .expect("MinorAmount division by zero or overflow")
    }
}

impl Sum for MinorAmount {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, |acc, amount| acc + amount)
    }
}

impl<'a> Sum<&'a MinorAmount> for MinorAmount {
    fn sum<I: Iterator<Item = &'a MinorAmount>>(iter: I) -> Self {
        iter.copied().sum()
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
}
