#[cfg(feature = "ssr")]
pub fn unix_now() -> i64 {
    time::OffsetDateTime::now_utc().unix_timestamp()
}
