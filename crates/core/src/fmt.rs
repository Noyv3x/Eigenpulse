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
    time::OffsetDateTime::from_unix_timestamp(t)
        .ok()
        .map(|d| format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day()))
        .unwrap_or_else(|| "—".into())
}

/// Format an `Option<unix_seconds>` as `MM-DD HH:MM`. Same wasm32-safety note.
pub fn fmt_ts_minute(ts: Option<i64>) -> String {
    let Some(t) = ts else { return "—".into() };
    time::OffsetDateTime::from_unix_timestamp(t)
        .ok()
        .map(|d| format!("{:02}-{:02} {:02}:{:02}", d.month() as u8, d.day(), d.hour(), d.minute()))
        .unwrap_or_else(|| "—".into())
}

/// Format an `Option<unix_seconds>` as `MM-DD`. Empty string for None / invalid
/// (callers that need an em-dash placeholder use `fmt_ts_date`).
pub fn fmt_ts_md(ts: Option<i64>) -> String {
    ts.and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok())
        .map(|d| format!("{:02}-{:02}", d.month() as u8, d.day()))
        .unwrap_or_default()
}

/// Format an `Option<unix_seconds>` as `HH:MM`. Empty string for None / invalid.
pub fn fmt_ts_hm(ts: Option<i64>) -> String {
    ts.and_then(|t| time::OffsetDateTime::from_unix_timestamp(t).ok())
        .map(|d| format!("{:02}:{:02}", d.hour(), d.minute()))
        .unwrap_or_default()
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
}
