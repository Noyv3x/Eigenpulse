# i18n key 清单（Task #2 产出, by architect）

每节末尾标"覆盖率"。Phase D 实施时 frontend-dev/backend-dev 按这份清单一个文件一个文件搬，搬完后用 `grep -nE '"[一-鿿][^"]*"'` 跑一遍验证零 hit。

> **范围**：仅 UI/系统 chrome。**不**翻译用户填入的数据（账户名、分类名、商户名、笔记标题等）。

---

## 1. crates/core/src/nav.rs

namespace = `core.nav.section.*`

| 行 | 中文 | key | en |
|---|---|---|---|
| 13 | `"核心 · CORE"` | `core.nav.section.core` | `Core · CORE` |
| 14 | `"模块 · MODULES"` | `core.nav.section.modules` | `Modules · MODULES` |
| 15 | `"系统 · SYSTEM"` | `core.nav.section.system` | `System · SYSTEM` |

需要把 `pub fn label(&self) -> &'static str` 改成接 i18n。两种实现：
- 删 `label()`，让 view 端调 `t!(i18n, core.nav.section.core)`。
- 保留 `label()` 返回 key，view 端 `td_string_dyn!`。

推荐删，view 端按 enum match。

---

## 2. crates/ui/src/sidebar.rs

namespace = `core.nav.*` + `ui.sidebar.*`

**v3 决断**（architect §A）：删 dual-title `name`/`name_cn` 双字段模式，改为单一 `name_key` 字段，资源里直接存完整双语串。zh 资源含「Dashboard · 全局仪表」整串，en 资源含「Dashboard」单串。view 调用方零分支。

`NavItem` 改造：

```rust
pub struct NavItem {
    pub code: &'static str,        // "DSH"
    pub name_key: &'static str,    // "core.nav.dsh.name"
    pub icon: IconKind,
    pub section: NavSection,
    pub path: &'static str,
}
```

view 渲染：

```rust
<span class="nav-label">{ep_i18n::t(locale, item.name_key)}</span>
```

8 个 NAV item 的 `name_key`（共 8 keys，对比 v1 的 16 keys 减半）：

| code | name_key | zh-CN value | en value |
|---|---|---|---|
| DSH | `core.nav.dsh.name` | `Dashboard · 全局仪表` | `Dashboard` |
| TDY | `core.nav.tdy.name` | `Today · 今日聚焦` | `Today` |
| FIN | `core.nav.fin.name` | `Finance · 财务管理` | `Finance` |
| FIT | `core.nav.fit.name` | `Fitness · 健身管理` | `Fitness` |
| LRN | `core.nav.lrn.name` | `Learning · 学习管理` | `Learning` |
| MOD | `core.nav.mod.name` | `Modules · 模块市场` | `Modules` |
| RPT | `core.nav.rpt.name` | `Reports · 报表中心` | `Reports` |
| CFG | `core.nav.cfg.name` | `Settings · 系统设置` | `Settings` |

同样的：`PageHead` 组件删 `title_cn` prop（共 ~8 处调用方），所有 `<PageHead title="Finance" title_cn="财务管理"/>` 改成 `<PageHead title=t!(locale, fin.page.title)/>`。资源里 zh: `"Finance · 财务管理"`, en: `"Finance"`。

其他文案（sidebar bottom）：

| 行 | 中文 | key |
|---|---|---|
| 78 (`OWNER · UID-0001`) | — | 不翻，UID 是 ID |
| 80 | `"退出登录"` | `ui.sidebar.logout_title` |

`Personal ERP · v0.1` (37) 也建议保留，是 brand 标语。

---

## 3. crates/ui/src/topbar.rs

namespace = `ui.topbar.*`

| 行 | 中文 | key | en |
|---|---|---|---|
| 24 | `"折叠侧栏"` | `ui.topbar.toggle_sidebar` | `Toggle sidebar` |
| 42 | `"搜索模块、单号或记录…"` | `ui.topbar.search_placeholder` | `Search modules, doc IDs, records…` |
| 47 | `"切到浅色"` / `"切到深色"` | `ui.topbar.theme_toggle.to_light` / `.to_dark` | `Switch to light` / `Switch to dark` |
| 58 | `"通知"` | `ui.topbar.notif_title` | `Notifications` |
| 63 | `"帮助"` | `ui.topbar.help_title` | `Help` |

新增：

| key | zh-CN | en |
|---|---|---|
| `ui.topbar.lang_toggle.to_en` | (按钮显示"中"，hover 提示"切到英文") | (按钮显示"EN"，hover 提示"Switch to 中文") |

---

## 4. app/src/app.rs

namespace = `app.notfound.*`

| 行 | 中文 | key | en |
|---|---|---|---|
| 57 | `"404"` | — | 不翻 |
| 58 | `"页面未找到 · "` | `app.notfound.title` | `Page not found · ` |
| 58 | `"返回首页"` | `app.notfound.home_link` | `Back to home` |

---

## 5. app/src/login.rs

namespace = `app.login.*` —— **Phase B 必做**（team-lead 修正 #3，architect review gap，已确认）。

| 行 | 中文 | key | en |
|---|---|---|---|
| 26 | `密码错误，请重试` | `app.login.err_wrong_password` | `Wrong password, try again` |
| 33 | `登录 · Eigenpulse` | `app.login.title` | `Sign in · Eigenpulse` |
| 44 | `Personal ERP · 登录` | `app.login.subtitle` | `Personal ERP · Sign in` |
| 49 | `密码 · PASSWORD` | `app.login.password_label` | `Password · PASSWORD` |
| 52 | `登录 · LOGIN` | `app.login.submit_btn` | `Sign in · LOGIN` |
| 53 | `单用户系统 · 密码由 EP_ADMIN_PASSWORD 环境变量在首次启动时设定。` | `app.login.help_text` | `Single-user system · Password set via EP_ADMIN_PASSWORD on first boot.` |

落地法（v3）：login.rs 是 axum handler，不是 Leptos 组件。从 `axum::Extension<Locale>` 提取（locale_layer middleware 已注入），调普通函数 `ep_i18n::t(locale, "app.login.title")` 渲染。`<html lang>` 也要从硬编码 `zh-CN` 改成 `locale.as_html_lang()`。

---

## 6. app/src/views/dashboard.rs

namespace = `app.dashboard.*`

| 行 | 中文 | key |
|---|---|---|
| 75 | `DASHBOARD · 全局视图` | `app.dashboard.page.module` |
| 77 | `早上好` | `app.dashboard.page.title_cn` (静态；动态时段问候等 v2 再做) |
| 78 | `今天是 2026 年 4 月 25 日 · 周六。您有 6 项待办，3 个模块有新更新。` | `app.dashboard.page.subtitle` (含变量插值 — 留 v2 真接入计算) |
| 82 | `导出周报` | `app.dashboard.btn.export_weekly` |
| 83 | `新增记录` | `app.dashboard.btn.new_record` |
| 87 | `加载中…` (placeholder) | `app.common.loading` |
| 89 | `加载失败 · ` | `app.common.load_failed` |
| 101 | `月度结余` | `app.dashboard.kpi.monthly_savings` |
| 102 | `+18.4% vs 上月` | `app.dashboard.kpi.savings_delta` (硬编码占位 — 后续真算) |
| 103 | `预算使用率` | `app.dashboard.kpi.budget_usage` |
| 104 | `¥{n} 剩余` | `app.dashboard.kpi.budget_remain` (插值) |
| 105 | `本周训练` | `app.dashboard.kpi.weekly_workouts` |
| 106 | `连续 14 天` | `app.dashboard.kpi.streak_14d` (写死先翻) |
| 107 | `静息心率` | `app.dashboard.kpi.resting_hr` |
| 108 | `-3 vs 4 周均值` | `app.dashboard.kpi.hr_delta` (写死) |
| 109 | `本周学习` | `app.dashboard.kpi.weekly_learning` |
| 110 | `目标 14h · 88%` | `app.dashboard.kpi.learning_goal` (写死) |
| 111 | `平均睡眠` | `app.dashboard.kpi.avg_sleep` |
| 112 | `+0.4 vs 上周` | `app.dashboard.kpi.sleep_delta` (写死) |
| 115 | `活动流 · Activity Journal` | `app.dashboard.section.activity` |
| 122 | `时间` | `app.dashboard.col.time` |
| 123 | `模块` | `app.dashboard.col.module` |
| 124 | `单号` | `app.dashboard.col.doc_id` |
| 125 | `描述` | `app.dashboard.col.summary` |
| 126 | `关联` | `app.dashboard.col.link` |
| 127 | `数值 / 状态` | `app.dashboard.col.value_status` |

---

## 7. app/src/views/today.rs

namespace = `app.today.*`

| 行 | 中文 | key |
|---|---|---|
| 108 | `今日 · {date}` | `app.today.page.title_cn` (插值) |
| 123 | `TODAY · 今日聚焦` | `app.today.page.module` |
| 126 | `来自各模块的真实事件流 · 按时间排序 · 0:00 起算` | `app.today.page.subtitle` |
| 130 | `今日事件` | `app.today.kpi.event_count` |
| 131 | `条` | `app.today.unit.entries` (`entries` in en) |
| 132 | `跨模块累计` | `app.today.kpi.cross_module_total` |
| 133 | `今日支出` | `app.today.kpi.spent` |
| 134 | `FIN 自动累计` | `app.today.kpi.fin_auto` |
| 135 | `今日训练` | `app.today.kpi.workouts` |
| 136 | `次` | `app.today.unit.times` |
| 137 | `FIT 自动累计` | `app.today.kpi.fit_auto` |
| 138 | `今日学习` | `app.today.kpi.learning` |
| 140 | `LRN 自动累计` | `app.today.kpi.lrn_auto` |
| 143 | `今日时间线` | `app.today.card.timeline.title` |
| 144 | `尚无事件 · 在任一模块创建一条记录即可填充` | `app.today.empty_sub` |
| 145 | `{n} 条事件 · 点击跳转源模块` | `app.today.timeline_sub` (插值) |
| 147 | `今日还没有事件。去 Finance / Fitness / Learning 任一模块创建一条记录就会出现在这里。` | `app.today.empty_body` |

---

## 8. app/src/views/reports.rs

namespace = `app.reports.*`

| 行 | 中文 | key |
|---|---|---|
| 157 | `REPORTS · 报表中心` | `app.reports.page.module` |
| 159 | `报表中心` | `app.reports.page.title_cn` |
| 160 | `基于 fin_txn / fin_account 的实时聚合 · 12 月趋势 · 类别 · 账户` | `app.reports.page.subtitle` |
| 162 | `loading…` | `app.common.loading` |
| 164 | `加载失败 · ` | `app.common.load_failed` |
| 176 | `本年收入` | `app.reports.kpi.year_income` |
| 177 | `YTD` | — (英文术语，不翻) |
| 178 | `本年支出` | `app.reports.kpi.year_expense` |
| 180 | `储蓄率` | `app.reports.kpi.savings_rate` |
| 183 | `¥{n} 净结余` | `app.reports.kpi.savings_amount` (插值) |
| 185 | `账户数` | `app.reports.kpi.account_count` |
| 186 | `覆盖 {n} 月数据` | `app.reports.kpi.month_coverage` (插值) |
| 211 | `月度趋势` | `app.reports.card.month_trend.title` |
| 212-213 | `{n} 月 · 本月 ¥{net} 净结余 · 收入 ¥{in} / 支出 ¥{out}` | `app.reports.card.month_trend.sub` (4 插值) |
| 217 | `收入 · INCOME` | `app.reports.row_label.income` |
| 220 | `支出 · EXPENSE` | `app.reports.row_label.expense` |
| 224 | `净结余 · NET (绿=盈余 / 玫=透支)` | `app.reports.row_label.net` |
| 233 | `近 30 天 · 共 ¥{n}` | `app.reports.card.cat_share.sub` (插值) |
| 238 | `类别分布` | `app.reports.card.cat_share.title` |
| 240 | `近 30 天还没有支出数据 · 在 Finance 记一笔以填充。` | `app.reports.empty.cat_share` |
| 286 | `账户健康` | `app.reports.card.acc_health.title` |
| 287 | `总资产 ¥{n} · {m} 账户` | `app.reports.card.acc_health.sub` (2 插值) |
| 300 | `占总资产 {n}%` | `app.reports.acc_pct` (插值) |
| 315 | `Ring 占比按非负余额归一化` | `app.reports.acc_health_footer` |

---

## 9. app/src/views/notifications.rs

namespace = `app.notifications.*`

| 行 | 中文 | key |
|---|---|---|
| 49 | `NOTIFICATIONS · 通知中心` | `app.notifications.page.module` |
| 51 | `通知中心` | `app.notifications.page.title_cn` |
| 54 | `loading…` | `app.common.loading` |
| 56 | `加载失败 · ` | `app.common.load_failed` |
| 57 | `暂无通知` | `app.notifications.empty` |

---

## 10. app/src/views/settings/mod.rs

namespace = `app.settings.index.*`

| 行 | 中文 | key |
|---|---|---|
| 20 | `SETTINGS · 系统设置` | `app.settings.index.page.module` |
| 22 | `系统设置` | `app.settings.index.page.title_cn` |
| 25 | `账户 & 身份` | `app.settings.index.card.account.title` |
| 27 | `用户` | `app.settings.index.row.user` |
| 28 | `角色` | `app.settings.index.row.role` |
| 29 | `数据条数` | `app.settings.index.row.data_count` |
| 32 | `数据 & 同步` | `app.settings.index.card.data.title` |
| 34 | `本地存储` | `app.settings.index.row.local_storage` |
| 35 | `上次备份` | `app.settings.index.row.last_backup` |
| 35 | `尚未配置` | `app.settings.index.value.unconfigured` |
| 36 | `同步状态` | `app.settings.index.row.sync_status` |
| 36 | `本地` | `app.settings.index.value.local` |
| 39 | `通知 · 通道` | `app.settings.index.card.notify.title` |
| 40 | `在 ` / ` 中配置外部推送。` | `app.settings.index.card.notify.body_pre` / `.body_post` (拆两段) |
| 40 | `通道管理` | `app.settings.index.card.notify.link_label` |
| 42 | `API · Personal Access Tokens` | `app.settings.index.card.api.title` |
| 42 | `/api/v1/* 端点鉴权` | `app.settings.index.card.api.sub` |
| 43 | `在 ` / ` 中生成 PAT。` | `app.settings.index.card.api.body_pre` / `.body_post` |
| 43 | `安全管理` | `app.settings.index.card.api.link_label` |
| 45 | `外观 · 显示密度` | `app.settings.index.card.tweaks.title` |
| 45 | `主题切换在右上角图标` | `app.settings.index.card.tweaks.sub` |
| 47 | `密度 · DENSITY` | `app.settings.index.card.tweaks.density_label` |
| 52 | `宽松` | `app.settings.index.card.tweaks.density_comfortable` |
| 56 | `紧凑` | `app.settings.index.card.tweaks.density_compact` |

新增：

- `app.settings.index.card.lang.title` = "语言 · LANGUAGE" / "Language · LANGUAGE"
- 也可以放 Topbar，不在 settings 卡（见方案 §5）

---

## 11. app/src/views/settings/security.rs (CFG-SEC)

namespace = `app.settings.security.*`

key 计数：约 42 条。涵盖：

- `*.page.{module,title,title_cn,subtitle}`
- `*.card.password.{title,sub}` + 三个输入 label + 提交按钮 + 成功/失败提示文字
- `*.card.new_token.{title}` + 输入 label / 占位符 / scope 标签 / 一次性 token 警示
- `*.card.token_list.{title}` + 表头 7 列 + 状态 tag (`已撤销` / `已过期` / `有效` / `永久`) + 撤销按钮 + 撤销确认对话框

完整列表 frontend-dev 按"一行一 key"展开（建议同行对应同 key），格式参见前文。

特别注意：

- L162-167 `ALL_SCOPES` 数组里的 `("fin:read", "FIN · 只读")` 中文标签 → key = `app.settings.security.scope.fin_read` 等。
- L193 `修改登录密码，管理 /api/v1/* 的 Personal Access Tokens。` → 整段一个 key。
- L283 `✓ Token · 仅展示一次，请妥善保存` → `app.settings.security.new_token.warning`。
- L319-322 `已撤销` / `已过期` / `有效` → 状态 tag，key 用 `app.settings.security.status.{revoked,expired,active}`。
- L326 `永久` → `app.settings.security.expires.never`。

---

## 12. app/src/views/settings/notifications.rs (CFG-NOT)

namespace = `app.settings.notifications.*`

key 计数：约 28。涵盖：

- 页面 chrome、`新增通道` 卡（包括 select option 文案 `SMTP · 邮件` 等 5 条）
- `已配置通道` 卡（5 列表头 + ON/OFF + 测试 / 删除按钮 + 各种 confirm 对话框）
- 测试结果 toast (L198-199 `✓ 测试通道发送成功` / `✕ 测试失败 · `)
- 错误码：`{kind} 通道测试失败 · 详细错误已记录到服务器日志` (server_fns.rs:108) → 保留 `kind` 是大写英文，前缀本地化

具体 key 命名沿用 `app.settings.notifications.{section}.{field}`，frontend-dev 按文件展开。

---

## 13. modules/finance/src/view.rs

namespace = `finance.*` —— 168 条，按子区段分。

### 13.1 PageHead + banner

| 行 | 中文 | key |
|---|---|---|
| 56 | `FINANCE · 财务管理` | `finance.page.module` |
| 58 | `财务管理` | `finance.page.title_cn` |
| 59 | `账户、预算、收支、投资。支持跨模块关联与自动分类。` | `finance.page.subtitle` |
| 63 | `记一笔` | `finance.page.action_record` |
| 69 | `加载失败 · ` | `app.common.load_failed` (复用 app namespace) |
| 224 | `健康` / 226 `持平` / 228 `关注` | `finance.banner.tone.{healthy,flat,warn}` |
| 235 | `净资产 / NET WORTH` | `finance.banner.label.net_worth` |
| 242 | `本周` (`{sign}¥{n} 本周`) | `finance.banner.this_week` (插值) |
| 243 | `储蓄率 {n}%` | `finance.banner.savings_rate` (插值) |
| 244 | `{n} 账户` | `finance.banner.account_count` (插值) |
| 249 | `月收入` | `finance.banner.month_income` |
| 254 | `月支出` | `finance.banner.month_expense` |
| 259 | `月结余` | `finance.banner.month_savings` |

### 13.2 KPI 卡（4 条）

| 行 | 中文 | key |
|---|---|---|
| 163 | `本月预算` | `finance.kpi.monthly_budget` |
| 166 | `日均支出` | `finance.kpi.daily_avg` |
| 170 | `储蓄率` | `finance.kpi.savings_rate` |
| 174 | `应急金` | `finance.kpi.emergency_fund` |
| 175 | `月` | `finance.unit.month` |
| 147 | `{sign}¥{n} vs 90d 均值` | `finance.kpi.daily_delta` (插值) |
| 149 | `第 {n} 天 · 90d 数据不足` | `finance.kpi.daily_delta_thin` (插值) |
| 152 | `¥{n} 净结余` | `finance.kpi.savings_amount` (插值) |
| 154 | `−¥{n} 透支` | `finance.kpi.savings_overdraft` (插值) |
| 157 | `¥{liq} 流动 / ¥{avg} 月均` | `finance.kpi.emergency_meta` (2 插值) |
| 159 | `数据不足 · 至少需 1 月支出` | `finance.kpi.emergency_thin` |

### 13.3 Tabs

| 行 | 中文 | key |
|---|---|---|
| 182 | `总账 / Ledger` | `finance.tab.ledger` |
| 183 | `预算 / Budget` | `finance.tab.budget` |
| 184 | `账户 / Accounts` | `finance.tab.accounts` |
| 185 | `类别 / Categories` | `finance.tab.categories` |
| 186 | `报表 / Reports` | `finance.tab.reports` |

### 13.4 记一笔表单

| 行 | 中文 | key |
|---|---|---|
| 277 | `记一笔` (Card title) | `finance.card.new_txn.title` |
| 277 | `新建交易 · 自动生成 FIN-NNNNN 单号 · 标签自动定号位` | `finance.card.new_txn.sub` |
| 281 | `商户 / 描述` | `finance.form.merchant` |
| 283 | `盒马 · 生鲜` (placeholder) | `finance.form.merchant_placeholder` |
| 286 | `金额 (¥)` | `finance.form.amount` |
| 291 | `标签` | `finance.form.tag` |
| 293 | `支出 · exp` | `finance.form.tag_exp` |
| 294 | `收入 · inc` | `finance.form.tag_inc` |
| 300 | `类别` | `finance.form.category` |
| 310 | `账户` | `finance.form.account` |
| 320 | `关联单号 (可选)` | `finance.form.linked_doc` |
| 327 | `备注 (可选)` | `finance.form.note` |
| 331 | `日期 (可选 · 默认今天)` | `finance.form.date` |
| 333 | `留空 = 当前时间` | `finance.form.date_title` |
| 341 | `记录` | `finance.btn.record` |

### 13.5 转账表单

| 行 | 中文 | key |
|---|---|---|
| 358 | `转账` (Card title) | `finance.card.transfer.title` |
| 358 | `账户间互转 · 自动生成两笔配对记录` | `finance.card.transfer.sub` |
| 362 | `从账户` | `finance.form.from_account` |
| 372 | `到账户` | `finance.form.to_account` |
| 387 | `日期 (可选)` | `finance.form.date_optional` |
| 395 | `月初分配 · 储蓄` (placeholder) | `finance.form.transfer_note_placeholder` |
| 403 | `转账` (按钮) | `finance.btn.transfer` |

### 13.6 总账表（交易明细）

| 行 | 中文 | key |
|---|---|---|
| 440 | `暂无交易 · 在上方记一笔填充` | `finance.ledger.empty` |
| 441 | `展示最近 50 笔 · 本月已记录 {n} 笔 · 支持商户搜索 / 类别 / 日期筛选` | `finance.ledger.sub_50` (插值) |
| 442 | `共 {v} 笔（全部）· 本月 {m} 笔 · 支持商户搜索 / 类别 / 日期筛选` | `finance.ledger.sub` (插值) |
| 447 | `交易明细` | `finance.card.ledger.title` |
| 449 | `搜索商户 / 描述…` | `finance.ledger.search_placeholder` |
| 456 | `全部类别` | `finance.ledger.filter.all_categories` |
| 467 | `起始日期` | `finance.ledger.filter.date_from` |
| 472 | `结束日期` | `finance.ledger.filter.date_to` |
| 474 | `导出` | `finance.btn.export` |
| 481-488 | `日期` `单号` `商户 / 描述` `类别` `账户` `金额` `关联` `操作` | `finance.col.{date,doc_id,merchant,category,account,amount,link,actions}` |
| 520 | `支出结构` | `finance.card.cat_sum.title` |
| 520 | `本月 · 按类别` | `finance.card.cat_sum.sub` |
| 522 | `本月暂无支出 · 在左侧记一笔填充` | `finance.card.cat_sum.empty` |
| 547 | `智能建议` | `finance.card.suggestions.title` |
| 547 | `基于本月预算 + 近 30 天交易 · 规则驱动` | `finance.card.suggestions.sub` |
| 559 | `未识别可行动建议 · 数据健康` | `finance.card.suggestions.empty` |
| 622 | `转账` (Tag in tfr row) | `finance.tag.transfer` |
| 640 | `转账记录不可编辑，请删除后重建` | `finance.row.tfr_edit_disabled_title` |
| 642 | `删除该转账？两笔配对记录会同时回滚，余额同步恢复。` | `finance.confirm.delete_tfr` |
| 648 | `编辑` | `finance.btn.edit` |
| 651 | `删除该笔交易？账户余额会同步回滚。` | `finance.confirm.delete_txn` |
| 679-720 | 编辑表单字段（重用 `finance.form.*` 同名 key） | (复用) |
| 722 | `保存` | `finance.btn.save` |
| 724 | `取消` | `finance.btn.cancel` |

### 13.7 预算编辑（budget tab）

| 行 | 中文 | key |
|---|---|---|
| 803 | `预算池 · {period}` | `finance.card.budget_pool.title` (插值) |
| 805 | `本期尚未设置预算` | `finance.card.budget_pool.sub_empty` |
| 807 | `{n} 个类别 · 已用 ¥{used} / ¥{total}` | `finance.card.budget_pool.sub` (3 插值) |
| 809 | `从 {period} 导入` | `finance.btn.import_budgets_from` (插值) |
| 810 | `{period} 期间未设置任何预算 · 通过右侧编辑器添加，或一键复制上期。` | `finance.card.budget_pool.empty_hint` (插值) |
| 811 | `基于本月支出节奏推算 · 建议 {period} 期` | `finance.card.next_month_plan.sub` (插值) |
| 874 | `未预算的类别 · 已发生支出` | `finance.budget.unbudgeted_label` |
| 890 | `编辑预算` | `finance.card.budget_edit.title` |
| 891 | `选择期间 + 类别 · 金额 0 视为删除条目` | `finance.card.budget_edit.sub` |
| 895 | `期间` | `finance.form.period` |
| 916 | `保存` | (复用 `finance.btn.save`) |
| 927 | `下月规划` | `finance.card.next_month_plan.title` |
| 929 | `近 3 个月支出数据不足 · 至少需 1 笔记录方能给出规划` | `finance.card.next_month_plan.empty` |
| 945 | `建议金额 = 近 3 月该类别支出 ÷ 3 · 取整到 50` | `finance.card.next_month_plan.formula` |

### 13.8 账户管理（accounts tab）

涵盖 L1011-1219，常见 keys：

| 行 | 中文 | key |
|---|---|---|
| 1018 | `已归档 {n} 个账户` | `finance.acc.archived_count` (插值) |
| 1020 | `账户管理` | `finance.card.acc_mgr.title` |
| 1020 | `新建 / 编辑 / 归档 · 余额由交易自动维护` | `finance.card.acc_mgr.sub` |
| 1023 | `新建账户` | `finance.acc.btn.new` |
| 1029 | `code` (label) | (英文术语，不翻) |
| 1030 | `2..=16 字符，仅大写字母 / 数字 / 连字符` (input title) | `finance.acc.code_title` |
| 1035 | `名称` | `finance.form.name` |
| 1036 | `招行储蓄` (placeholder) | `finance.acc.name_placeholder` |
| 1040 | `类型` | `finance.form.type` |
| 1048 | `色调` | `finance.form.tone` |
| 1057 | `开户余额` | `finance.acc.opening_balance` |
| 1064 | `创建` | `finance.btn.create` |
| 1095 | `取消归档账户 {code}？` | `finance.confirm.unarchive_account` (插值) |
| 1110 | `取消归档` | `finance.btn.unarchive` |
| 1125 | `最近活动 {date}` | `finance.acc.last_seen` (插值) |
| 1126 | `尚无活动` | `finance.acc.no_activity` |
| 1128 | `取消归档` / `归档` | `finance.btn.{unarchive,archive}` |
| 1142 | `{verb} 账户 {code}？余额保留，历史交易仍可查询。` | `finance.confirm.archive_account` (2 插值) |
| 1163 | `编辑` | (复用 `finance.btn.edit`) |
| 1168 | `code (不可改)` | `finance.acc.code_locked` |
| 1170 | `账户 code 不可修改` (title) | `finance.acc.code_locked_title` |
| 1199 | `保存` | (复用) |

### 13.9 类别管理（categories tab）

L1221-1403。同样模板：

- `分类管理` → `finance.card.cat_mgr.title`
- `新建 / 编辑 / 归档 · 不可硬删（保留 FK 引用）` → `finance.card.cat_mgr.sub`
- `新建分类` → `finance.cat.btn.new`
- `教育` (placeholder) → `finance.cat.name_placeholder`
- `1..=8 字符，仅大写字母 / &` (input title) → `finance.cat.code_title`
- `顺序` → `finance.form.sort_order`
- `创建` → (复用)
- 表头列 `名称` `code` `色调` `顺序` `在用` `归档` `操作` → `finance.col.{name,code,tone,sort,usage,archived,actions}`
- `{verb} 分类 {code}？` → `finance.confirm.archive_category` (2 插值)
- `分类 code 不可修改` → `finance.cat.code_locked_title`

### 13.10 finance reports tab（页内子标签）

L1405-1523。复用 reports namespace 的几个 key（月度趋势、收入/支出 row label），独有的：

- `暂无可聚合数据 · 至少需要一笔交易` → `finance.reports.empty`
- `月度趋势` → `finance.card.month_trend.title`
- `本月尚无支出数据` → `finance.card.cat_share.empty`

---

## 14. modules/finance/src/server_fns.rs

namespace = `finance.err.*`（Phase D 错误码改造）

错误码改造对照（详见方案 §8）：

| 原中文 | 新错误码 | i18n key |
|---|---|---|
| `日期格式应为 YYYY-MM-DD,收到 '{s}'` | `finance.date_format` | `finance.err.date_format` |
| `交易 '{doc_id}' 不存在` | `finance.txn_not_found` | `finance.err.txn_not_found` |
| `转账记录不可编辑,请删除后重建` | `finance.tfr_not_editable` | `finance.err.tfr_not_editable` |
| `from_account / to_account 都必填` | `finance.transfer_accounts_required` | `finance.err.transfer_accounts_required` |
| `转出与转入账户不能相同` | `finance.transfer_same_account` | `finance.err.transfer_same_account` |
| `amount 必须是正数` | `finance.amount_must_be_positive` | (同前) |
| `分类 TFR 不存在或已归档；请到分类管理新建/取消归档` | `finance.tfr_category_missing` | (同前) |
| `code 必须 2..=16 字符,且只允许大写字母/数字/连字符` | `finance.acc_code_format` | … |
| `code 只允许大写字母/数字/连字符` | `finance.acc_code_charset` | … |
| `name 必填且长度不超过 64 字符` | `finance.name_format_64` | … |
| `type 必须是 {:?} 之一` | `finance.acc_type_invalid` | (1 插值 = `{types}`) |
| `tone 必须为空或 {:?} 之一` | `finance.tone_invalid` | (1 插值) |
| `opening_balance 必须为有限数` | `finance.opening_balance_finite` | … |
| `账户 code '{code}' 已存在` | `finance.acc_code_taken` | (1 插值) |
| `账户 '{code}' 不存在` | `finance.acc_not_found` | (1 插值) |
| `code 必须 1..=8 字符,只允许大写字母和 '&'` | `finance.cat_code_format` | … |
| `name 必填且长度不超过 32 字符` | `finance.name_format_32` | … |
| `分类 code '{code}' 已存在` | `finance.cat_code_taken` | (1 插值) |
| `分类 '{code}' 不存在` | `finance.cat_not_found` | (1 插值) |
| `大额支出 · {merchant}` (notify warn 标题) | `finance.notify.big_expense_title` | (1 插值) |

英文已存在的 `merchant is required` / `amount must be a positive number` 等保持成 key（对应 `finance.err.merchant_required` 等），en.json 直接拷贝。

---

## 15. modules/fitness/src/view.rs

namespace = `fitness.*`，54 条。骨架：

- `FITNESS · 健身管理` → `fitness.page.module`
- `健身管理` → `fitness.page.title_cn`
- `训练计划、动作库、恢复指标。与饮食、睡眠、财务装备互联。` → `fitness.page.subtitle`
- 4 个 KPI label + delta 文案
- `训练状态 / STATUS` → `fitness.banner.label`
- `本周 {n}/{m}` → `fitness.banner.weekly` (2 插值)
- `近 12 周累计 {n} 分钟` (2 插值)
- `平均 {n} min/次`
- `连续 {n} 天` / `本周有氧 {n}min` / `近 7 天最重 · {label}`
- 强度 `高强度` / `中强度` / `轻量` → `fitness.strain.{h,m,l}`
- `近 7 天无记录` → `fitness.empty.no_recent`
- `周训练` → `fitness.banner.weekly_progress`
- 4 KPI labels: `本周总负荷` / `本周有氧` / `平均时长 · 30 天` / `近 7 天最重`
- `周负荷趋势` (Card)
- 表单：`类型` / `力量 · 推日` / `计划` / `时长 (min)` / `负荷 / 距离` / `7,840kg 或 5km` / `强度`
- `身体 · 今日` (Card) + `Wearable · 占位 · 待接入`
- 6 行 placeholder: `睡眠时长` / `深睡比例` / `步数` / `热量消耗` / `压力指数` / `体重` / `占位`
- `训练记录` (Card) + `共 {n} 次 · 近 30 次`
- `还没有训练记录。先用左侧表单记一次。`
- 表头：`日期` `单号` `类型` `计划` `时长` `负荷` `强度` `操作`
- `删除该训练？` (confirm)
- `记录` (按钮)

---

## 16. modules/learning/src/view.rs

namespace = `learning.*`，43 条。骨架：

- `LEARNING · 学习管理` / `学习管理` / `课程进度 · 书籍状态 · 笔记 28 天热度。所有数字来自 lrn_* 表实时聚合。`
- `学习状态 / STATUS` / `近 30 天 {n} 笔记` / `{n} 个课程进行中 · 平均进度 {pct}%`
- `在读 {r} · 已完成 {d} · 待读 {t} · 共 {tot}`
- `课程均值`
- 4 KPI labels: `近 30 天笔记` / `课程进度` / `在读书籍` / `待读队列`
- `28 天热力 {n} 条` / `{n} 个课程` / `已完成 {n}` / `共 {n} 本`
- `条`
- `笔记热度` (Card) + `近 28 天 · 单元格深度按当日笔记条数 (0..4) 归一化`
- `阅读列表` / `Books`
- `书名` / `作者` / `待读` / `阅读中` / `已完成` (form select)
- `+ 添加`
- 表头：`单号` `书名` `作者` `状态` `操作`
- `删除该书？`
- `笔记` / `Notes`
- `标题` / `正文（可选）`
- `添加笔记`
- `删除该笔记？`
- `进行中的课程` (Card) + `只读 · 后续接入 Coursera/Anki 同步`
- `截止 {date}`
- 状态 tag：`待读` / `阅读中` / `已完成` → `learning.book_status.{todo,reading,done}`

---

## 17. modules/mod_marketplace/src/view.rs

namespace = `marketplace.*`，35 条。骨架：

- `MODULES · 模块市场` / `模块市场` / `系统由模块构成。每个模块是独立的数据域与界面，可随时启用、停用或扩展。`
- `开发文档` / `创建自定义模块`
- `架构 / ARCHITECTURE` / `你的生活操作系统` / `· 可无限扩展`
- `每个模块遵循统一的 ` / ` 四段式结构。` / `数据通过单号（如 FIN-24091 ↔ FIT-P-002）在模块间相互关联。`
- `已启用` / `Beta` / `可扩展`
- 4 tabs：`全部` / `已启用` / `可添加` / `Beta`
- 12 个 ModuleCard 的 `name` + `desc`：
  - `Dashboard` / `全局指标与今日聚焦`
  - `财务管理` / `账户、预算、收支、投资组合`
  - `健身管理` / `训练计划、动作库、身体指标`
  - `学习管理` / `课程、阅读、笔记、Anki 集成`
  - `饮食管理` / `热量、宏量元素、备餐计划`
  - `睡眠分析` / `睡眠阶段、负荷、恢复建议`
  - `任务 / OKR` / `目标拆解、周期复盘`
  - `日记 / 情绪` / `每日心情、结构化反思`
  - `习惯追踪` / `每日打卡、连续天数、热图`
  - `个人资产盘点` / `物品、保修、折旧、定位`
  - `旅行 / 里程` / `行程、签证、里程账户`
  - `人际 / CRM` / `联系人、关系维护提醒`
  - 已启用/未启用/Beta 状态 label：`已启用` / `BETA` / `未启用`
- `模块能力指示` (title 属性)
- `已关联 3 模块` / `可关联 4 模块`
- `管理` / `加入 Beta` / `启用`
- `创建自定义模块` (再次) + `使用模板或从零开始 · 自定义字段与关联`
- `模块关联 · Inter-Module Links`
- `数据关联矩阵` (Card title) + `行 → 列 · 表示有引用关系`
- `源 → 目标` (表头)

key 命名规则：
- `marketplace.card.{module_code}.{name,desc}` （MODULE_CARDS 表）
- `marketplace.status.{on,beta,off}`
- `marketplace.btn.{manage,join_beta,enable,...}`

### v3 显式 ModuleCard keys（24 条 = 12 卡 × {name,desc}）

落地法：`mod_marketplace/src/view.rs::CARDS` 静态数组从 `name: "财务管理"` / `desc: "..."` 改为 `name_key: "marketplace.card.fin.name"` + `desc_key: "marketplace.card.fin.desc"`。view 渲染处 `t(locale, c.name_key)`。

| code | name_key | desc_key |
|---|---|---|
| DSH | `marketplace.card.dsh.name` | `marketplace.card.dsh.desc` |
| FIN | `marketplace.card.fin.name` | `marketplace.card.fin.desc` |
| FIT | `marketplace.card.fit.name` | `marketplace.card.fit.desc` |
| LRN | `marketplace.card.lrn.name` | `marketplace.card.lrn.desc` |
| NUT | `marketplace.card.nut.name` | `marketplace.card.nut.desc` |
| SLP | `marketplace.card.slp.name` | `marketplace.card.slp.desc` |
| TSK | `marketplace.card.tsk.name` | `marketplace.card.tsk.desc` |
| JRN | `marketplace.card.jrn.name` | `marketplace.card.jrn.desc` |
| HAB | `marketplace.card.hab.name` | `marketplace.card.hab.desc` |
| INV | `marketplace.card.inv.name` | `marketplace.card.inv.desc` |
| TRV | `marketplace.card.trv.name` | `marketplace.card.trv.desc` |
| REL | `marketplace.card.rel.name` | `marketplace.card.rel.desc` |

---

## 18. 公共 / shared 文案（被多处引用）

namespace = `app.common.*`

| key | zh-CN | en |
|---|---|---|
| `app.common.loading` | `loading…` | `loading…` (不翻) |
| `app.common.load_failed` | `加载失败 · ` | `Load failed · ` |
| `app.common.empty_dash` | `—` | `—` |
| `app.common.cancel` | `取消` | `Cancel` |
| `app.common.save` | `保存` | `Save` |
| `app.common.delete` | `删除` | `Delete` |
| `app.common.edit` | `编辑` | `Edit` |
| `app.common.create` | `创建` | `Create` |

---

## 19. 覆盖率自检脚本

实施完毕后跑：

```bash
# 应该返回 0
grep -nrE '"[一-鿿][^"]*"' /workspaces/code/Eigenpulse/modules /workspaces/code/Eigenpulse/app/src /workspaces/code/Eigenpulse/crates/ui /workspaces/code/Eigenpulse/crates/core/src/nav.rs --include='*.rs' | grep -v '//' | grep -v '#\[doc' | wc -l
```

正则会假阳性命中代码注释里的中文（如 SQL 注释 `-- "在用" 列`），人工确认。

---

## 20. 哪些**故意保留**为中文（不是漏掉）

- 设计 token 类的 mono 文案（如 `净资产 / NET WORTH`）—— 这是设计语言要求中英并置，**整段作一个 key**，不拆。
- 单号示例 `FIT-S-0412`、code 标签 `FIN-K01` —— 设计 ID，跨语言保留。
- placeholder 里的具体例子（如 `盒马 · 生鲜`、`月初分配 · 储蓄`）—— 翻译时英文版改成同等地道示例（`Whole Foods · groceries` / `Monthly allocation · savings`）。
- 用户域数据（`fin_account.name` / `fin_category.name` 表里的 `招商银行 · 主卡`）—— 详见方案 §7。
- README、CLAUDE.md、`docs/` 下的文档 —— 不在本任务范围。
