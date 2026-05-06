use ep_core::{fmt_int, IconKind};
// `fmt_ts_hm` and `server_err` are consumed only inside the
// `#[cfg(feature = "ssr")]` body of `load_dashboard`; importing them at
// module scope would warn on the wasm32 hydrate target where the body is
// replaced by an `ssr-only` stub.
#[cfg(feature = "ssr")]
use ep_core::{fmt_ts_hm, server_err};
use ep_i18n::{t, use_locale};
use ep_ui::{kpi::Direction, Card, Icon, Kpi, PageHead, SectionLabel, Tag};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DashboardData {
    pub today_count_done: u32,
    pub today_count_total: u32,
    pub savings: f64,
    pub budget_pct: u32,
    pub budget_remain: f64,
    pub recent: Vec<ActivityRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityRow {
    pub time: String,
    pub module: String,
    pub doc_id: String,
    pub summary: String,
    pub link_doc: Option<String>,
    pub amount: Option<f64>,
}

#[server(LoadDashboard, "/api/_internal/dsh", "Url", "load_dashboard")]
pub async fn load_dashboard() -> Result<DashboardData, ServerFnError> {
    #[cfg(feature = "ssr")]
    {
        ep_auth::require_user_for_server_fn().await?;
        let state: ep_core::AppState = expect_context();
        let pool = &state.db;
        let income: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_txn WHERE amount > 0 AND tag='inc' AND occurred_at >= unixepoch('now','start of month')"
        ).fetch_one(pool).await.map_err(server_err)?;
        let expense: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(-amount), 0.0) FROM fin_txn WHERE tag = 'exp' AND occurred_at >= unixepoch('now','start of month')"
        ).fetch_one(pool).await.map_err(server_err)?;
        let savings = income - expense;
        let budget_total: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount), 0.0) FROM fin_budget WHERE period = strftime('%Y-%m','now')"
        ).fetch_one(pool).await.map_err(server_err)?;
        let budget_pct = if budget_total > 0.0 {
            (expense / budget_total * 100.0).round() as u32
        } else {
            0
        };
        let budget_remain = (budget_total - expense).max(0.0);

        let rows: Vec<(i64, String, String, String, Option<String>, Option<f64>)> = sqlx::query_as(
            "SELECT occurred_at, module, doc_id, summary, link_doc, amount
               FROM activity ORDER BY occurred_at DESC LIMIT 12",
        )
        .fetch_all(pool)
        .await
        .map_err(server_err)?;
        let recent = rows
            .into_iter()
            .map(|r| ActivityRow {
                time: fmt_ts_hm(Some(r.0)),
                module: r.1,
                doc_id: r.2,
                summary: r.3,
                link_doc: r.4,
                amount: r.5,
            })
            .collect();

        Ok(DashboardData {
            today_count_done: 2,
            today_count_total: 8,
            savings,
            budget_pct,
            budget_remain,
            recent,
        })
    }
    #[cfg(not(feature = "ssr"))]
    {
        Err(ServerFnError::ServerError("ssr-only".into()))
    }
}

#[component]
pub fn DashboardView() -> impl IntoView {
    let r = Resource::new(|| (), |_| async { load_dashboard().await });
    let locale = use_locale();
    view! {
        <div class="view">
            <PageHead
                code="DSH-01"
                module=t(locale, "app.dashboard.page.module")
                title="Hello, Leo"
                title_cn=t(locale, "app.dashboard.page.title_cn")
                sub=t(locale, "app.dashboard.page.subtitle")
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Export size=14/>{t(locale, "app.dashboard.btn.export_weekly")}</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>{t(locale, "app.dashboard.btn.new_record")}</button>
                    </>
                }.into_any()
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">{t(locale, "app.common.loading")}</div> }>
                {move || r.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {e.to_string()}</div></div> }.into_any(),
                    Ok(d) => render_body(d).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_body(d: DashboardData) -> impl IntoView {
    let locale = use_locale();
    let recent = d.recent.clone();
    view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label=t(locale, "app.dashboard.kpi.monthly_savings") value=format!("¥{}", fmt_int(d.savings))
                 delta=t(locale, "app.dashboard.kpi.savings_delta") dir=Direction::Up/>
            <Kpi code="FIN-K02" label=t(locale, "app.dashboard.kpi.budget_usage") value=format!("{}", d.budget_pct) unit="%"
                 delta=format!("¥{} {}", fmt_int(d.budget_remain), t(locale, "app.dashboard.kpi.budget_remain_suffix")) dir=Direction::Flat/>
            <Kpi code="FIT-K01" label=t(locale, "app.dashboard.kpi.weekly_workouts") value="5/6"
                 delta=t(locale, "app.dashboard.kpi.workout_delta") dir=Direction::Up/>
            <Kpi code="FIT-K02" label=t(locale, "app.dashboard.kpi.resting_heart_rate") value="58" unit="bpm"
                 delta=t(locale, "app.dashboard.kpi.heart_rate_delta") dir=Direction::Down/>
            <Kpi code="LRN-K01" label=t(locale, "app.dashboard.kpi.weekly_learning") value="12.4" unit="h"
                 delta=t(locale, "app.dashboard.kpi.learning_delta") dir=Direction::Up/>
            <Kpi code="SLP-K01" label=t(locale, "app.dashboard.kpi.avg_sleep") value="7.4" unit="h"
                 delta=t(locale, "app.dashboard.kpi.sleep_delta") dir=Direction::Up/>
        </div>

        <SectionLabel index="§ 02".to_string()>{t(locale, "app.dashboard.activity.title")}</SectionLabel>

        <Card>
            <div class="scroll-x">
                <table class="tbl">
                    <thead>
                        <tr>
                            <th style="width:110px">{t(locale, "app.dashboard.table.time")}</th>
                            <th style="width:90px">{t(locale, "app.dashboard.table.module")}</th>
                            <th style="width:130px">{t(locale, "app.dashboard.table.doc")}</th>
                            <th>{t(locale, "app.dashboard.table.description")}</th>
                            <th style="width:120px">{t(locale, "app.dashboard.table.link")}</th>
                            <th class="num" style="width:140px">{t(locale, "app.dashboard.table.amount_status")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {recent.into_iter().map(|r| {
                            let tone = match r.module.as_str() {
                                "FIN" => ep_core::Tone::Amber,
                                "FIT" => ep_core::Tone::Green,
                                "LRN" => ep_core::Tone::Blue,
                                _ => ep_core::Tone::None,
                            };
                            let amt = match r.amount {
                                Some(v) if v >= 0.0 => view! { <td class="num amt-pos">{format!("+¥{}", fmt_int(v))}</td> }.into_any(),
                                Some(v) => view! { <td class="num amt-neg">{format!("−¥{}", fmt_int(v.abs()))}</td> }.into_any(),
                                None => view! { <td class="num dim">"—"</td> }.into_any(),
                            };
                            view! {
                                <tr>
                                    <td class="mono dim">{r.time}</td>
                                    <td><Tag tone=tone>{r.module.clone()}</Tag></td>
                                    <td class="doc">{r.doc_id.clone()}</td>
                                    <td>{r.summary.clone()}</td>
                                    <td class="mono dim">
                                        {match r.link_doc {
                                            Some(l) => view! { <span><Icon kind=IconKind::Link size=11/>" "{l}</span> }.into_any(),
                                            None => view! { <span>"—"</span> }.into_any(),
                                        }}
                                    </td>
                                    {amt}
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </div>
        </Card>
    }
}
