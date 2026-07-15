use ep_core::{ModuleDescriptor, ModuleSummary, ModuleSummaryState};
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{
    Card, Chart, ChartDatum, ChartHeight, ChartSpec, ChartTone, ChartValue, Icon, LoadError,
    PageHead, SparklineChart, Tag,
};
use leptos::prelude::*;
use leptos::server_fn::ServerFnError;
use leptos_router::components::A;

#[component]
pub fn DashboardView() -> impl IntoView {
    let locale = use_locale();
    view! {
        <div class="view hub-home">
            <PageHead
                module=t(locale, "app.dashboard.page.module")
                title=t(locale, "app.dashboard.page.title")
                title_cn=t(locale, "app.dashboard.page.title_cn")
                sub=t(locale, "app.dashboard.page.subtitle")
            />
            <div class="hub-grid">
                {crate::modules::MODULES.iter().map(|module| {
                    let summary = Resource::new(|| (), move |_| module.load_summary());
                    view! { <ModuleCard descriptor=module.descriptor summary/> }
                }).collect_view()}
            </div>
        </div>
    }
}

#[component]
fn ModuleCard(
    descriptor: &'static ModuleDescriptor,
    summary: Resource<Result<ModuleSummary, ServerFnError>>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
        <Card class="hub-module-card">
            <div class="hub-module-head">
                <span class="hub-module-icon"><Icon kind=descriptor.icon size=22/></span>
                <div>
                    <h2>{t(locale, descriptor.name_key)}</h2>
                    <p class="muted">{t(locale, descriptor.description_key)}</p>
                </div>
            </div>
            <Suspense fallback=move || view! {
                <div class="hub-summary-grid">
                    {(0..3).map(|_| view! { <span class="skeleton-line" style="height:42px;display:block"></span> }).collect_view()}
                </div>
            }>
                {move || summary.get().map(|result| match result {
                    Ok(summary) => render_summary(summary).into_any(),
                    Err(error) => view! { <LoadError detail=server_fn_error_text(&error)/> }.into_any(),
                })}
            </Suspense>
            <div class="hub-module-foot">
                <A href=descriptor.route attr:class="btn primary">
                    {t(locale, "app.dashboard.open_module")}
                    <Icon kind=ep_core::IconKind::Arrow size=14/>
                </A>
            </div>
        </Card>
    }
}

fn render_summary(summary: ModuleSummary) -> impl IntoView {
    let locale = use_locale();
    let tone = match summary.state {
        ModuleSummaryState::Active => ep_core::Tone::Green,
        ModuleSummaryState::Unavailable => ep_core::Tone::Rose,
        ModuleSummaryState::Ready => ep_core::Tone::Blue,
        ModuleSummaryState::Empty => ep_core::Tone::None,
    };
    let state_key = match summary.state {
        ModuleSummaryState::Active => "app.dashboard.state.active",
        ModuleSummaryState::Unavailable => "app.dashboard.state.unavailable",
        ModuleSummaryState::Ready => "app.dashboard.state.ready",
        ModuleSummaryState::Empty => "app.dashboard.state.empty",
    };
    let trend = summary.trend.map(|trend| {
        let label = t(locale, &trend.label_key).to_string();
        let tone = match summary.slug.as_str() {
            "finance" => ChartTone::Positive,
            "fitness" => ChartTone::Primary,
            "journal" => ChartTone::Warning,
            _ => ChartTone::Neutral,
        };
        let spec = ChartSpec::Sparkline(SparklineChart {
            points: trend
                .points
                .into_iter()
                .map(|point| {
                    ChartDatum::new(
                        point.label,
                        ChartValue::new(f64::from(point.position), point.display),
                    )
                    .with_tone(tone)
                })
                .collect(),
            tone,
            area: true,
        });
        let spec = Signal::derive(move || spec.clone());
        view! {
            <div class="hub-summary-trend" style="margin-top:14px">
                <Chart label=label spec=spec height=ChartHeight::Compact/>
            </div>
        }
    });
    view! {
        <div>
            <div style="margin-bottom:12px"><Tag tone=tone>{t(locale, state_key)}</Tag></div>
            <div class="hub-summary-grid">
                {summary.metrics.into_iter().map(|metric| {
                    let detail = metric.detail.map(|detail| {
                        if detail.contains('.') { t(locale, &detail).to_string() } else { detail }
                    });
                    view! {
                        <div class="hub-summary-item">
                            <span class="muted">{t(locale, &metric.label_key)}</span>
                            <strong>{metric.value}</strong>
                            {detail.map(|detail| view! { <small class="dim">{detail}</small> })}
                        </div>
                    }
                }).collect_view()}
            </div>
            {trend}
        </div>
    }
}
