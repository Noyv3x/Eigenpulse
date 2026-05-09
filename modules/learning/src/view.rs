use crate::model::*;
use crate::server_fns::*;
use ep_core::IconKind;
use ep_i18n::{server_fn_error_text, t, tf, use_locale};
use ep_ui::{Card, Direction, Heatmap, Icon, Kpi, PageHead, Ring, RowDeleteAction, Tag};
use leptos::prelude::*;

#[component]
pub fn LearningView() -> impl IntoView {
    let locale = use_locale();
    let data = Resource::new(|| (), |_| async { load_learning().await });
    let add_book = ServerAction::<AddBook>::new();
    let cycle = ServerAction::<CycleBookStatus>::new();
    let del_book = ServerAction::<DeleteBook>::new();
    let add_note = ServerAction::<AddNote>::new();
    let del_note = ServerAction::<DeleteNote>::new();

    Effect::new(move |prev: Option<()>| {
        add_book.version().get();
        cycle.version().get();
        del_book.version().get();
        add_note.version().get();
        del_note.version().get();
        if prev.is_some() {
            data.refetch();
        }
    });

    view! {
        <div class="view">
            <PageHead
                code="LRN-03"
                module=t(locale, "learning.page.module")
                title=t(locale, "learning.page.title")
                title_cn=t(locale, "learning.page.title_cn")
                sub=t(locale, "learning.page.sub")
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:200px">{t(locale, "app.common.loading")}</div> }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">{t(locale, "app.common.load_failed")} " · " {server_fn_error_text(&e)}</div></div> }.into_any(),
                    Ok(d) => render_learning(d, add_book, cycle, del_book, add_note, del_note).into_any(),
                })}
            </Suspense>
        </div>
    }
}

fn render_learning(
    d: LearningData,
    add_book: ServerAction<AddBook>,
    cycle: ServerAction<CycleBookStatus>,
    del_book: ServerAction<DeleteBook>,
    add_note: ServerAction<AddNote>,
    del_note: ServerAction<DeleteNote>,
) -> impl IntoView {
    let locale = use_locale();
    let s = d.summary.clone();
    let progress_pct = (s.courses_avg_progress * 100.0).round() as u32;
    let books_total = s.books_done + s.books_reading + s.books_todo;
    let banner_text = tf(
        locale,
        "learning.banner.courses",
        &[
            ("count", &d.courses.len().to_string()),
            ("pct", &progress_pct.to_string()),
        ],
    );
    let heatmap_total: u32 = s.note_heatmap_28d.iter().map(|c| *c as u32).sum();
    let heatmap_data = s.note_heatmap_28d;
    view! {
        <div class="module-banner">
            <div class="module-glyph lrn mono">"LRN"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "learning.banner.status")}</span>
                    <Tag tone=ep_core::Tone::Blue dot=true>{tf(locale, "learning.banner.notes", &[("count", &s.notes_30d.to_string())])}</Tag>
                </div>
                <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                    {banner_text}
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono">{tf(locale, "learning.banner.books", &[
                        ("reading", &s.books_reading.to_string()),
                        ("done", &s.books_done.to_string()),
                        ("todo", &s.books_todo.to_string()),
                        ("total", &books_total.to_string()),
                    ])}</span>
                </div>
            </div>
            <div style="text-align:center">
                <Ring pct=progress_pct size=80 thick=6 children_text=format!("{}%", progress_pct)/>
                <div class="mono dim" style="font-size:10px;margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">{t(locale, "learning.title.course_avg")}</div>
            </div>
        </div>

        <div class="kpi-grid">
            <Kpi code="LRN-K01" label=t(locale, "learning.kpi.notes") value=format!("{}", s.notes_30d)
                 unit=t(locale, "app.today.unit.entries").to_string()
                 delta=tf(locale, "learning.kpi.heat", &[("count", &heatmap_total.to_string())]) dir=Direction::Up/>
            <Kpi code="LRN-K02" label=t(locale, "learning.kpi.course_progress")
                 value=format!("{}", progress_pct) unit="%".to_string()
                 delta=tf(locale, "learning.kpi.courses", &[("count", &d.courses.len().to_string())]) dir=Direction::Flat/>
            <Kpi code="LRN-K03" label=t(locale, "learning.kpi.books_reading") value=format!("{}", s.books_reading)
                 delta=tf(locale, "learning.kpi.books_done", &[("count", &s.books_done.to_string())]) dir=Direction::Flat/>
            <Kpi code="LRN-K04" label=t(locale, "learning.kpi.books_todo") value=format!("{}", s.books_todo)
                 delta=tf(locale, "learning.kpi.total_books", &[("count", &books_total.to_string())]) dir=Direction::Flat/>
        </div>

        <Card title=t(locale, "learning.card.heat.title") code="LRN-HEAT-01" sub=t(locale, "learning.card.heat.sub")>
            <Heatmap data=heatmap_data/>
        </Card>

        <div style="margin-top:20px"></div>

        {render_body(d, add_book, cycle, del_book, add_note, del_note)}
    }
}

fn render_body(
    d: LearningData,
    add_book: ServerAction<AddBook>,
    cycle: ServerAction<CycleBookStatus>,
    del_book: ServerAction<DeleteBook>,
    add_note: ServerAction<AddNote>,
    del_note: ServerAction<DeleteNote>,
) -> impl IntoView {
    let locale = use_locale();
    view! {
        <div class="grid-2">
            <Card title=t(locale, "learning.card.books.title") code="LRN-BK-01" sub=t(locale, "learning.card.books.sub")>
                <ActionForm action=add_book attr:class="vstack" attr:style="gap:8px;margin-bottom:12px">
                    <div style="display:grid;grid-template-columns:2fr 1fr 1fr auto;gap:8px;align-items:end">
                        <input name="name" required maxlength=MAX_BOOK_NAME_CHARS.to_string()
                               placeholder=t(locale, "learning.field.book")
                               style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        <input name="author" maxlength=MAX_BOOK_AUTHOR_CHARS.to_string()
                               placeholder=t(locale, "learning.field.author")
                               style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        <select name="status" style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)">
                            <option value="todo" selected="selected">{t(locale, "learning.status.todo")}</option>
                            <option value="reading">{t(locale, "learning.status.reading")}</option>
                            <option value="done">{t(locale, "learning.status.done")}</option>
                        </select>
                        <button class="btn primary sm" type="submit">{t(locale, "learning.submit.add")}</button>
                    </div>
                    <span class="error-slot">
                        {move || add_book.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{server_fn_error_text(&e)}</span>
                        })}
                    </span>
                </ActionForm>

                <table class="tbl">
                    <thead>
                        <tr>
                            <th style="width:90px">{t(locale, "learning.field.doc")}</th>
                            <th>{t(locale, "learning.field.book")}</th>
                            <th style="width:120px">{t(locale, "learning.field.author")}</th>
                            <th style="width:90px">{t(locale, "learning.field.status")}</th>
                            <th class="num" style="width:70px">{t(locale, "learning.field.ops")}</th>
                        </tr>
                    </thead>
                    <tbody>
                        {d.books.into_iter().map(|b| {
                            let doc = b.doc_id.clone();
                            let doc2 = b.doc_id.clone();
                            let (tone, label) = match b.status.as_str() {
                                "done" => (ep_core::Tone::Green, t(locale, "learning.status.done")),
                                "reading" => (ep_core::Tone::Blue, t(locale, "learning.status.reading")),
                                _ => (ep_core::Tone::None, t(locale, "learning.status.todo")),
                            };
                            view! {
                                <tr>
                                    <td class="doc">{b.doc_id}</td>
                                    <td><span class="serif">{b.name}</span></td>
                                    <td class="dim">{b.author.unwrap_or_default()}</td>
                                    <td>
                                        <ActionForm action=cycle attr:style="display:inline">
                                            <input type="hidden" name="doc_id" value=doc/>
                                            <button class="btn sm" type="submit" title=t(locale, "learning.title.switch_status")>
                                                <Tag tone=tone>{label}</Tag>
                                            </button>
                                        </ActionForm>
                                    </td>
                                    <td class="num">
                                        <RowDeleteAction action=del_book value=doc2
                                                         confirm=t(locale, "learning.confirm.book") label="×"/>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </Card>

            <Card title=t(locale, "learning.card.notes.title") code="LRN-N-01" sub=t(locale, "learning.card.notes.sub")>
                <ActionForm action=add_note attr:class="vstack" attr:style="gap:8px;margin-bottom:12px">
                    <input name="title" required maxlength=MAX_NOTE_TITLE_CHARS.to_string()
                           placeholder=t(locale, "learning.field.title")
                           style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                    <textarea name="body" rows="2" maxlength=MAX_NOTE_BODY_CHARS.to_string()
                              placeholder=t(locale, "learning.field.body")
                              style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"></textarea>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary sm" type="submit"><Icon kind=IconKind::Plus size=12/>{t(locale, "learning.submit.add_note")}</button>
                        <span class="error-slot">
                            {move || add_note.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{server_fn_error_text(&e)}</span>
                            })}
                        </span>
                    </div>
                </ActionForm>

                <div class="vstack" style="gap:0">
                    {d.notes.into_iter().map(|n| {
                        let doc = n.doc_id.clone();
                        let when = ep_core::fmt_ts_date(Some(n.updated_at));
                        let body = n.body.clone();
                        view! {
                            <div class="list-row">
                                <div class="icon-tile mono" style="font-size:10px">{n.doc_id.split('-').next_back().unwrap_or("").to_string()}</div>
                                <div>
                                    <div class="title">{n.title}</div>
                                    {body.map(|b| view! {
                                        <div class="muted" style="font-size:12px;margin-top:2px;white-space:pre-wrap">{b}</div>
                                    })}
                                    <div class="meta mono dim">{when}</div>
                                </div>
                                <RowDeleteAction action=del_note value=doc
                                                 confirm=t(locale, "learning.confirm.note") label="×"/>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Card>
        </div>

        <div style="margin-top:24px"></div>

        <Card title=t(locale, "learning.card.courses.title") code="LRN-CRS-01" sub=t(locale, "learning.card.courses.sub")>
            <div class="vstack" style="gap:0">
                {d.courses.into_iter().map(|c| {
                    let pct = (c.progress * 100.0) as u32;
                    let bar_color = c.tone.as_deref().map(|t| format!("var(--{t})")).unwrap_or_else(|| "var(--primary)".into());
                    view! {
                        <div style="padding:12px 0;border-bottom:1px solid var(--border)">
                            <div style="display:flex;justify-content:space-between;align-items:baseline;margin-bottom:6px">
                                <div>
                                    <div style="font-size:13.5px;font-weight:500">{c.name}</div>
                                    <div class="mono dim" style="font-size:10.5px;margin-top:2px">
                                        {c.doc_id}" · "{c.provider.unwrap_or_default()}" · "{tf(locale, "learning.course.due", &[("date", &c.due_on.unwrap_or_else(|| "—".into()))])}
                                    </div>
                                </div>
                                <div class="mono" style="font-size:12px;font-weight:500">{pct}"%"</div>
                            </div>
                            <div class="bar"><span style=format!("width:{}%;background:{}", pct, bar_color)></span></div>
                        </div>
                    }
                }).collect_view()}
            </div>
        </Card>
    }
}
