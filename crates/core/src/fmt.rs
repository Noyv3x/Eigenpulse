use crate::MinorAmount;

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

/// Format an integer minor-unit amount with `decimals` fractional places and
/// thousands separators. `fmt_minor(1_840_000, 2)` → `"18,400.00"`;
/// `fmt_minor(18_400, 0)` → `"18,400"`; `fmt_minor(-4_250, 2)` → `"-42.50"`;
/// `fmt_minor(50_000_000, 8)` → `"0.50000000"`. Pure integer/string math —
/// safe in wasm32 view code.
pub fn fmt_minor(amount: impl Into<MinorAmount>, decimals: u8) -> String {
    thousands_sep(&fmt_minor_raw(amount, decimals))
}

/// Like [`fmt_minor`] but without thousands separators — suitable for the
/// `value` of an `<input type="number">`. `fmt_minor_raw(123_456, 2)` →
/// `"1234.56"`.
pub fn fmt_minor_raw(amount: impl Into<MinorAmount>, decimals: u8) -> String {
    let amount = amount.into();
    if decimals == 0 {
        return amount.to_string();
    }
    let neg = amount.is_negative();
    let mag = amount.abs().as_i128() as u128;
    let scale = 10u128.pow(u32::from(decimals));
    let int_part = mag / scale;
    let frac_part = mag % scale;
    let sign = if neg { "-" } else { "" };
    format!(
        "{sign}{int_part}.{frac_part:0width$}",
        width = usize::from(decimals)
    )
}

/// Format only the integer (major-unit) part of a minor-unit amount, rounded
/// half-up to the nearest major unit, with thousands separators. For compact
/// KPI displays that intentionally drop the fractional part.
/// `fmt_minor_compact(1_840_050, 2)` → `"18,401"`.
pub fn fmt_minor_compact(amount: impl Into<MinorAmount>, decimals: u8) -> String {
    let amount = amount.into();
    if decimals == 0 {
        return thousands_sep(&amount.to_string());
    }
    let neg = amount.is_negative();
    let mag = amount.abs().as_i128() as u128;
    let scale = 10u128.pow(u32::from(decimals));
    let major = (mag + scale / 2) / scale;
    // `major != 0` keeps a rounded-to-zero negative from printing "-0".
    let s = if neg && major != 0 {
        format!("-{major}")
    } else {
        major.to_string()
    };
    thousands_sep(&s)
}

/// Parse a human decimal string into integer minor units at `decimals`
/// precision. Accepts an optional leading `-`, ASCII digits, and an optional
/// `.` fraction; fractional digits beyond `decimals` are rounded half-up.
/// Thousands separators, exponents, and other shapes return `None`, as does
/// an `i128` overflow. Pure math — safe in wasm32 view code.
pub fn parse_minor(s: &str, decimals: u8) -> Option<MinorAmount> {
    let s = s.trim();
    let (neg, body) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s),
    };
    let (int_str, frac_str) = match body.split_once('.') {
        Some((i, f)) => (i, f),
        None => (body, ""),
    };
    if int_str.is_empty() && frac_str.is_empty() {
        return None;
    }
    if !int_str.bytes().all(|b| b.is_ascii_digit()) || !frac_str.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let dec = usize::from(decimals);
    let scale = 10i128.pow(u32::from(decimals));
    let int_part: i128 = if int_str.is_empty() {
        0
    } else {
        int_str.parse().ok()?
    };
    // Fractional value scaled to `decimals`, rounding half-up on excess digits.
    let frac_value: i128 = if frac_str.len() <= dec {
        let padded = format!("{frac_str:0<dec$}");
        if padded.is_empty() {
            0
        } else {
            padded.parse().ok()?
        }
    } else {
        let kept: i128 = if dec == 0 {
            0
        } else {
            frac_str[..dec].parse().ok()?
        };
        let round_up = frac_str.as_bytes()[dec] >= b'5';
        kept + i128::from(round_up)
    };
    let total = int_part.checked_mul(scale)?.checked_add(frac_value)?;
    let total = if neg { -total } else { total };
    if total == i128::MIN {
        None
    } else {
        Some(MinorAmount::new(total))
    }
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

/// Format an `Option<unix_seconds>` as `YYYY-MM-DD`. Empty string for None /
/// invalid — the empty-fallback flavor of [`fmt_ts_date`], suitable as the
/// `value` of an `<input type="date">` where `"—"` would be invalid HTML.
pub fn fmt_ts_ymd(ts: Option<i64>) -> String {
    ts.and_then(unix_to_ymdhm)
        .map(|(y, m, d, _, _)| format!("{y:04}-{m:02}-{d:02}"))
        .unwrap_or_default()
}

/// Scale a major-unit amount (e.g. `500` yuan) into minor units (`50_000`
/// at 2 decimals).
pub fn major_to_minor(major: i64, decimals: u8) -> MinorAmount {
    MinorAmount::new(i128::from(major) * 10_i128.pow(u32::from(decimals)))
}

/// The `step` / smallest-positive value for an `<input type="number">` money
/// field at a given precision. `decimals=2` → `"0.01"`, `0` → `"1"`,
/// `8` → `"0.00000001"`.
pub fn amount_step(decimals: u8) -> String {
    if decimals == 0 {
        "1".to_string()
    } else {
        format!("0.{}1", "0".repeat(decimals as usize - 1))
    }
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

/// Parse a strict `YYYY-MM-DD` calendar date — exactly 4-2-2 ASCII digits with
/// single `-` separators — into `(year, month, day)`. Returns `None` for any
/// other shape. This only validates the textual shape; pair with
/// [`ymd_to_unix_midnight`] to reject impossible dates like `2026-02-31`.
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
    fn fmt_minor_handles_precision_sign_and_thousands() {
        // 2-decimal currency.
        assert_eq!(fmt_minor(1_840_000, 2), "18,400.00");
        assert_eq!(fmt_minor(-4_250, 2), "-42.50");
        assert_eq!(fmt_minor(5, 2), "0.05");
        assert_eq!(fmt_minor(0, 2), "0.00");
        // 0-decimal currency (e.g. JPY) drops the point entirely.
        assert_eq!(fmt_minor(18_400, 0), "18,400");
        assert_eq!(fmt_minor(-7, 0), "-7");
        // High-precision currency (e.g. BTC at 8 places).
        assert_eq!(fmt_minor(150_000_000, 8), "1.50000000");
        // 18-decimal crypto-style assets are formatted without floating point.
        assert_eq!(
            fmt_minor(MinorAmount::new(1_234_567_890_123_456_789), 18),
            "1.234567890123456789"
        );
    }

    #[test]
    fn fmt_minor_raw_omits_thousands_separators() {
        // `<input type="number">` values must stay comma-free.
        assert_eq!(fmt_minor_raw(123_456, 2), "1234.56");
        assert_eq!(fmt_minor_raw(18_400, 0), "18400");
        assert_eq!(fmt_minor_raw(-4_250, 2), "-42.50");
    }

    #[test]
    fn fmt_minor_compact_rounds_to_major_unit() {
        assert_eq!(fmt_minor_compact(1_840_050, 2), "18,401"); // .50 rounds up
        assert_eq!(fmt_minor_compact(1_840_049, 2), "18,400");
        assert_eq!(fmt_minor_compact(18_400, 0), "18,400");
        // A negative that rounds to zero must not print "-0".
        assert_eq!(fmt_minor_compact(-30, 2), "0");
        assert_eq!(fmt_minor_compact(-1_840_050, 2), "-18,401");
    }

    #[test]
    fn parse_minor_scales_rounds_and_rejects_bad_shapes() {
        assert_eq!(parse_minor("42", 2), Some(MinorAmount::from(4_200)));
        assert_eq!(parse_minor("42.5", 2), Some(MinorAmount::from(4_250)));
        assert_eq!(parse_minor("-42.50", 2), Some(MinorAmount::from(-4_250)));
        assert_eq!(parse_minor(" 0 ", 2), Some(MinorAmount::ZERO));
        // Excess fractional digits round half-up; carry propagates.
        assert_eq!(parse_minor("0.567", 2), Some(MinorAmount::from(57)));
        assert_eq!(parse_minor("1.999", 2), Some(MinorAmount::from(200)));
        // 0-decimal currency rounds the whole fraction away.
        assert_eq!(parse_minor("42.5", 0), Some(MinorAmount::from(43)));
        assert_eq!(parse_minor("42.4", 0), Some(MinorAmount::from(42)));
        // 18-decimal crypto currency, beyond i64::MAX.
        assert_eq!(
            parse_minor("12345.000000000000000001", 18),
            Some(MinorAmount::new(12_345_000_000_000_000_000_001))
        );
        // Bad shapes: thousands separators, exponents, blanks, double dots.
        for bad in ["", "-", "1,234.56", "1e3", "abc", "4..2", "4.2.1", "."] {
            assert_eq!(parse_minor(bad, 2), None, "bad={bad}");
        }
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
        // Shape is valid but the date is impossible — parse_ymd still returns
        // it; ymd_to_unix_midnight is what rejects 2026-02-31.
        assert_eq!(parse_ymd("2026-02-31"), Some((2026, 2, 31)));
        assert_eq!(ymd_to_unix_midnight(2026, 2, 31), None);
    }
}
