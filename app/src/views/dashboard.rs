use ep_core::{fmt_int, IconKind};
// `fmt_ts_hm` and `server_err` are consumed only inside the
// `#[cfg(feature = "ssr")]` body of `load_dashboard`; importing them at
// module scope would warn on the wasm32 hydrate target where the body is
// replaced by an `ssr-only` stub.
#[cfg(feature = "ssr")]
use ep_core::{fmt_ts_hm, server_err};
use ep_ui::{Card, Icon, Kpi, kpi::Direction, PageHead, SectionLabel, Tag};
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
        let budget_pct = if budget_total > 0.0 { (expense / budget_total * 100.0).round() as u32 } else { 0 };
        let budget_remain = (budget_total - expense).max(0.0);

        let rows: Vec<(i64, String, String, String, Option<String>, Option<f64>)> = sqlx::query_as(
            "SELECT occurred_at, module, doc_id, summary, link_doc, amount
               FROM activity ORDER BY occurred_at DESC LIMIT 12"
        ).fetch_all(pool).await.map_err(server_err)?;
        let recent = rows.into_iter().map(|r| ActivityRow {
            time: fmt_ts_hm(Some(r.0)),
            module: r.1, doc_id: r.2, summary: r.3, link_doc: r.4, amount: r.5,
        }).collect();

        Ok(DashboardData { today_count_done: 2, today_count_total: 8, savings, budget_pct, budget_remain, recent })
    }
    #[cfg(not(feature = "ssr"))]
    { Err(ServerFnError::ServerError("ssr-only".into())) }
}

#[component]
pub fn DashboardView() -> impl IntoView {
    let r = Resource::new(|| (), |_| async { load_dashboard().await });
    view! {
        <div class="view">
            <PageHead
                code="DSH-01".to_string()
                module="DASHBOARD · 全局视图".to_string()
                title="Hello, Leo".to_string()
                title_cn="早上好"
                sub="今天是 2026 年 4 月 25 日 · 周六。您有 6 项待办，3 个模块有新更新。"
                actions=view! {
                    <>
                        <button class="btn"><Icon kind=IconKind::Export size=14/>"导出周报"</button>
                        <button class="btn primary"><Icon kind=IconKind::Plus size=14/>"新增记录"</button>
                    </>
                }.into_any()
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">"loading…"</div> }>
                {move || r.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">"加载失败 · " {e.to_string()}</div></div> }.into_any(),
                    Ok(d) => render_body(d).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_body(d: DashboardData) -> impl IntoView {
    let recent = d.recent.clone();
    view! {
        <div class="kpi-grid">
            <Kpi code="FIN-K01" label="月度结余" value=format!("¥{}", fmt_int(d.savings))
                 delta="+18.4% vs 上月".to_string() dir=Direction::Up/>
            <Kpi code="FIN-K02" label="预算使用率" value=format!("{}", d.budget_pct) unit="%".to_string()
                 delta=format!("¥{} 剩余", fmt_int(d.budget_remain)) dir=Direction::Flat/>
            <Kpi code="FIT-K01" label="本周训练" value="5/6".to_string()
                 delta="连续 14 天".to_string() dir=Direction::Up/>
            <Kpi code="FIT-K02" label="静息心率" value="58".to_string() unit="bpm".to_string()
                 delta="-3 vs 4 周均值".to_string() dir=Direction::Down/>
            <Kpi code="LRN-K01" label="本周学习" value="12.4".to_string() unit="h".to_string()
                 delta="目标 14h · 88%".to_string() dir=Direction::Up/>
            <Kpi code="SLP-K01" label="平均睡眠" value="7.4".to_string() unit="h".to_string()
                 delta="+0.4 vs 上周".to_string() dir=Direction::Up/>
        </div>

        <SectionLabel index="§ 02".to_string()>"活动流 · Activity Journal"</SectionLabel>

        <Card>
            <div class="scroll-x">
                <table class="tbl">
                    <thead>
                        <tr>
                            <th style="width:110px">"时间"</th>
                            <th style="width:90px">"模块"</th>
                            <th style="width:130px">"单号"</th>
                            <th>"描述"</th>
                            <th style="width:120px">"关联"</th>
                            <th class="num" style="width:140px">"数值 / 状态"</th>
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

