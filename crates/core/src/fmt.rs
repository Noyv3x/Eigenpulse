#[cfg(feature = "ssr")]
use chrono::{
    DateTime, Datelike, LocalResult, Months, NaiveDate, Offset, SecondsFormat, TimeDelta, TimeZone,
    Timelike, Utc,
};
#[cfg(feature = "ssr")]
use chrono_tz::Tz;
#[cfg(feature = "ssr")]
use std::sync::{Arc, RwLock};

/// A validated, immutable IANA timezone snapshot.
///
/// Server functions take one snapshot at request entry and use it for every
/// label and calendar boundary they produce. This prevents a concurrent
/// settings change from mixing two timezone rules inside one response. The
/// type is SSR-only so the hydrate bundle never carries the IANA database.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppTimezone(Tz);

/// A local civil date with no timezone or clock attached.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CalendarDate {
    pub year: i32,
    pub month: u8,
    pub day: u8,
}

#[cfg(feature = "ssr")]
impl CalendarDate {
    pub fn ymd(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn ym(self) -> String {
        format!("{:04}-{:02}", self.year, self.month)
    }

    fn naive(self) -> Option<NaiveDate> {
        NaiveDate::from_ymd_opt(self.year, u32::from(self.month), u32::from(self.day))
    }

    fn from_naive(value: NaiveDate) -> Self {
        Self {
            year: value.year(),
            month: value.month() as u8,
            day: value.day() as u8,
        }
    }
}

/// A UTC half-open interval representing one local calendar bucket.
#[cfg(feature = "ssr")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CalendarRange {
    pub label: String,
    pub start: i64,
    pub end: i64,
}

#[cfg(feature = "ssr")]
impl AppTimezone {
    pub fn parse(name: &str) -> Option<Self> {
        if name.is_empty() || name != name.trim() || name.len() > 64 {
            return None;
        }
        name.parse().ok().map(Self)
    }

    pub const fn utc() -> Self {
        Self(chrono_tz::UTC)
    }

    pub fn name(self) -> &'static str {
        self.0.name()
    }

    pub fn date(self, ts: i64) -> Option<CalendarDate> {
        let value = datetime_in_timezone(ts, self.0)?;
        Some(CalendarDate {
            year: value.year(),
            month: value.month() as u8,
            day: value.day() as u8,
        })
    }

    pub fn local_fields(self, ts: i64) -> Option<(i32, u8, u8, u8, u8)> {
        let value = datetime_in_timezone(ts, self.0)?;
        Some((
            value.year(),
            value.month() as u8,
            value.day() as u8,
            value.hour() as u8,
            value.minute() as u8,
        ))
    }

    pub fn fmt_date(self, ts: Option<i64>) -> String {
        ts.and_then(|value| self.date(value))
            .map(CalendarDate::ymd)
            .unwrap_or_else(|| "—".into())
    }

    pub fn fmt_minute(self, ts: Option<i64>) -> String {
        ts.and_then(|value| self.local_fields(value))
            .map(|(_, month, day, hour, minute)| {
                format!("{month:02}-{day:02} {hour:02}:{minute:02}")
            })
            .unwrap_or_else(|| "—".into())
    }

    /// Empty-string variant for HTML date input values.
    pub fn fmt_ymd(self, ts: Option<i64>) -> String {
        ts.and_then(|value| self.date(value))
            .map(CalendarDate::ymd)
            .unwrap_or_default()
    }

    pub fn fmt_rfc3339(self, ts: i64) -> String {
        datetime_in_timezone(ts, self.0)
            .map(|value| value.to_rfc3339_opts(SecondsFormat::Secs, false))
            .unwrap_or_default()
    }

    pub fn utc_offset_label(self, ts: i64) -> String {
        let Some(value) = datetime_in_timezone(ts, self.0) else {
            return "UTC".into();
        };
        let seconds = value.offset().fix().local_minus_utc();
        if seconds == 0 {
            return "UTC+00:00".into();
        }
        let sign = if seconds < 0 { '-' } else { '+' };
        let seconds = seconds.unsigned_abs();
        format!(
            "UTC{sign}{:02}:{:02}",
            seconds / 3_600,
            (seconds % 3_600) / 60
        )
    }

    /// First representable instant on a local calendar date.
    pub fn date_start(self, date: CalendarDate) -> Option<i64> {
        local_ymd_start_in(date.year, date.month, date.day, self.0)
    }

    /// A stable instant within a local date, suitable for date-only business
    /// inputs stored as UTC Unix seconds. Skipped civil dates are rejected.
    pub fn date_midpoint(self, date: CalendarDate) -> Option<i64> {
        let start = self.date_start(date)?;
        let next_date = CalendarDate::from_naive(date.naive()?.succ_opt()?);
        let end = self.date_start(next_date)?;
        (end > start).then(|| start + (end - start) / 2)
    }

    pub fn shift_date(self, date: CalendarDate, days: i64) -> Option<CalendarDate> {
        date.naive()?
            .checked_add_signed(TimeDelta::days(days))
            .map(CalendarDate::from_naive)
    }

    pub fn month_range(self, year: i32, month: u8) -> Option<CalendarRange> {
        let start_date = NaiveDate::from_ymd_opt(year, u32::from(month), 1)?;
        let end_date = start_date.checked_add_months(Months::new(1))?;
        let start_date = CalendarDate::from_naive(start_date);
        let end_date = CalendarDate::from_naive(end_date);
        Some(CalendarRange {
            label: start_date.ym(),
            start: self.date_start(start_date)?,
            end: self.date_start(end_date)?,
        })
    }

    pub fn recent_months(self, now: i64, count: u16) -> Option<Vec<CalendarRange>> {
        if count == 0 {
            return Some(Vec::new());
        }
        let today = self.date(now)?.naive()?;
        let current = NaiveDate::from_ymd_opt(today.year(), today.month(), 1)?;
        (0..count)
            .map(|index| {
                let months_back = u32::from(count - 1 - index);
                let start = current.checked_sub_months(Months::new(months_back))?;
                self.month_range(start.year(), start.month() as u8)
            })
            .collect()
    }

    /// Recent ISO weeks (Monday start), oldest first. Each boundary is
    /// converted independently so DST weeks may contain 167 or 169 hours.
    pub fn recent_weeks(self, now: i64, count: u16) -> Option<Vec<CalendarRange>> {
        if count == 0 {
            return Some(Vec::new());
        }
        let today = self.date(now)?.naive()?;
        let current_monday = today.checked_sub_signed(TimeDelta::days(i64::from(
            today.weekday().num_days_from_monday(),
        )))?;
        (0..count)
            .map(|index| {
                let weeks_back = i64::from(count - 1 - index);
                let start = current_monday.checked_sub_signed(TimeDelta::days(weeks_back * 7))?;
                let end = start.checked_add_signed(TimeDelta::days(7))?;
                let start = CalendarDate::from_naive(start);
                let end = CalendarDate::from_naive(end);
                Some(CalendarRange {
                    label: start.ymd(),
                    start: self.date_start(start)?,
                    end: self.date_start(end)?,
                })
            })
            .collect()
    }

    pub fn trailing_days_start(self, now: i64, days: u16) -> Option<i64> {
        if days == 0 {
            return None;
        }
        let today = self.date(now)?;
        let start = self.shift_date(today, -(i64::from(days) - 1))?;
        self.date_start(start)
    }
}

/// Mutable application setting whose readers always receive immutable copies.
#[cfg(feature = "ssr")]
#[derive(Clone)]
pub struct TimezoneStore {
    current: Arc<RwLock<AppTimezone>>,
    updates: Arc<tokio::sync::Mutex<()>>,
}

#[cfg(feature = "ssr")]
impl TimezoneStore {
    pub fn new(timezone: AppTimezone) -> Self {
        Self {
            current: Arc::new(RwLock::new(timezone)),
            updates: Arc::new(tokio::sync::Mutex::new(())),
        }
    }

    pub fn snapshot(&self) -> AppTimezone {
        *self
            .current
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn replace(&self, timezone: AppTimezone) {
        *self
            .current
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = timezone;
    }

    /// Serialize the database commit + in-memory publication performed by
    /// settings updates. Holding this guard prevents two concurrent saves
    /// from publishing in the opposite order from their database commits.
    pub async fn begin_update(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.updates.clone().lock_owned().await
    }
}

#[cfg(feature = "ssr")]
impl Default for TimezoneStore {
    fn default() -> Self {
        Self::new(AppTimezone::utc())
    }
}

/// Insert thousands separators into a numeric string while preserving sign and decimal part.
fn thousands_sep(s: &str) -> String {
    let (int_part, frac) = match s.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (s, None),
    };
    let neg = int_part.starts_with('-');
    let digits: &str = if neg { &int_part[1..] } else { int_part };
    let mut rev = String::new();
    for (i, ch) in digits.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            rev.push(',');
        }
        rev.push(ch);
    }
    let mut out: String = rev.chars().rev().collect();
    if neg {
        out = format!("-{out}");
    }
    if let Some(f) = frac {
        out.push('.');
        out.push_str(f);
    }
    out
}

/// Format an `f64` as integer with thousands separators (e.g. `18,400`).
pub fn fmt_int(v: f64) -> String {
    thousands_sep(&format!("{:.0}", v))
}

/// Convert unix seconds to UTC `(year, month, day, hour, minute)`.
///
/// Howard Hinnant's civil-date algorithm, using Euclidean division so negative
/// timestamps before 1970 still work. Keeps hydrate-side date formatting small
/// and avoids pulling the full `time` crate into the WASM bundle.
pub fn unix_to_ymdhm(ts: i64) -> Option<(i32, u8, u8, u8, u8)> {
    const MIN: i64 = -62_167_219_200; // 0000-01-01T00:00:00Z
    const MAX: i64 = 253_402_300_799; // 9999-12-31T23:59:59Z
    if !(MIN..=MAX).contains(&ts) {
        return None;
    }

    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days)?;
    let hour = (secs / 3_600) as u8;
    let minute = ((secs % 3_600) / 60) as u8;
    Some((year, month, day, hour, minute))
}

/// Whether a Unix timestamp is safe for every application date pipeline.
///
/// SQLite's calendar functions and the browser-facing formatters are only
/// reliable over the civil range used by the application. Reserving year
/// 9999 also leaves enough room for a positive local-time offset without
/// crossing SQLite's upper calendar boundary.
pub fn is_valid_app_timestamp(ts: i64) -> bool {
    unix_to_ymdhm(ts).is_some_and(|(year, _, _, _, _)| (1..=9998).contains(&year))
}

#[cfg(feature = "ssr")]
fn datetime_in_timezone(ts: i64, timezone: Tz) -> Option<DateTime<Tz>> {
    DateTime::<Utc>::from_timestamp(ts, 0).map(|utc| utc.with_timezone(&timezone))
}

#[cfg(feature = "ssr")]
fn local_ymd_start_in(year: i32, month: u8, day: u8, timezone: Tz) -> Option<i64> {
    let midnight =
        NaiveDate::from_ymd_opt(year, u32::from(month), u32::from(day))?.and_hms_opt(0, 0, 0)?;
    for minutes in 0..=2 * 24 * 60 {
        let candidate = midnight.checked_add_signed(TimeDelta::minutes(minutes))?;
        match timezone.from_local_datetime(&candidate) {
            LocalResult::Single(dt) => return Some(dt.timestamp()),
            LocalResult::Ambiguous(first, second) => {
                return Some(first.timestamp().min(second.timestamp()));
            }
            LocalResult::None => {}
        }
    }
    None
}

/// Parse a strict `YYYY-MM-DD` calendar date — exactly 4-2-2 ASCII digits with
/// single `-` separators — into `(year, month, day)`. Returns `None` for any
/// other shape. Calendar validity is checked by server-side timezone helpers
/// at the point where the date is converted to an application timestamp.
pub fn parse_ymd(s: &str) -> Option<(i32, u8, u8)> {
    let mut parts = s.split('-');
    let (y, m, d) = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(y), Some(m), Some(d), None) => (y, m, d),
        _ => return None,
    };
    if y.len() != 4 || m.len() != 2 || d.len() != 2 {
        return None;
    }
    if !(y.bytes().chain(m.bytes()).chain(d.bytes())).all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((y.parse().ok()?, m.parse().ok()?, d.parse().ok()?))
}

fn civil_from_days(days: i64) -> Option<(i32, u8, u8)> {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = mp + if mp < 10 { 3 } else { -9 };
    let year = y + if m <= 2 { 1 } else { 0 };
    let year = i32::try_from(year).ok()?;
    Some((year, m as u8, d as u8))
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2024-05-01 00:00:00 UTC — chosen because the unix epoch is exact and the
    // expected MM-DD output ("05-01") doesn't drift across local timezones.
    #[cfg(feature = "ssr")]
    const TS_2024_05_01_UTC: i64 = 1_714_521_600;

    #[test]
    fn thousands_sep_basic() {
        assert_eq!(thousands_sep("18400"), "18,400");
        assert_eq!(thousands_sep("1234567"), "1,234,567");
        assert_eq!(thousands_sep("999"), "999");
    }

    #[test]
    fn thousands_sep_decimals_and_negatives() {
        assert_eq!(thousands_sep("-18400.50"), "-18,400.50");
        assert_eq!(thousands_sep("-12.34"), "-12.34");
        assert_eq!(thousands_sep("0.00"), "0.00");
    }

    #[test]
    fn fmt_int_basic() {
        assert_eq!(fmt_int(18_400.0), "18,400");
        assert_eq!(fmt_int(0.0), "0");
        // 18_400.49.round() = 18_400; .50 (bank-rounded) might round to even,
        // both 18,400 and 18,401 are acceptable formatter outputs — we only
        // care that the thousands separator is in the right place.
        assert!(matches!(fmt_int(18_400.5).as_str(), "18,400" | "18,401"));
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn fmt_ts_date_none_em_dash() {
        assert_eq!(AppTimezone::utc().fmt_date(None), "—");
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn fmt_ts_minute_combines_md_and_hm() {
        let ts = TS_2024_05_01_UTC + 9 * 3600 + 15 * 60;
        let timezone = AppTimezone::utc();
        assert_eq!(timezone.fmt_minute(Some(ts)), "05-01 09:15");
        assert_eq!(timezone.fmt_minute(None), "—");
    }

    #[test]
    fn unix_to_ymdhm_handles_epoch_leap_days_and_negative_timestamps() {
        assert_eq!(unix_to_ymdhm(0), Some((1970, 1, 1, 0, 0)));
        assert_eq!(unix_to_ymdhm(1_582_934_400), Some((2020, 2, 29, 0, 0)));
        assert_eq!(unix_to_ymdhm(-1), Some((1969, 12, 31, 23, 59)));
    }

    #[test]
    fn application_timestamp_range_reserves_calendar_boundaries() {
        const YEAR_ONE: i64 = -62_135_596_800;
        const YEAR_9999: i64 = 253_370_764_800;
        assert!(is_valid_app_timestamp(YEAR_ONE));
        assert!(is_valid_app_timestamp(YEAR_9999 - 1));
        assert!(!is_valid_app_timestamp(YEAR_ONE - 1));
        assert!(!is_valid_app_timestamp(YEAR_9999));
        assert!(!is_valid_app_timestamp(i64::MAX));
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn timezone_rules_handle_offsets_and_dst_at_each_instant() {
        let shanghai = datetime_in_timezone(0, chrono_tz::Asia::Shanghai).unwrap();
        assert_eq!(
            (
                shanghai.year(),
                shanghai.month(),
                shanghai.day(),
                shanghai.hour()
            ),
            (1970, 1, 1, 8)
        );
        assert_eq!(
            shanghai.to_rfc3339_opts(SecondsFormat::Secs, false),
            "1970-01-01T08:00:00+08:00"
        );

        let winter = 1_705_320_000;
        let summer = 1_721_044_800;
        assert_eq!(
            datetime_in_timezone(winter, chrono_tz::America::New_York)
                .unwrap()
                .hour(),
            7
        );
        assert_eq!(
            datetime_in_timezone(summer, chrono_tz::America::New_York)
                .unwrap()
                .hour(),
            8
        );
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn timezone_names_are_strict_iana_values() {
        assert_eq!(
            AppTimezone::parse("Asia/Shanghai").map(AppTimezone::name),
            Some("Asia/Shanghai")
        );
        for invalid in [
            "",
            " Asia/Shanghai",
            "Asia/Shanghai ",
            "not-a-zone",
            "+08:00",
            "CST",
        ] {
            assert_eq!(AppTimezone::parse(invalid), None, "name={invalid:?}");
        }
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn local_day_boundaries_follow_dst_and_skipped_dates() {
        let new_york = AppTimezone::parse("America/New_York").unwrap();
        let before = new_york
            .date_start(CalendarDate {
                year: 2024,
                month: 3,
                day: 10,
            })
            .unwrap();
        let after = new_york
            .date_start(CalendarDate {
                year: 2024,
                month: 3,
                day: 11,
            })
            .unwrap();
        assert_eq!(unix_to_ymdhm(before), Some((2024, 3, 10, 5, 0)));
        assert_eq!(unix_to_ymdhm(after), Some((2024, 3, 11, 4, 0)));
        assert_eq!(after - before, 23 * 3_600);

        let apia = AppTimezone::parse("Pacific/Apia").unwrap();
        let skipped = CalendarDate {
            year: 2011,
            month: 12,
            day: 30,
        };
        assert_eq!(apia.date_midpoint(skipped), None);
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn calendar_ranges_use_local_boundaries_and_stable_snapshots() {
        let new_york = AppTimezone::parse("America/New_York").unwrap();
        let now = DateTime::parse_from_rfc3339("2024-03-13T12:00:00Z")
            .unwrap()
            .timestamp();
        let weeks = new_york.recent_weeks(now, 2).unwrap();
        assert_eq!(weeks[0].label, "2024-03-04");
        assert_eq!(weeks[0].end - weeks[0].start, 167 * 3_600);
        assert_eq!(weeks[1].label, "2024-03-11");

        let store = TimezoneStore::new(new_york);
        let request_snapshot = store.snapshot();
        store.replace(AppTimezone::parse("Asia/Shanghai").unwrap());
        assert_eq!(request_snapshot.name(), "America/New_York");
        assert_eq!(store.snapshot().name(), "Asia/Shanghai");
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn date_midpoint_round_trips_in_extreme_positive_offset() {
        let timezone = AppTimezone::parse("Pacific/Kiritimati").unwrap();
        let date = CalendarDate {
            year: 2026,
            month: 7,
            day: 14,
        };
        let timestamp = timezone.date_midpoint(date).unwrap();
        assert_eq!(timezone.date(timestamp), Some(date));
        assert_eq!(timezone.utc_offset_label(timestamp), "UTC+14:00");
    }

    #[test]
    fn parse_ymd_accepts_only_strict_4_2_2_digit_shape() {
        assert_eq!(parse_ymd("2026-05-08"), Some((2026, 5, 8)));
        // Shape rejects: wrong widths, non-digit, wrong separator count, blanks.
        for bad in [
            "",
            "2026-5-8",
            "26-05-08",
            "2026-05-8",
            "2026/05/08",
            "2026-05-08-1",
            "abcd-05-08",
            "2026-05",
            " 2026-05-08 ",
        ] {
            assert_eq!(parse_ymd(bad), None, "bad={bad}");
        }
        // Shape validation is deliberately separate from calendar validation.
        assert_eq!(parse_ymd("2026-02-31"), Some((2026, 2, 31)));
    }
}
