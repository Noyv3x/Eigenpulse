//! System status / maintenance page.
//!
//! Renders the OWNER-gated [`crate::admin::admin_status`] snapshot (binary
//! version, DB size, integrity, live session / notification counts, last
//! backup) plus a one-click "Back up now" action wired to
//! [`crate::admin::run_backup`]. All sizes and timestamps are precomputed on
//! the DTO server-side, keeping this view wasm-safe (no clock / fs on the
//! hydrate target).

use crate::admin::{admin_status, AdminStatus, RunBackupFn};
use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{Card, Icon, LoadError, PageHead, StatRow, Tag};
use leptos::prelude::*;

/// Human-readable byte size. Pure integer/string math (no float formatting
/// crate), wasm-safe. `1536` → `"1.5 KB"`, `0` → `"0 B"`. Shared with the
/// settings Data card.
pub fn fmt_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    // One decimal place, trimmed of a trailing ".0".
    let rounded = (value * 10.0).round() / 10.0;
    let text = if (rounded.fract()).abs() < f64::EPSILON {
        format!("{}", rounded as i64)
    } else {
        format!("{rounded:.1}")
    };
    format!("{text} {}", UNITS[unit])
}

#[component]
pub fn StatusView() -> impl IntoView {
    let locale = use_locale();
    let status = Resource::new(|| (), |_| async { admin_status().await });
    let backup = ServerAction::<RunBackupFn>::new();

    // Refetch the status snapshot after a backup completes so the "last backup"
    // row reflects the new file. Skip the initial (prev=None) pass so we don't
    // double-fetch on mount.
    Effect::new(move |prev: Option<()>| {
        backup.version().get();
        if prev.is_some() {
            status.refetch();
        }
    });

    view! {
        <div class="view">
            <PageHead
                code="CFG-STA-01"
                module=t(locale, "app.status.page.module")
                title=t(locale, "app.status.page.title")
                title_cn=t(locale, "app.status.page.title_cn")
                sub=t(locale, "app.status.page.sub")
            />

            <div class="grid-2">
                <Card title=t(locale, "app.status.system_card.title") code="CFG-STA-SYS">
                    <Suspense fallback=move || view! {
                        <div style="display:flex;flex-direction:column;gap:8px;padding:6px 0">
                            <span class="skeleton-line" style="height:14px;width:55%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:60%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:45%;display:block"></span>
                        </div>
                    }>
                        {move || status.get().map(|res| match res {
                            Err(e) => view! { <LoadError detail=server_fn_error_text(&e)/> }.into_any(),
                            Ok(s) => render_system(s).into_any(),
                        })}
                    </Suspense>
                </Card>

                <Card title=t(locale, "app.status.backup_card.title") code="CFG-STA-BK"
                      sub=t(locale, "app.status.backup_card.sub")>
                    <Suspense fallback=move || view! {
                        <div style="display:flex;flex-direction:column;gap:8px;padding:6px 0">
                            <span class="skeleton-line" style="height:14px;width:50%;display:block"></span>
                            <span class="skeleton-line" style="height:14px;width:40%;display:block"></span>
                        </div>
                    }>
                        {move || status.get().map(|res| match res {
                            Err(e) => view! { <LoadError detail=server_fn_error_text(&e)/> }.into_any(),
                            Ok(s) => render_backup_status(s).into_any(),
                        })}
                    </Suspense>

                    <div style="margin-top:14px">
                        <ActionForm action=backup attr:class="hstack" attr:style="gap:10px;align-items:center;flex-wrap:wrap">
                            <button class="btn primary" type="submit">
                                <Icon kind=IconKind::Upload size=14/>{t(locale, "app.status.backup_card.run")}
                            </button>
                            // Stable wrapper element so the tachys text-node
                            // walker keeps its anchor next to the sibling
                            // <ActionForm> button (see AGENTS.md footgun).
                            <span class="error-slot">
                                {move || match backup.value().get() {
                                    Some(Ok(info)) => {
                                        let label = tf(
                                            locale,
                                            "app.status.backup_card.done",
                                            &[("size", &fmt_bytes(info.bytes))],
                                        );
                                        view! { <span class="tag green">{label}</span> }.into_any()
                                    }
                                    Some(Err(e)) => view! {
                                        <span class="tag rose">{server_fn_error_text(&e)}</span>
                                    }.into_any(),
                                    None => ().into_any(),
                                }}
                            </span>
                        </ActionForm>
                    </div>
                </Card>
            </div>
        </div>
    }
}

fn render_system(s: AdminStatus) -> impl IntoView {
    let locale = use_locale();
    let (integrity_tone, integrity_label) = if s.integrity_ok {
        (ep_core::Tone::Green, t(locale, "app.status.integrity.ok"))
    } else {
        (ep_core::Tone::Rose, t(locale, "app.status.integrity.bad"))
    };
    view! {
        <div class="vstack" style="gap:0">
            <StatRow label=t(locale, "app.status.field.version") value=s.version/>
            <StatRow label=t(locale, "app.status.field.db_size") value=fmt_bytes(s.db_size_bytes)/>
            <div class="stat-row">
                <span class="stat-label">{t(locale, "app.status.field.integrity")}</span>
                <span class="stat-value"><Tag tone=integrity_tone>{integrity_label}</Tag></span>
            </div>
            <StatRow label=t(locale, "app.status.field.sessions") value=ep_core::fmt_int(s.session_count as f64)/>
            <StatRow label=t(locale, "app.status.field.notifications") value=ep_core::fmt_int(s.notification_count as f64)/>
        </div>
    }
}

fn render_backup_status(s: AdminStatus) -> impl IntoView {
    let locale = use_locale();
    let value = match (s.last_backup_exists, s.last_backup_bytes) {
        (true, Some(bytes)) => fmt_bytes(bytes),
        _ => t(locale, "app.settings.index.unconfigured").to_string(),
    };
    view! {
        <div class="vstack" style="gap:0">
            <StatRow label=t(locale, "app.status.field.last_backup") value=value/>
            {s.last_backup_path.map(|path| view! {
                <StatRow label=t(locale, "app.status.field.backup_path") value=path/>
            })}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_bytes;

    #[test]
    fn fmt_bytes_scales_units() {
        assert_eq!(fmt_bytes(0), "0 B");
        assert_eq!(fmt_bytes(512), "512 B");
        assert_eq!(fmt_bytes(1024), "1 KB");
        assert_eq!(fmt_bytes(1536), "1.5 KB");
        assert_eq!(fmt_bytes(1024 * 1024), "1 MB");
        assert_eq!(fmt_bytes(5 * 1024 * 1024 + 512 * 1024), "5.5 MB");
        assert_eq!(fmt_bytes(1024 * 1024 * 1024), "1 GB");
    }
}
