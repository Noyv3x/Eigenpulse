//! Shared inline-style constants for form controls across module views.
//!
//! These were duplicated byte-for-byte in `finance`, `fitness`, and
//! `learning` view modules; they now live here as the single source of
//! truth so a styling tweak is a one-line edit. The string values are
//! preserved verbatim to keep visual outcomes identical.

/// Standard text/select input chrome: padding, border, radius, surface.
pub const INPUT_STYLE: &str =
    "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)";

/// Same as [`INPUT_STYLE`] but with the monospace font (numeric/date inputs).
pub const INPUT_STYLE_MONO: &str = "padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono)";

/// Small uppercase field-label styling for the `<span class="mono dim">`
/// captions above form inputs.
pub const FIELD_LABEL: &str = "font-size:11px;text-transform:uppercase;letter-spacing:0.06em";
