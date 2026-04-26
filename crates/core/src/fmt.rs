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
