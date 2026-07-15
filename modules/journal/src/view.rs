use crate::model::{JournalAnalytics, JournalData, JournalEntry, JournalEntryListItem};
use crate::server_fns::*;
use ep_i18n::{server_fn_error_text, t, use_locale};
use ep_ui::{
    AxisChart, AxisSeries, CalendarHeatmapChart, Card, Chart, ChartDatum, ChartHeight, ChartSpec,
    ChartTone, ChartValue, ErrorSlot, Field, HorizontalBarChart, LoadError, PageHead,
    RowDeleteAction, SkeletonCard,
};
use leptos::prelude::*;
use std::time::Duration;

#[component]
pub fn JournalView() -> impl IntoView {
    let locale = use_locale();
    let search_value = RwSignal::new(String::new());
    let query = RwSignal::new(String::new());
    let include_archived = RwSignal::new(false);
    let month_range = RwSignal::new(12_usize);
    let previous_year = RwSignal::new(false);
    let create = ServerAction::<CreateJournalEntry>::new();
    let update = ServerAction::<UpdateJournalEntry>::new();
    let archive = ServerAction::<ArchiveJournalEntry>::new();
    let delete = ServerAction::<DeleteJournalEntry>::new();
    let mut update_query = debounce(Duration::from_millis(300), move |value| query.set(value));

    let data = Resource::new(
        move || {
            (
                query.get(),
                include_archived.get(),
                create.version().get(),
                update.version().get(),
                archive.version().get(),
                delete.version().get(),
            )
        },
        |(query, include_archived, ..)| async move { load_journal(query, include_archived, 0).await },
    );
    let analytics = Resource::new(
        move || {
            (
                include_archived.get(),
                create.version().get(),
                update.version().get(),
                archive.version().get(),
                delete.version().get(),
            )
        },
        |(include_archived, ..)| async move { load_journal_analytics(include_archived).await },
    );

    view! {
        <div class="view" data-testid="journal-view">
            <PageHead
                module=t(locale, "journal.module.name")
                title=t(locale, "journal.page.title")
                title_cn=t(locale, "journal.page.title_cn")
                sub=t(locale, "journal.module.description")
            />

            <div class="grid-2">
                <Card
                    title=t(locale, "journal.create.title")
                    sub=t(locale, "journal.create.sub")
                >
                    <Suspense fallback=move || view! { <SkeletonCard rows=4/> }>
                        {move || data.get().map(|result| match result {
                            Ok(payload) => render_create_form(payload.today, create).into_any(),
                            Err(error) => view! {
                                <LoadError detail=server_fn_error_text(&error)/>
                            }.into_any(),
                        })}
                    </Suspense>
                </Card>

                <Card
                    title=t(locale, "journal.search.title")
                    sub=t(locale, "journal.search.sub")
                >
                    <div class="form-grid">
                        <Field label=t(locale, "journal.field.search") wide=true>
                            <input
                                class="ep-input"
                                type="search"
                                maxlength="200"
                                placeholder=t(locale, "journal.search.placeholder")
                                prop:value=move || search_value.get()
                                on:input=move |event| {
                                    let value = event_target_value(&event);
                                    search_value.set(value.clone());
                                    update_query(value);
                                }
                            />
                        </Field>
                        <Field label=t(locale, "journal.field.archive_filter") wide=true>
                            <label class="hstack" style="gap:8px">
                                <input
                                    type="checkbox"
                                    prop:checked=move || include_archived.get()
                                    on:change=move |event| {
                                        include_archived.set(event_target_checked(&event));
                                    }
                                />
                                <span>{t(locale, "journal.filter.include_archived")}</span>
                            </label>
                        </Field>
                    </div>
                </Card>
            </div>

            <div data-testid="journal-analytics">
                <Suspense fallback=move || view! {
                    <div class="vstack" style="gap:20px">
                        <SkeletonCard rows=5/>
                        <div class="grid-2">
                            <SkeletonCard rows=5/>
                            <SkeletonCard rows=5/>
                        </div>
                    </div>
                }>
                    {move || analytics.get().map(|result| match result {
                        Ok(payload) => render_analytics(payload, month_range, previous_year).into_any(),
                        Err(error) => view! {
                            <LoadError detail=server_fn_error_text(&error)/>
                        }.into_any(),
                    })}
                </Suspense>
            </div>

            <Suspense fallback=move || view! { <SkeletonCard rows=6/> }>
                {move || data.get().map(|result| match result {
                    Ok(payload) => render_entry_list(
                        payload,
                        query,
                        include_archived,
                        update,
                        archive,
                        delete,
                    ).into_any(),
                    Err(error) => view! {
                        <LoadError detail=server_fn_error_text(&error)/>
                    }.into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_analytics(
    analytics: JournalAnalytics,
    month_range: RwSignal<usize>,
    previous_year: RwSignal<bool>,
) -> impl IntoView {
    let locale = use_locale();
    let current_year = analytics.current_year;
    let prior_year = analytics.previous_year;
    let months = StoredValue::new(analytics.months);
    let days = StoredValue::new(analytics.days);
    let tags = StoredValue::new(analytics.tags);

    let monthly_spec = Signal::derive(move || {
        let months = months.read_value();
        let start = months.len().saturating_sub(month_range.get());
        let visible = &months[start..];
        ChartSpec::Axis(AxisChart {
            categories: visible.iter().map(|month| month.period.clone()).collect(),
            series: vec![AxisSeries::bar(
                t(locale, "journal.chart.monthly.series"),
                visible
                    .iter()
                    .map(|month| {
                        Some(ChartValue::new(
                            month.entries as f64,
                            month.entries.to_string(),
                        ))
                    })
                    .collect(),
            )
            .with_tone(ChartTone::Primary)],
            y_label: None,
            stacked: false,
        })
    });
    let calendar_spec = Signal::derive(move || {
        let year = if previous_year.get() {
            prior_year
        } else {
            current_year
        };
        let prefix = format!("{year:04}-");
        let points = days
            .read_value()
            .iter()
            .filter(|day| day.entry_date.starts_with(&prefix))
            .map(|day| {
                ChartDatum::new(
                    day.entry_date.clone(),
                    ChartValue::new(day.entries as f64, day.entries.to_string()),
                )
                .with_tone(ChartTone::Positive)
            })
            .collect::<Vec<_>>();
        let max = points
            .iter()
            .map(|point| point.value.value)
            .reduce(f64::max)
            .unwrap_or(1.0)
            .max(1.0);
        ChartSpec::CalendarHeatmap(CalendarHeatmapChart {
            year,
            points,
            min: Some(0.0),
            max: Some(max),
        })
    });
    let tag_spec = Signal::derive(move || {
        ChartSpec::HorizontalBar(HorizontalBarChart {
            items: tags
                .read_value()
                .iter()
                .enumerate()
                .map(|(index, tag)| {
                    let label = if tag.is_other {
                        t(locale, "journal.chart.tags.other").to_string()
                    } else {
                        tag.name.clone()
                    };
                    let tone = if tag.is_other {
                        ChartTone::Neutral
                    } else if index < 3 {
                        ChartTone::Warning
                    } else {
                        ChartTone::Primary
                    };
                    ChartDatum::new(
                        label,
                        ChartValue::new(tag.entries as f64, tag.entries.to_string()),
                    )
                    .with_tone(tone)
                })
                .collect(),
            max: None,
        })
    });

    view! {
        <div class="vstack" style="gap:20px">
            <Card
                title=t(locale, "journal.chart.monthly.title")
                sub=t(locale, "journal.chart.monthly.sub")
            >
                <div
                    class="hstack"
                    role="group"
                    aria-label=t(locale, "journal.chart.monthly.title")
                    style="justify-content:flex-end;gap:6px;margin-bottom:8px;flex-wrap:wrap"
                >
                    {[
                        (3_usize, "journal.chart.range_3"),
                        (6_usize, "journal.chart.range_6"),
                        (12_usize, "journal.chart.range_12"),
                    ].into_iter().map(|(range, key)| view! {
                        <button
                            type="button"
                            data-testid=format!("journal-range-{range}")
                            class=move || if month_range.get() == range { "btn primary" } else { "btn" }
                            aria-pressed=move || (month_range.get() == range).to_string()
                            on:click=move |_| month_range.set(range)
                        >
                            {t(locale, key)}
                        </button>
                    }).collect_view()}
                </div>
                <Chart
                    label=t(locale, "journal.chart.entries_monthly")
                    spec=monthly_spec
                    height=ChartHeight::Standard
                />
            </Card>

            <div class="grid-2">
                <Card
                    title=t(locale, "journal.chart.calendar.title")
                    sub=t(locale, "journal.chart.calendar.sub")
                >
                    <div
                        class="hstack"
                        role="group"
                        aria-label=t(locale, "journal.chart.calendar.title")
                        style="justify-content:flex-end;gap:6px;margin-bottom:8px;flex-wrap:wrap"
                    >
                        <button
                            type="button"
                            data-testid="journal-year-current"
                            class=move || if previous_year.get() { "btn" } else { "btn primary" }
                            aria-pressed=move || (!previous_year.get()).to_string()
                            on:click=move |_| previous_year.set(false)
                        >
                            {format!("{current_year} · {}", t(locale, "journal.chart.current_year"))}
                        </button>
                        <button
                            type="button"
                            data-testid="journal-year-previous"
                            class=move || if previous_year.get() { "btn primary" } else { "btn" }
                            aria-pressed=move || previous_year.get().to_string()
                            on:click=move |_| previous_year.set(true)
                        >
                            {format!("{prior_year} · {}", t(locale, "journal.chart.previous_year"))}
                        </button>
                    </div>
                    <Chart
                        label=t(locale, "journal.chart.calendar.title")
                        spec=calendar_spec
                        height=ChartHeight::Tall
                    />
                </Card>

                <Card
                    title=t(locale, "journal.chart.tags.title")
                    sub=t(locale, "journal.chart.tags.sub")
                >
                    <Chart
                        label=t(locale, "journal.chart.tags.title")
                        spec=tag_spec
                        height=ChartHeight::Tall
                    />
                </Card>
            </div>
        </div>
    }
}

fn render_create_form(today: String, create: ServerAction<CreateJournalEntry>) -> impl IntoView {
    let locale = use_locale();
    view! {
        <ActionForm action=create attr:data-testid="journal-create-form">
            <div class="form-grid">
                <Field label=t(locale, "journal.field.title") wide=true>
                    <input class="ep-input" name="title" required maxlength="200"/>
                </Field>
                <Field label=t(locale, "journal.field.date")>
                    <input class="ep-input" type="date" name="entry_date" required value=today/>
                </Field>
                <Field label=t(locale, "journal.field.mood")>
                    <input class="ep-input" name="mood" maxlength="40"/>
                </Field>
                <Field label=t(locale, "journal.field.tags") wide=true hint=t(locale, "journal.field.tags_hint")>
                    <input class="ep-input" name="tags" maxlength="1000"/>
                </Field>
                <Field label=t(locale, "journal.field.body") wide=true>
                    <textarea class="ep-textarea" name="body" rows="10" maxlength="100000"></textarea>
                </Field>
            </div>
            <div class="hstack" style="gap:10px;margin-top:12px">
                <button
                    class="btn primary"
                    type="submit"
                    disabled=move || create.pending().get()
                    aria-busy=move || create.pending().get().to_string()
                >
                    {t(locale, "journal.action.create")}
                </button>
                <ErrorSlot action=create/>
            </div>
        </ActionForm>
    }
}

fn render_entry_list(
    data: JournalData,
    query: RwSignal<String>,
    include_archived: RwSignal<bool>,
    update: ServerAction<UpdateJournalEntry>,
    archive: ServerAction<ArchiveJournalEntry>,
    delete: ServerAction<DeleteJournalEntry>,
) -> impl IntoView {
    let locale = use_locale();
    let entries = RwSignal::new(data.entries);
    let next_offset = RwSignal::new(data.next_offset);
    let loading_more = RwSignal::new(false);
    let load_more_error = RwSignal::new(None::<String>);

    let load_more = move |_| {
        let Some(offset) = next_offset.get_untracked() else {
            return;
        };
        if loading_more.get_untracked() {
            return;
        }
        loading_more.set(true);
        load_more_error.set(None);
        let query = query.get_untracked();
        let include_archived = include_archived.get_untracked();
        leptos::task::spawn_local(async move {
            match load_journal(query, include_archived, offset).await {
                Ok(page) => {
                    entries.update(|loaded| loaded.extend(page.entries));
                    next_offset.set(page.next_offset);
                }
                Err(error) => load_more_error.set(Some(server_fn_error_text(&error))),
            }
            loading_more.set(false);
        });
    };

    view! {
        <Card title=t(locale, "journal.list.title")>
            <p class="muted" style="margin:0 0 12px">
                {move || format!("{} {}", entries.read().len(), t(locale, "journal.list.count"))}
            </p>
            <div data-testid="journal-entry-list">
                {move || {
                    let loaded = entries.get();
                    if loaded.is_empty() {
                        view! { <div class="empty-state">{t(locale, "journal.list.empty")}</div> }.into_any()
                    } else {
                        view! {
                            <div class="vstack" style="gap:14px">
                                {loaded.into_iter().map(|entry| {
                                    render_entry(entry, update, archive, delete)
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
            <div class="hstack" style="margin-top:14px;gap:10px">
                {move || next_offset.get().map(|_| view! {
                    <button
                        class="btn"
                        type="button"
                        on:click=load_more
                        disabled=move || loading_more.get()
                        aria-busy=move || loading_more.get().to_string()
                    >
                        {t(locale, "journal.action.load_more")}
                    </button>
                })}
                {move || load_more_error.get().map(|detail| view! { <LoadError detail/> })}
            </div>
            <div class="vstack" style="margin-top:10px">
                <ErrorSlot action=update/>
                <ErrorSlot action=archive/>
                <ErrorSlot action=delete/>
            </div>
        </Card>
    }
}

fn render_entry(
    entry: JournalEntryListItem,
    update: ServerAction<UpdateJournalEntry>,
    archive: ServerAction<ArchiveJournalEntry>,
    delete: ServerAction<DeleteJournalEntry>,
) -> impl IntoView {
    let locale = use_locale();
    let id = entry.id;
    let archived = entry.archived_at.is_some();
    let metadata = match (&entry.mood, entry.tags.is_empty()) {
        (Some(mood), false) => format!("{mood} · {}", entry.tags),
        (Some(mood), true) => mood.clone(),
        (None, false) => entry.tags.clone(),
        (None, true) => String::new(),
    };
    let load_entry = ServerAction::<LoadJournalEntryForEdit>::new();
    let request_entry = move |_| {
        if !matches!(load_entry.value().get_untracked(), Some(Ok(_)))
            && !load_entry.pending().get_untracked()
        {
            load_entry.dispatch(LoadJournalEntryForEdit { id });
        }
    };
    view! {
        <article
            data-testid=format!("journal-entry-{id}")
            style="padding:16px;border:1px solid var(--border);border-radius:12px"
        >
            <div class="hstack" style="justify-content:space-between;gap:12px;align-items:flex-start">
                <div>
                    <div class="hstack" style="gap:8px;flex-wrap:wrap">
                        <strong>{entry.title.clone()}</strong>
                        {archived.then(|| view! {
                            <span class="tag">{t(locale, "journal.status.archived")}</span>
                        })}
                    </div>
                    <div class="mono dim" style="margin-top:4px">{entry.entry_date.clone()}</div>
                </div>
                <div class="hstack" style="gap:6px;flex-wrap:wrap;justify-content:flex-end">
                    <ActionForm action=archive>
                        <input type="hidden" name="id" value=id/>
                        <input type="hidden" name="archived" value=(!archived).to_string()/>
                        <button
                            class="btn sm"
                            type="submit"
                            disabled=move || archive.pending().get()
                            aria-busy=move || archive.pending().get().to_string()
                        >
                            {if archived { t(locale, "journal.action.restore") } else { t(locale, "journal.action.archive") }}
                        </button>
                    </ActionForm>
                    <RowDeleteAction
                        action=delete
                        value=id.to_string()
                        label=t(locale, "journal.action.delete")
                        confirm=t(locale, "journal.delete.confirm")
                    />
                </div>
            </div>
            {(!metadata.is_empty()).then(|| view! {
                <p class="muted" style="margin:8px 0 0">{metadata}</p>
            })}
            {(!entry.body_preview.is_empty()).then(|| view! {
                <p style="white-space:pre-wrap;margin:12px 0 0">
                    {entry.body_preview}
                    {entry.body_truncated.then_some("…")}
                </p>
            })}

            <details style="margin-top:14px">
                <summary class="btn sm" on:click=request_entry>
                    {t(locale, "journal.action.edit")}
                </summary>
                <div style="margin-top:12px">
                    {move || match load_entry.value().get() {
                        Some(Ok(entry)) => render_edit_form(entry, update).into_any(),
                        Some(Err(error)) => view! {
                            <LoadError detail=server_fn_error_text(&error)/>
                        }.into_any(),
                        None => view! {
                            <p class="muted" hidden=move || !load_entry.pending().get()>
                                {t(locale, "journal.list.loading_entry")}
                            </p>
                        }.into_any(),
                    }}
                </div>
            </details>
        </article>
    }
}

fn render_edit_form(
    entry: JournalEntry,
    update: ServerAction<UpdateJournalEntry>,
) -> impl IntoView {
    let locale = use_locale();
    let id = entry.id;
    let mood = entry.mood.unwrap_or_default();
    view! {
        <ActionForm action=update>
            <input type="hidden" name="id" value=id/>
            <div class="form-grid">
                <Field label=t(locale, "journal.field.title") wide=true>
                    <input class="ep-input" name="title" required maxlength="200" value=entry.title/>
                </Field>
                <Field label=t(locale, "journal.field.date")>
                    <input class="ep-input" type="date" name="entry_date" required value=entry.entry_date/>
                </Field>
                <Field label=t(locale, "journal.field.mood")>
                    <input class="ep-input" name="mood" maxlength="40" value=mood/>
                </Field>
                <Field label=t(locale, "journal.field.tags") wide=true>
                    <input class="ep-input" name="tags" maxlength="1000" value=entry.tags/>
                </Field>
                <Field label=t(locale, "journal.field.body") wide=true>
                    <textarea class="ep-textarea" name="body" rows="8" maxlength="100000">{entry.body}</textarea>
                </Field>
            </div>
            <button
                class="btn primary"
                type="submit"
                disabled=move || update.pending().get()
                aria-busy=move || update.pending().get().to_string()
            >
                {t(locale, "journal.action.save")}
            </button>
        </ActionForm>
    }
}
