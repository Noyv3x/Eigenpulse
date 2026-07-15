use crate::{AppTimezone, NotifyBusHandle, TimezoneStore};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub cookie_key: cookie::Key,
    pub notify: NotifyBusHandle,
    pub leptos_options: leptos::config::LeptosOptions,
    pub timezone: TimezoneStore,
}

impl AppState {
    /// Capture one immutable timezone for the complete request/operation.
    pub fn timezone(&self) -> AppTimezone {
        self.timezone.snapshot()
    }
}

// axum FromRef impls so handlers can extract sub-states via State<...>
impl axum::extract::FromRef<AppState> for SqlitePool {
    fn from_ref(s: &AppState) -> Self {
        s.db.clone()
    }
}
impl axum::extract::FromRef<AppState> for cookie::Key {
    fn from_ref(s: &AppState) -> Self {
        s.cookie_key.clone()
    }
}
impl axum::extract::FromRef<AppState> for leptos::config::LeptosOptions {
    fn from_ref(s: &AppState) -> Self {
        s.leptos_options.clone()
    }
}
