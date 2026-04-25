use crate::{AppState, IconKind, NavSection};
use leptos::prelude::AnyView;
use std::future::Future;
use std::pin::Pin;

/// Cross-module link declaration (powers MOD-MTX-01 matrix).
#[derive(Clone, Debug)]
pub struct ModuleLink {
    pub source: &'static str,
    pub target: &'static str,
    pub kind: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetKind { Kpi, Card, Chart }

/// A widget injected by a module into the global Dashboard grid.
/// Render fn returns a Leptos AnyView and runs in SSR; widgets that need data
/// should perform their own DB reads inside `render`.
pub struct DashboardWidget {
    pub code: &'static str,
    pub kind: WidgetKind,
    pub render: fn(AppState) -> AnyView,
}

#[derive(Clone, Debug)]
pub struct TodayItem {
    pub time: String,
    pub state: &'static str, // "done" | "pending" | "blocked"
    pub text: String,
    pub doc_ref: String,
}

pub trait Module: Sync + 'static {
    fn code(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn name_cn(&self) -> &'static str;
    fn nav_section(&self) -> NavSection;
    fn nav_icon(&self) -> IconKind;
    fn glyph(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn version(&self) -> &'static str;

    /// `(name, sql)` pairs run idempotently via `_ep_module_migration` ledger.
    fn migrations(&self) -> &'static [(&'static str, &'static str)];

    /// Web UI / server-fn routes mounted under the global cookie session middleware.
    fn routes(&self, state: AppState) -> axum::Router<AppState>;

    /// Open-API sub-router; mounted under PAT middleware at `/api/v1/<code>`.
    fn open_api(&self, _state: AppState) -> axum::Router<AppState> { axum::Router::new() }
    fn open_api_scopes(&self) -> &'static [&'static str] { &[] }

    fn dashboard_widgets(&self) -> &'static [DashboardWidget] { &[] }

    fn today_items<'a>(
        &'a self,
        _state: &'a AppState,
        _date: time::Date,
    ) -> Pin<Box<dyn Future<Output = Vec<TodayItem>> + Send + 'a>> {
        Box::pin(async move { Vec::new() })
    }

    fn links(&self) -> &'static [ModuleLink] { &[] }
}
