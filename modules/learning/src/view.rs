use crate::model::*;
use crate::server_fns::*;
use ep_core::IconKind;
use ep_ui::{Card, Heatmap, Icon, Kpi, kpi::Direction, PageHead, Ring, RowDeleteAction, Tag};
use leptos::prelude::*;

#[component]
pub fn LearningView() -> impl IntoView {
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
        if prev.is_some() { data.refetch(); }
    });

    view! {
        <div class="view">
            <PageHead
                code="LRN-03"
                module="LEARNING · 学习管理"
                title="Learning"
                title_cn="学习管理"
                sub="课程进度 · 书籍状态 · 笔记 28 天热度。所有数字来自 lrn_* 表实时聚合。"
            />

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:200px">"loading…"</div> }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <div class="card"><div class="card-body">"加载失败 · " {e.to_string()}</div></div> }.into_any(),
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
    let s = d.summary.clone();
    let progress_pct = (s.courses_avg_progress * 100.0).round() as u32;
    let books_total = s.books_done + s.books_reading + s.books_todo;
    let banner_text = format!("{} 个课程进行中 · 平均进度 {}%", d.courses.len(), progress_pct);
    let heatmap_total: u32 = s.note_heatmap_28d.iter().map(|c| *c as u32).sum();
    let heatmap_data = s.note_heatmap_28d;
    view! {
        <div class="module-banner">
            <div class="module-glyph lrn mono">"LRN"</div>
            <div style="flex:1">
                <div class="hstack" style="margin-bottom:6px;gap:8px">
                    <span class="mono dim" style="font-size:11px;text-transform:uppercase;letter-spacing:0.06em">"学习状态 / STATUS"</span>
                    <Tag tone=ep_core::Tone::Blue dot=true>{format!("近 30 天 {} 笔记", s.notes_30d)}</Tag>
                </div>
                <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                    {banner_text}
                </div>
                <div class="hstack" style="gap:16px;margin-top:8px;font-size:12.5px;color:var(--ink-3)">
                    <span class="mono">{format!("在读 {} · 已完成 {} · 待读 {} · 共 {}",
                                              s.books_reading, s.books_done, s.books_todo, books_total)}</span>
                </div>
            </div>
            <div style="text-align:center">
                <Ring pct=progress_pct size=80 thick=6 children_text=format!("{}%", progress_pct)/>
                <div class="mono dim" style="font-size:10px;margin-top:6px;text-transform:uppercase;letter-spacing:0.06em">"课程均值"</div>
            </div>
        </div>

        <div class="kpi-grid">
            <Kpi code="LRN-K01" label="近 30 天笔记" value=format!("{}", s.notes_30d)
                 unit="条".to_string()
                 delta=format!("28 天热力 {} 条", heatmap_total) dir=Direction::Up/>
            <Kpi code="LRN-K02" label="课程进度"
                 value=format!("{}", progress_pct) unit="%".to_string()
                 delta=format!("{} 个课程", d.courses.len()) dir=Direction::Flat/>
            <Kpi code="LRN-K03" label="在读书籍" value=format!("{}", s.books_reading)
                 delta=format!("已完成 {}", s.books_done) dir=Direction::Flat/>
            <Kpi code="LRN-K04" label="待读队列" value=format!("{}", s.books_todo)
                 delta=format!("共 {} 本", books_total) dir=Direction::Flat/>
        </div>

        <Card title="笔记热度" code="LRN-HEAT-01" sub="近 28 天 · 单元格深度按当日笔记条数 (0..4) 归一化">
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
    view! {
        <div class="grid-2">
            <Card title="阅读列表" code="LRN-BK-01" sub="Books">
                <ActionForm action=add_book attr:class="vstack" attr:style="gap:8px;margin-bottom:12px">
                    <div style="display:grid;grid-template-columns:2fr 1fr 1fr auto;gap:8px;align-items:end">
                        <input name="name" required placeholder="书名"
                               style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        <input name="author" placeholder="作者"
                               style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                        <select name="status" style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)">
                            <option value="todo" selected="selected">"待读"</option>
                            <option value="reading">"阅读中"</option>
                            <option value="done">"已完成"</option>
                        </select>
                        <button class="btn primary sm" type="submit">"+ 添加"</button>
                    </div>
                    <span class="error-slot">
                        {move || add_book.value().get().and_then(|r| r.err()).map(|e| view! {
                            <span class="tag rose">{e.to_string()}</span>
                        })}
                    </span>
                </ActionForm>

                <table class="tbl">
                    <thead>
                        <tr>
                            <th style="width:90px">"单号"</th>
                            <th>"书名"</th>
                            <th style="width:120px">"作者"</th>
                            <th style="width:90px">"状态"</th>
                            <th class="num" style="width:70px">"操作"</th>
                        </tr>
                    </thead>
                    <tbody>
                        {d.books.into_iter().map(|b| {
                            let doc = b.doc_id.clone();
                            let doc2 = b.doc_id.clone();
                            let (tone, label) = match b.status.as_str() {
                                "done" => (ep_core::Tone::Green, "已完成"),
                                "reading" => (ep_core::Tone::Blue, "阅读中"),
                                _ => (ep_core::Tone::None, "待读"),
                            };
                            view! {
                                <tr>
                                    <td class="doc">{b.doc_id}</td>
                                    <td><span class="serif">{b.name}</span></td>
                                    <td class="dim">{b.author.unwrap_or_default()}</td>
                                    <td>
                                        <ActionForm action=cycle attr:style="display:inline">
                                            <input type="hidden" name="doc_id" value=doc/>
                                            <button class="btn sm" type="submit" title="点击切换状态">
                                                <Tag tone=tone>{label}</Tag>
                                            </button>
                                        </ActionForm>
                                    </td>
                                    <td class="num">
                                        <RowDeleteAction action=del_book value=doc2
                                                         confirm="删除该书？" label="×"/>
                                    </td>
                                </tr>
                            }
                        }).collect_view()}
                    </tbody>
                </table>
            </Card>

            <Card title="笔记" code="LRN-N-01" sub="Notes">
                <ActionForm action=add_note attr:class="vstack" attr:style="gap:8px;margin-bottom:12px">
                    <input name="title" required placeholder="标题"
                           style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2)"/>
                    <textarea name="body" rows="2" placeholder="正文（可选）"
                              style="padding:6px 10px;border:1px solid var(--border);border-radius:6px;background:var(--bg-2);font-family:var(--font-mono);font-size:12px"></textarea>
                    <div class="hstack" style="gap:8px">
                        <button class="btn primary sm" type="submit"><Icon kind=IconKind::Plus size=12/>"添加笔记"</button>
                        <span class="error-slot">
                            {move || add_note.value().get().and_then(|r| r.err()).map(|e| view! {
                                <span class="tag rose">{e.to_string()}</span>
                            })}
                        </span>
                    </div>
                </ActionForm>

                <div class="vstack" style="gap:0">
                    {d.notes.into_iter().map(|n| {
                        let doc = n.doc_id.clone();
                        let when = ep_core::fmt_ts_date(Some(n.updated_at));
                        view! {
                            <div class="list-row">
                                <div class="icon-tile mono" style="font-size:10px">{n.doc_id.split('-').next_back().unwrap_or("").to_string()}</div>
                                <div>
                                    <div class="title">{n.title}</div>
                                    <div class="meta mono dim">{when}</div>
                                </div>
                                <RowDeleteAction action=del_note value=doc
                                                 confirm="删除该笔记？" label="×"/>
                            </div>
                        }
                    }).collect_view()}
                </div>
            </Card>
        </div>

        <div style="margin-top:24px"></div>

        <Card title="进行中的课程" code="LRN-CRS-01" sub="只读 · 后续接入 Coursera/Anki 同步">
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
                                        {c.doc_id}" · "{c.provider.unwrap_or_default()}" · 截止 "{c.due_on.unwrap_or_else(|| "—".into())}
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
