use crate::model::*;
use crate::server_fns::*;
use ep_core::IconKind;
use ep_ui::{Card, Icon, Kpi, kpi::Direction, PageHead, Ring, Tag};
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
                sub="课程、书籍、笔记与 Anki 复习。以每周 14 小时为基准。"
            />

            <div class="module-banner">
                <div class="module-glyph lrn mono">"LRN"</div>
                <div style="flex:1">
                    <div class="hstack" style="margin-bottom:6px;gap:8px">
                        <span class="mono" style="font-size:11px;color:var(--ink-3);text-transform:uppercase;letter-spacing:0.06em">"本周学习 / WEEKLY STUDY"</span>
                        <Tag tone=ep_core::Tone::Blue dot=true>"进行中"</Tag>
                    </div>
                    <div style="font-size:22px;font-weight:600;letter-spacing:-0.01em">
                        "12.4 " <span class="mono dim" style="font-size:14px;font-weight:500">"/ 14 小时"</span>
                    </div>
                </div>
                <div style="text-align:center">
                    <Ring pct=89 size=80 thick=6 children_text="12.4h".to_string()/>
                </div>
            </div>

            <div class="kpi-grid">
                <Kpi code="LRN-K01" label="本周时长"   value="12.4".to_string() unit="h".to_string() delta="目标 14h · 89%".to_string() dir=Direction::Up/>
                <Kpi code="LRN-K02" label="待复习卡片" value="60".to_string()                       delta="-18 vs 昨日".to_string()    dir=Direction::Down/>
                <Kpi code="LRN-K03" label="笔记总数"   value="221".to_string()                      delta="+3 本周".to_string()        dir=Direction::Up/>
                <Kpi code="LRN-K04" label="专注时段"   value="2h 40m".to_string()                   delta="平均 · 日".to_string()      dir=Direction::Flat/>
            </div>

            <Suspense fallback=move || view! { <div class="placeholder-img" style="min-height:160px">"loading…"</div> }>
                {move || data.get().map(|res| match res {
                    Err(e) => view! { <p>"加载失败 · " {e.to_string()}</p> }.into_any(),
                    Ok(d) => render_body(d, add_book, cycle, del_book, add_note, del_note).into_any(),
                })}
            </Suspense>
        </div>
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
                                        <span class="row-actions-slot">
                                            <ActionForm action=del_book attr:style="display:inline">
                                                <input type="hidden" name="doc_id" value=doc2/>
                                                <button class="btn sm" type="submit"
                                                        style="color:var(--rose-ink)"
                                                        onclick="return confirm('删除？')">"×"</button>
                                            </ActionForm>
                                        </span>
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
                                <span class="row-actions-slot">
                                    <ActionForm action=del_note attr:style="display:inline">
                                        <input type="hidden" name="doc_id" value=doc/>
                                        <button class="btn sm" type="submit"
                                                style="color:var(--rose-ink)"
                                                onclick="return confirm('删除？')">"×"</button>
                                    </ActionForm>
                                </span>
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
