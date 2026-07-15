use serde::{Deserialize, Serialize};

/// Envelope returned by keyset-paginated Open API list endpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

impl<T> Page<T> {
    pub fn new(items: Vec<T>, next_cursor: Option<String>) -> Self {
        Self { items, next_cursor }
    }
}

/// Common query parameters for keyset-paginated Open API list endpoints.
#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub struct PageQuery {
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub const DEFAULT_PAGE_LIMIT: u32 = 50;
pub const MAX_PAGE_LIMIT: u32 = 200;

#[cfg(feature = "ssr")]
mod server {
    use super::{PageQuery, DEFAULT_PAGE_LIMIT, MAX_PAGE_LIMIT};
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine as _;
    use std::fmt;

    const CURSOR_MAGIC: &[u8; 3] = b"EP\x01";
    const MAX_CURSOR_CHARS: usize = 1_024;

    /// Decoded key for descending `(sort_value, tie_breaker)` pagination.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct KeysetCursor {
        pub sort_value: i64,
        pub tie_breaker: String,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum PageError {
        InvalidLimit,
        InvalidCursor,
    }

    impl fmt::Display for PageError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                Self::InvalidLimit => write!(f, "limit must be between 1 and {MAX_PAGE_LIMIT}"),
                Self::InvalidCursor => f.write_str("invalid pagination cursor"),
            }
        }
    }

    impl std::error::Error for PageError {}

    impl PageQuery {
        pub fn validated_limit(&self) -> Result<i64, PageError> {
            let limit = self.limit.unwrap_or(DEFAULT_PAGE_LIMIT);
            if !(1..=MAX_PAGE_LIMIT).contains(&limit) {
                return Err(PageError::InvalidLimit);
            }
            Ok(i64::from(limit))
        }

        pub fn decode_cursor(&self, scope: &str) -> Result<Option<KeysetCursor>, PageError> {
            self.cursor
                .as_deref()
                .map(|raw| decode_cursor(scope, raw))
                .transpose()
        }
    }

    /// Encode a cursor with an endpoint-specific scope so it cannot be reused
    /// accidentally against a different resource.
    pub fn encode_cursor(scope: &str, sort_value: i64, tie_breaker: &str) -> String {
        let scope = scope.as_bytes();
        let tie_breaker = tie_breaker.as_bytes();
        debug_assert!(scope.len() <= u8::MAX as usize);
        debug_assert!(tie_breaker.len() <= u16::MAX as usize);

        let mut bytes = Vec::with_capacity(14 + scope.len() + tie_breaker.len());
        bytes.extend_from_slice(CURSOR_MAGIC);
        bytes.push(scope.len() as u8);
        bytes.extend_from_slice(scope);
        bytes.extend_from_slice(&sort_value.to_be_bytes());
        bytes.extend_from_slice(&(tie_breaker.len() as u16).to_be_bytes());
        bytes.extend_from_slice(tie_breaker);
        URL_SAFE_NO_PAD.encode(bytes)
    }

    pub fn decode_cursor(scope: &str, raw: &str) -> Result<KeysetCursor, PageError> {
        if raw.is_empty() || raw.len() > MAX_CURSOR_CHARS {
            return Err(PageError::InvalidCursor);
        }
        let bytes = URL_SAFE_NO_PAD
            .decode(raw)
            .map_err(|_| PageError::InvalidCursor)?;
        if bytes.len() < 14 || bytes.get(..3) != Some(CURSOR_MAGIC.as_slice()) {
            return Err(PageError::InvalidCursor);
        }

        let scope_len = bytes[3] as usize;
        let sort_start = 4usize
            .checked_add(scope_len)
            .ok_or(PageError::InvalidCursor)?;
        let id_len_start = sort_start.checked_add(8).ok_or(PageError::InvalidCursor)?;
        let id_start = id_len_start
            .checked_add(2)
            .ok_or(PageError::InvalidCursor)?;
        if id_start > bytes.len() || bytes.get(4..sort_start) != Some(scope.as_bytes()) {
            return Err(PageError::InvalidCursor);
        }

        let sort_value = i64::from_be_bytes(
            bytes[sort_start..id_len_start]
                .try_into()
                .map_err(|_| PageError::InvalidCursor)?,
        );
        let id_len = u16::from_be_bytes(
            bytes[id_len_start..id_start]
                .try_into()
                .map_err(|_| PageError::InvalidCursor)?,
        ) as usize;
        if id_start.checked_add(id_len) != Some(bytes.len()) || id_len == 0 {
            return Err(PageError::InvalidCursor);
        }
        let tie_breaker = std::str::from_utf8(&bytes[id_start..])
            .map_err(|_| PageError::InvalidCursor)?
            .to_owned();
        Ok(KeysetCursor {
            sort_value,
            tie_breaker,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn cursor_round_trip_is_url_safe_and_scope_bound() {
            let raw = encode_cursor("finance.transactions", 42, "17");
            assert!(!raw.contains(['+', '/', '=']));
            assert_eq!(
                decode_cursor("finance.transactions", &raw).unwrap(),
                KeysetCursor {
                    sort_value: 42,
                    tie_breaker: "17".into()
                }
            );
            assert_eq!(
                decode_cursor("fitness.workouts", &raw),
                Err(PageError::InvalidCursor)
            );
        }

        #[test]
        fn malformed_cursor_and_out_of_range_limit_are_rejected() {
            assert_eq!(
                decode_cursor("finance.transactions", "not!base64"),
                Err(PageError::InvalidCursor)
            );
            assert_eq!(
                PageQuery {
                    limit: Some(0),
                    cursor: None
                }
                .validated_limit(),
                Err(PageError::InvalidLimit)
            );
            assert_eq!(
                PageQuery {
                    limit: Some(MAX_PAGE_LIMIT + 1),
                    cursor: None
                }
                .validated_limit(),
                Err(PageError::InvalidLimit)
            );
        }
    }
}

#[cfg(feature = "ssr")]
pub use server::{encode_cursor, PageError};
