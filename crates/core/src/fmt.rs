/// Insert thousands separators into a numeric string while preserving sign and decimal part.
pub fn thousands_sep(s: &str) -> String {
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

/// Format an `f64` as money with two decimals and thousands separators (e.g. `18,400.00`).
pub fn fmt_money(v: f64) -> String {
    thousands_sep(&format!("{:.2}", v))
}

/// Format an `Option<unix_seconds>` as `YYYY-MM-DD`. Returns `—` for `None` or
/// invalid timestamps. Pure math — safe in wasm32 view code (no `now_utc`).
pub fn fmt_ts_date(ts: Option<i64>) -> String {
    let Some(t) = ts else { return "—".into() };
    unix_to_ymdhm(t)
        .map(|(y, m, d, _, _)| format!("{y:04}-{m:02}-{d:02}"))
        .unwrap_or_else(|| "—".into())
}

/// Format an `Option<unix_seconds>` as `MM-DD HH:MM`. Same wasm32-safety note.
pub fn fmt_ts_minute(ts: Option<i64>) -> String {
    let Some(t) = ts else { return "—".into() };
    unix_to_ymdhm(t)
        .map(|(_, m, d, hh, mm)| format!("{m:02}-{d:02} {hh:02}:{mm:02}"))
        .unwrap_or_else(|| "—".into())
}

/// Format an `Option<unix_seconds>` as `MM-DD`. Empty string for None / invalid
/// (callers that need an em-dash placeholder use `fmt_ts_date`).
pub fn fmt_ts_md(ts: Option<i64>) -> String {
    ts.and_then(unix_to_ymdhm)
        .map(|(_, m, d, _, _)| format!("{m:02}-{d:02}"))
        .unwrap_or_default()
}

/// Format an `Option<unix_seconds>` as `HH:MM`. Empty string for None / invalid.
pub fn fmt_ts_hm(ts: Option<i64>) -> String {
    ts.and_then(unix_to_ymdhm)
        .map(|(_, _, _, hh, mm)| format!("{hh:02}:{mm:02}"))
        .unwrap_or_default()
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

pub fn ymd_to_unix_midnight(year: i32, month: u8, day: u8) -> Option<i64> {
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day)?;
    Some(days * 86_400)
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

fn days_from_civil(year: i32, month: u8, day: u8) -> Option<i64> {
    if day > days_in_month(year, month)? {
        return None;
    }
    let y = i64::from(year) - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146_097 + doe - 719_468)
}

fn days_in_month(year: i32, month: u8) -> Option<u8> {
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => return None,
    })
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2024-05-01 00:00:00 UTC — chosen because the unix epoch is exact and the
    // expected MM-DD output ("05-01") doesn't drift across local timezones.
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

    #[test]
    fn fmt_money_pos_neg_zero() {
        assert_eq!(fmt_money(18_400.0), "18,400.00");
        assert_eq!(fmt_money(-42.5), "-42.50");
        assert_eq!(fmt_money(0.0), "0.00");
    }

    #[test]
    fn fmt_ts_date_none_em_dash() {
        // Distinct from fmt_ts_md / fmt_ts_hm which return empty strings —
        // fmt_ts_date is for cells that need a placeholder glyph.
        assert_eq!(fmt_ts_date(None), "—");
    }

    #[test]
    fn fmt_ts_md_known_and_none() {
        assert_eq!(fmt_ts_md(Some(TS_2024_05_01_UTC)), "05-01");
        assert_eq!(fmt_ts_md(None), "");
    }

    #[test]
    fn fmt_ts_hm_known_and_none() {
        // 09:15:00 after midnight UTC.
        let ts = TS_2024_05_01_UTC + 9 * 3600 + 15 * 60;
        assert_eq!(fmt_ts_hm(Some(ts)), "09:15");
        assert_eq!(fmt_ts_hm(None), "");
    }

    #[test]
    fn fmt_ts_minute_combines_md_and_hm() {
        let ts = TS_2024_05_01_UTC + 9 * 3600 + 15 * 60;
        assert_eq!(fmt_ts_minute(Some(ts)), "05-01 09:15");
        assert_eq!(fmt_ts_minute(None), "—");
    }

    #[test]
    fn unix_to_ymdhm_handles_epoch_leap_days_and_negative_timestamps() {
        assert_eq!(unix_to_ymdhm(0), Some((1970, 1, 1, 0, 0)));
        assert_eq!(unix_to_ymdhm(1_582_934_400), Some((2020, 2, 29, 0, 0)));
        assert_eq!(unix_to_ymdhm(-1), Some((1969, 12, 31, 23, 59)));
        assert_eq!(ymd_to_unix_midnight(2024, 2, 29), Some(1_709_164_800));
        assert_eq!(ymd_to_unix_midnight(2023, 2, 29), None);
    }
}
