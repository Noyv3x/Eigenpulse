# Eigenpulse i18n 方案设计 (architect + frontend-dev + backend-dev, **v3**)

**owner**: architect · **status**: ✅ Phase A 已 commit `49ab330`（architect review 通过 + 3 nit + 1 gap，全部 Phase B/follow-up 范围）· **stack**: Leptos 0.7.8 / axum 0.7 / sqlx 0.8 (SQLite) / wasm32-unknown-unknown hydrate

> **v3 摘要**（2026-05-06，反映 Phase A 实际实施 + 三方 review 反馈）
>
> 1. **§0.5 决策时序 retrospective**（**新增**）：v1 → v2 → A=v1 → v2 → v1 → v2 终局的演化记录 + 「为什么不引 leptos_i18n」根因，给未来读者避坑。
> 2. **§2 detection 链 4 级**（不是 5 级）：URL → cookie → Accept-Language → `Locale::DEFAULT`。`app_user.locale` 列的 DB 查询从 middleware 移到 `app/src/login.rs::submit` 一次性种 cookie（首请求 cookie 缺失 + DB 非空时）。中间件零 IO。
> 3. **§5 切换器单按钮 + reload 模型**：Topbar 一颗 `lang-toggle`，显示 will-be label。`use_locale()` hydrate 端读 `<html lang>` attr（不是 fallback `Locale::DEFAULT` — 否则 toggle 单向 bug，Codex stop-time review 发现）；点击调 `client::switch_locale_via_reload(locale.toggle())` 写 cookie + `window.location.reload()`。SSR 重新渲染整页保证 `<html lang>` / chrome 文案 / DTO 全部对齐。
> 4. **§6 namespace 强制契约**（build.rs panic 校验，**新增表**）：每个 i18n 目录拥有唯一 prefix；`crates/i18n/i18n/` 独占 `app.common.*`，`modules/mod_marketplace/i18n/` 映射到 `marketplace.*`，其它 crate/module 走 `<crate-name>.*`。详见 §6 表。
> 5. **§8 错误码二分原则**（**新增**，backend-dev finance 19 处迁移实战提炼）：
>    - **E1 = 机器消费型英文**：PAT API 契约 / 内部不变量违反 / 数值合法性（如 `"merchant is required"` / `"unknown category_code 'X'"` / `"amount must be a positive number"`）— **保留英文不翻译**。PAT 调用方拿到稳定字符串便于编程匹配。
>    - **E2 = 用户文案型中文**：业务规则消息 / 引用不存在的资源（如 `"交易 'FIN-26092' 不存在"` / `"转账记录不可编辑..."` / `"分类 TFR 不存在或已归档..."`）— 走 `ep_i18n::err()/err_with()` + `<namespace>.err.<code>` keys。
> 6. **§10 风险表 retire wasm 体积风险**：实测 +4KB gzipped（vs baseline 662KB），远低于 §1 估算 < 10KB 上限。`phf::Map` 表 .rodata 共享 + reload 模式下 hydrate 端只需 fallback 字符串，证实手写 phf 是工程胜利。
>
> **Phase A 实测数据**：
> - Commit hash: `49ab330`（feat(i18n): Phase A — bilingual scaffold (zh-CN / en) with reload-mode toggle）
> - 31 文件 / +2411 行 / 含 backend-dev #9-12（set_user_locale + login cookie 种子 + finance 错误码 + finance fmt_server_err）
> - cargo check workspace + ssr 双向干净
> - wasm release gzipped: 666 KB（baseline 662 KB → +~4 KB）
> - Playwright 双向切换（zh-CN ⇄ en）+ `<html lang>` 同步 + 0 console errors
>
> **architect review (commit 49ab330) follow-up TODO**（非阻塞，独立 commit 处理）：
> - **nit 1**: `t()` miss path 的 `Box::leak(String)` 累积内存泄漏（dev/test 反复调时）。生产路径不命中（build.rs 校验兜底）。修复方案待定（panic / OnceLock interner / one-time leak）。
> - **nit 2**: `app/src/app.rs:59` NotFound 中文未迁移 → Phase B 一并做。
> - **nit 3**: `crates/ui/src/topbar.rs:31, 66` 残留中文 title（`折叠侧栏` / 主题切换 hover 文案）→ Phase B 顺手做（合并 Phase C）。
> - **gap**: `app/src/login.rs::page` 6 处中文 → Phase B 必做（team-lead 修正 #3 强制要求）。落地法：axum handler 用 `axum::Extension<Locale>` 提取器 + 普通 `t(locale, key)` 函数（非 leptos macro）。
>
> **历史版本归档**：v1 (leptos_i18n + reactive) 与 v2 (手写 phf + reload) 的演化过程见 §0.5。当前实施 = v2 终局。

---

## 0.5. 决策时序 retrospective（路线演化记录）

i18n 方案在 Phase A 实施期间经历了 6 次决策切换。这里如实记录，让未来读者**理解为什么不引 leptos_i18n** + 避免重蹈覆辙。

### 时序

| 节点 | 路线 | 决策依据 | 实施状态 |
|---|---|---|---|
| (1) 最早 | **v1**: `leptos_i18n = "0.5.11"` + 反应式 `<I18nContextProvider>` + `i18n.set_locale()` 切换 | architect 初版 plan + team-lead 第一条措辞 | frontend-dev 落地，Playwright 验证 SSR + hydrate 切换 OK |
| (2) | **v2**: 手写 `crates/i18n/` + `phf::Map` + reload 切换 | architect 接受 frontend-dev §4/§5 反馈：reload 模型下反应式框架卖点失效 + bundle +20-30KB 估算 vs phf +5KB | frontend-dev 切到 v2，重写 build.rs / lib.rs / client.rs |
| (3) | **A=v1**：撤回 v2，回到 v1 | team-lead 自己 cargo check 实测 v1 跑通，发现之前依赖 IDE 诊断判断 v1 失败是误读 | frontend-dev 已切 v2，磁盘是 v2，与 team-lead 认知错位 |
| (4) | **v2** | team-lead 看到 frontend-dev 已落地 v2 全套（build.rs 215 行 + client.rs + Cargo.toml + lib.rs 重写）+ Playwright 双向通过，承认 v2 工程优势（wasm +4KB 实测、不依赖停更框架、自维护小代码 < 250 行） | 与磁盘一致，commit 落定 |

### 为什么 v2 是终局（论据）

1. **wasm bundle 实测 +4KB gzipped**（reload 模式下 hydrate 端不参与翻译，wasm 只含 phf 表 .rodata 和 fallback 字符串）。leptos_i18n 0.5 即使关闭 ICU/format_*，框架自身的 `I18nContext` + 反应式 hook + dynamic_load 路径估 +20-30KB。
2. **leptos_i18n 0.5 已停更**（最新 0.6.x 不兼容 Leptos 0.7.8）。手写 phf 引擎 ~250 行（`build.rs` ~215 + `lib.rs` 函数体），自维护风险可控。
3. **reload 模式架构纯净**：cookie 是单一真相源；切语言 = 写 cookie + reload；SSR 重新渲染整页保证 chrome 文案 + DTO 数据全部对齐到新 locale。反应式模型下 server fn 返回 DTO 不会自动重 fetch，导致"切完语言后表头是英文，但表内容仍是 cookie 切换前的渲染"——边界模糊。
4. **per-crate 物理布局**与 `migrations/` 的项目惯例一致：每个 module 自带 `i18n/` 目录，删 module 时 i18n 资源同时被删，零孤儿 key。
5. **build.rs 编译期校验**：key 集合一致性 + namespace prefix + duplicate key 检测，让缺失/拼错在 build 期硬失败，比运行时 fallback 强。

### 为什么不是 v1（反驳论据）

- **反应式 i18n 在 SSR + reload 模型下没有用武之地**：反应式价值在于"客户端切语言无需服务器交互"——但这恰好是我们不需要的。我们要 SSR 用对的 locale 渲染 DTO + chrome，反应式只覆盖 chrome 不覆盖 DTO，半成品。
- **leptos_i18n 0.5.11 的 `[package.metadata.leptos-i18n]` + `load_locales!()` macro + 资源文件路径三处耦合在 workspace 多 crate 下脆弱**：namespace 增减必须同步修改三处，任意一处不一致编译失败（实施期间 backend-dev 加 finance namespace 时漏建 JSON 即触发）。手写 phf 的 build.rs 自动扫所有 i18n/ 目录，零配置。

### 关键经验教训

- **决策依据要凭 cargo check / 实际 commit / grep 验证**，不能凭 IDE 诊断（rust-analyzer 在 feature 关闭时展开 macro 误报）。
- **方案切换有显著成本**（每次 30-60min 重写 + Playwright 重测）。如果两个方案都跑通，**优先选已 commit 的现状**（沉没成本 + 可验证状态）。
- **i18n 框架选型与切换模型强耦合**：选 reload 切换 → 反应式框架卖点全废 → 手写最优。选反应式切换 → 框架价值显现 → leptos_i18n 合理。本项目选 reload 是因为 SSR 端 DTO 翻译边界更干净。

---

## 0. TL;DR（**先看这一段**）

- **i18n 引擎**：手写 `crates/i18n/`，`phf::Map<&'static str, &'static str>` × 2 locale，`fn t(locale, key) -> &'static str`。零反应式、零 ICU、零运行时 IO、SSR-only。**wasm bundle 增量 ≤ 10KB gzipped**（phf 自身 + 极少 fallback 字符串）。
- **资源布局**：`crates/<x>/i18n/{en,zh-CN}.json` 分散在每个 crate；`crates/i18n/build.rs` 在编译期扫描 + 合并 + phf_codegen → 单一静态查表。删 module 时一并删 i18n 文件，零孤儿 key。
- **Locale 同步模型**：cookie `ep_locale` 是**单一真相源**。axum 中间件把 `Locale` 注入 axum extension + leptos_axum context；server fn / view 通过 `expect_context::<Locale>()` 取。
- **切语言 = 写 cookie + `window.location.reload()`**。Hydrate 端**不**反应式重渲染——切语言闪一帧 SSR (~150-300ms)，可接受（低频操作）。
- **持久化**：cookie (`ep_locale`, Path=/, SameSite=Lax, Max-Age=1y, **不**Secure)、`app_user.locale` 列(`migrations/0002_user_locale.sql`)。无 localStorage 镜像（cookie 本身已在浏览器持久化，多写一份只增加复杂度）。
- **切换 UI**：Topbar `crates/ui/src/topbar.rs` 加一颗 `icon-btn`，显示**对面 locale 的标签**（zh-CN 时显示 "EN"，en 时显示 "中"），点击 = JS 写 cookie + reload。
- **DB 种子**：保留中文不动；新增 `app_user.locale` 列。
- **服务端错误**：`#[server]` 返回错误码字符串 `err:finance.txn_not_found`，view 端用 `t!(locale, "finance.err.txn_not_found")` 翻译。
- **实施顺序**：A 基础设施 (i18n crate + 中间件 + Topbar 按钮 + `<html lang>`) → B app/views → C ui+nav → D 业务模块。每批之间 `cargo check --workspace` 干净 + smoke 通过。

---

## 1. 框架决策：手写 `crates/i18n`

### 弃 leptos_i18n / leptos-fluent / rust-i18n 的根因

**核心论据**：选定 **reload 切换模型** (§3) 后，反应式 i18n 框架的所有卖点都失效：
- 反应式信号 (`I18nContext`) — 不需要，每次渲染 SSR 已用对 locale
- 编译时 key 校验 — 我们 build.rs 自己做（150 行）
- 复数 / 性别 / 选择器 (ICU) — 中文无复数；英文文案设计上避开复数（用 "{n} entries" 而非 "1 entry / 2 entries"）
- cookie 解析 — axum middleware + cookie crate 现成，不需要框架封装

留下的代价（如果引入 leptos_i18n 0.5.11）：
- 即便关掉 `icu_compiled_data` / `format_*` / `plurals`，框架自身的 `I18nContext` + 反应式 hook + dynamic_load 路径估 +20-30KB wasm
- 对项目"维护 i18n 框架的版本依赖"——0.5 已停更（team-lead 已确认），未来要么本地 fork 要么不维护
- 多一层抽象，view 写 `t!(i18n, key)` 就不能跨 crate 复用同一函数（i18n context 是 Leptos provide_context 拿的，模块外引用要打通）

### 实施

```rust
// crates/i18n/src/lib.rs
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Locale {
    #[default]
    ZhCn,
    En,
}

impl Locale {
    pub fn as_str(&self) -> &'static str {
        match self { Self::ZhCn => "zh-CN", Self::En => "en" }
    }
    pub fn parse(s: &str) -> Self {
        // 精确匹配 + 前缀回退（"zh-Hans" / "zh-CN" / "zh" → ZhCn；其他 → En）
        // 但保守起见仅支持 v1 集合：
        if s.eq_ignore_ascii_case("zh-CN") || s.eq_ignore_ascii_case("zh") || s.starts_with("zh-") {
            Self::ZhCn
        } else if s.eq_ignore_ascii_case("en") || s.starts_with("en-") {
            Self::En
        } else {
            Self::default()
        }
    }
    pub fn toggle(self) -> Self {
        match self { Self::ZhCn => Self::En, Self::En => Self::ZhCn }
    }
}

// build.rs 生成的（include!() 进 lib.rs）
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

// 公共 API
pub fn t(locale: Locale, key: &str) -> &'static str {
    let map = match locale {
        Locale::ZhCn => &ZH_CN,
        Locale::En => &EN,
    };
    map.get(key).copied().unwrap_or_else(|| {
        // SSR-only：缺 key 时直接返回 key 本身（让 missing key 在视觉上显眼）
        // 同时 tracing::warn!() 一行
        #[cfg(feature = "ssr")]
        tracing::warn!(target: "i18n", locale = %locale.as_str(), %key, "i18n key missing");
        key
    })
}

/// 带 `{name}` 占位符插值。少量场景：finance.err.* / 含 {n}/{date}/{period} 的 sub label
pub fn tf(locale: Locale, key: &str, args: &[(&str, &str)]) -> String {
    let template = t(locale, key);
    let mut out = template.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}
```

**build.rs**（伪代码骨架，实际 ~150 行）：

```rust
// crates/i18n/build.rs
use phf_codegen::Map;

fn main() {
    let workspace_root = std::env::var("CARGO_MANIFEST_DIR").unwrap()
        .parent().unwrap().parent().unwrap().to_path_buf();

    // 扫两个根：crates/<x>/i18n/{en,zh-CN}.json 和 modules/<x>/i18n/{en,zh-CN}.json
    let zh = collect_locale(&workspace_root, "zh-CN");
    let en = collect_locale(&workspace_root, "en");

    // 校验 1：两个 locale 的 key 集合必须一致
    assert_eq!(zh.keys().collect::<BTreeSet<_>>(), en.keys().collect::<BTreeSet<_>>(),
               "i18n: zh-CN and en key sets diverge");

    // 校验 2：grep 整个工作区 t!(.., "key") / tf!(.., "key", ..) 调用，确认 key 存在
    let used_keys = grep_t_calls(&workspace_root);
    let known_keys = zh.keys().collect::<BTreeSet<_>>();
    let missing: Vec<_> = used_keys.iter().filter(|k| !known_keys.contains(*k)).collect();
    if !missing.is_empty() {
        panic!("i18n: keys used in code but not defined in JSON:\n{:#?}", missing);
    }

    // 生成 phf_codegen
    let out = std::env::var("OUT_DIR").unwrap();
    let mut f = std::fs::File::create(format!("{out}/generated.rs")).unwrap();
    write_phf(&mut f, "ZH_CN", &zh);
    write_phf(&mut f, "EN", &en);

    // rerun-if-changed 精确到 i18n/ 目录
    println!("cargo:rerun-if-changed={}/crates", workspace_root.display());
    println!("cargo:rerun-if-changed={}/modules", workspace_root.display());
}
```

### 依赖增量

```toml
# crates/i18n/Cargo.toml
[dependencies]
phf = { version = "0.11", default-features = false }
serde = { workspace = true }
tracing = { workspace = true, optional = true }

[features]
ssr = ["dep:tracing"]
hydrate = []

[build-dependencies]
phf_codegen = "0.11"
serde_json = "1"
walkdir = "2"
```

`phf` 0.11 自身 wasm 增量 ~5KB（gzipped），`tracing` SSR-only 不进 wasm。**总 wasm 增量预估 < 10KB gzipped**（含静态字符串）。

> 实测验证：Phase A 完成后 `cargo leptos build --release && ls -lh target/site/pkg/eigenpulse_bg.wasm.gz` 对比 main 分支基线，记录差值。超 +30KB 报警。

---

## 2. Locale Detection 优先级（v1 不变）

```
1. URL ?locale=en            (调试用、不持久化)
2. cookie ep_locale          (用户主动选过)
3. user.locale (DB)          (登录后从 app_user.locale 读)
4. Accept-Language header    (浏览器默认)
5. 硬 fallback: zh-CN
```

支持的 locale 集合（v1）：

```rust
// crates/i18n/src/lib.rs
pub const SUPPORTED: &[&str] = &["zh-CN", "en"];
```

不收 `zh-TW` / `zh-HK` / `en-GB` 等子标签 —— `Locale::parse` 通过前缀回退把它们归并到 `zh-CN` / `en`。

---

## 3. SSR-only 同步模型（**关键变更**）

### 数据流（**简化版**）

```
[ Browser request ]
    ↓ Cookie: ep_locale=zh-CN
[ axum middleware: locale_layer ]
    - 解析优先级链 (§2) → locale: Locale
    - req.extensions_mut().insert(locale)
    - 如果是首次访问 (cookie 缺失)：附加 Set-Cookie 响应头
    ↓
[ leptos_routes_with_context ]
    - provide_context(locale)         ← server fn / view 在这里拿
    ↓
[ render to HTML ]
    - <html lang={locale.as_str()}>
    - 所有 view 调 t!(locale, "key") / tf!(locale, "key", ...) → 直接展开成 String
    ↓
[ Hydrate target ]
    - **不读 locale**。SSR 已渲染对的字符串，hydrate 只重建 DOM 树
    - wasm 包里 i18n 函数仍可调用（Topbar 切换按钮里用），但走 fallback 分支
      （wasm 端 ZH_CN/EN 静态表也存在 — phf 表是 const，跨 SSR/hydrate 共享 .rodata）
```

### 切语言事件流

```
[ User clicks Topbar lang button ]
    ↓ JS on:click
    1. document.cookie = "ep_locale=en; Path=/; Max-Age=31536000; SameSite=Lax"
    2. window.location.reload()
    ↓
[ Browser sends new GET with new cookie ]
    ↓
[ SSR renders integral page in new locale ]
```

### 关键避雷点

- **`<html lang="...">` 必须 SSR 端动态生成**（`app/src/app.rs:69` 当前硬编码 `zh-CN`，要改）
- **server fn 内部也要拿 Locale**（错误码翻译场景）：通过 `expect_context::<Locale>()` 同步获取，不需要 await
- **Hydrate 端 view 里 `t!(locale, key)` 也能调**（phf 表存在），但实际**永不用到**——因为整页 SSR 已经把字符串字面量编译进了 view 函数；hydrate 只是 reconcile DOM，不重新执行 view body 的字符串构建
- **Wasm-side panic 风险**：`tracing::warn!` 在 wasm 上不能用。i18n 的 missing-key warn 用 `#[cfg(feature = "ssr")]` 包；hydrate 端缺 key 静默回退到 key 字面量

---

## 4. 持久化策略

### Cookie 设计

```rust
// crates/i18n/src/cookie.rs
pub const LOCALE_COOKIE: &str = "ep_locale";

pub fn cookie_value(locale: Locale) -> String {
    format!("{LOCALE_COOKIE}={}; Path=/; Max-Age=31536000; SameSite=Lax",
            locale.as_str())
    // 故意不加 Secure (与 ep_sid 一致, CLAUDE.md「LAN/NAS HTTP」)
}
```

不签名（preference 不需要完整性，且 leptos_i18n 内置 cookie 解析也不签名）。

### `app_user.locale` 列

```sql
-- migrations/0002_user_locale.sql (新文件)
ALTER TABLE app_user ADD COLUMN locale TEXT NOT NULL DEFAULT '';
-- 不需要 back-fill — 空串视为"未明示"，走 cookie/Accept-Language fallback
```

CLAUDE.md「Migration discipline」：新增 `0002_*.sql` 是合规路径（**不**编辑 0001）。

### 持久化时机

- 用户在 Topbar 切换 → JS 直接写 cookie + reload
- SSR 中间件如果首次访问（cookie 缺失）从 `Accept-Language` 决定 locale → 在响应里 Set-Cookie，下次访问就有
- 登录后（`app/src/login.rs::submit` 成功路径）：如果 `app_user.locale` 非空，且与当前 cookie 不一致，覆盖 cookie。这条**Phase B 之后再做**（不影响首阶段闭环）

### 不要 localStorage 镜像

v1 方案曾参考 ep_tweaks 写 cookie + localStorage 双份。**v2 简化**：cookie 本身已在浏览器持久化，多一个 localStorage 镜像没有任何额外好处（reload 模式下 SSR 永远靠 cookie，不读 storage）。

---

## 5. 切换器 UI（Topbar 单按钮）

### 设计

参考 `crates/ui/src/topbar.rs:45-57` 的 theme button「显示 will-be 而非 current」模式：

```rust
// crates/ui/src/topbar.rs (简化片段)
let locale = use_locale();   // 从 context 取，SSR/hydrate 都拿一样的值

view! {
    // ... 现有 theme button 之前
    <button
        class="icon-btn lang-toggle"
        title=move || match locale.get() {
            Locale::ZhCn => "Switch to English",
            Locale::En => "切换到中文",
        }
        on:click=move |_| {
            #[cfg(feature = "hydrate")]
            switch_locale_via_reload(locale.get().toggle());
        }>
        <span class="mono" style="font-size:11px;font-weight:600">
            {move || match locale.get() {
                Locale::ZhCn => "EN",   // 显示 will-be
                Locale::En => "中",
            }}
        </span>
    </button>
}
```

```rust
// crates/i18n/src/client.rs
#[cfg(feature = "hydrate")]
pub fn switch_locale_via_reload(target: Locale) {
    use wasm_bindgen::JsCast;
    let Some(win) = web_sys::window() else { return };
    let Some(doc) = win.document() else { return };
    let Ok(html_doc) = doc.dyn_into::<web_sys::HtmlDocument>() else { return };
    let _ = html_doc.set_cookie(&crate::cookie::cookie_value(target));
    let _ = win.location().reload();
}
```

### 为什么是 `icon-btn` 不是 `seg`

- 语义：切换按钮是命令式动作（"切到 X"），不是输入态；segmented 控件是状态选择（适合 density 那种）
- 视觉一致性：与 Sun/Moon theme 按钮在 Topbar 上挨着，宽 24px 等高
- 设计语言：`<span class="mono" font-size:11px font-weight:600>` 与 `crates/ui/src/sidebar.rs:64` 的 `<span class="code mono">` (FIN/FIT/LRN 代号) 一致

### 不放 `/settings`

Topbar 已露出，settings 重复露出会割裂"单一来源"。reload 模式下 settings 卡片做切换更别扭（用户在 settings 里点一下按钮整页跳走）。

---

## 6. 资源文件物理布局

### 推荐：per-crate 磁盘 + 编译期合并

```
crates/
├── core/
│   └── i18n/
│       ├── en.json         ← core.nav.section.* 等
│       └── zh-CN.json
├── ui/
│   └── i18n/
│       ├── en.json         ← ui.topbar.* / ui.sidebar.*
│       └── zh-CN.json
└── i18n/                   ← 新 crate, 引擎本体
    ├── src/{lib,cookie,client,middleware}.rs
    ├── build.rs            ← 扫所有 i18n/ 目录合并
    └── i18n/               ← app.* / app.common.*（404 / loading…）
        ├── en.json
        └── zh-CN.json

modules/
├── finance/
│   └── i18n/
│       ├── en.json         ← finance.* (含 finance.err.* 错误码)
│       └── zh-CN.json
├── fitness/i18n/...
├── learning/i18n/...
└── mod_marketplace/i18n/...

app/
└── i18n/                   ← app.dashboard.* / app.today.* / app.reports.* / app.settings.*
    ├── en.json
    └── zh-CN.json
```

`crates/i18n/build.rs` 用 `walkdir` 扫两条根：`workspace_root/crates/*/i18n/` + `workspace_root/modules/*/i18n/` + `workspace_root/app/i18n/`，合并所有 `en.json` 到一个 `BTreeMap<String, String>`，`zh-CN.json` 同理。两个 map 的 key 集合**必须**一致（build.rs 校验）。

### key namespace 强制约定

| crate / 目录 | 允许的 key 前缀 |
|---|---|
| `crates/core/i18n/` | `core.*` |
| `crates/ui/i18n/` | `ui.*` |
| `crates/i18n/i18n/` | `app.common.*`（loading / load_failed / —） |
| `app/i18n/` | `app.dashboard.* / app.today.* / app.reports.* / app.notifications.* / app.settings.* / app.notfound.* / app.login.*` |
| `modules/finance/i18n/` | `finance.*` |
| `modules/fitness/i18n/` | `fitness.*` |
| `modules/learning/i18n/` | `learning.*` |
| `modules/mod_marketplace/i18n/` | `marketplace.*` |

build.rs 在合并时校验每条 key 的前缀与来源目录一致（违反 panic + 列出错位 key）。

### v3 build.rs 实测约束（Phase A 已实施 commit 49ab330）

`crates/i18n/build.rs` 落地后做了 3 条 panic-on-violation 校验：
1. **key-set 一致性**：`zh-CN` 与 `en` 的 key 集合必须完全相同（任一 locale 缺 key → panic 列出差集）
2. **namespace prefix 匹配**：每个 i18n 文件的所有 key 必须以表中规定的前缀开头（违反 → panic 列出错位 key + 文件路径）
3. **duplicate key 检测**：合并时同 key 在两个文件出现 → panic（多 namespace 共享 key 是程序员错误）

边界条件：
- **空 i18n 树**（项目刚 init 还没 keys）→ build.rs emit empty `phf::Map`，编译通过
- **`mod_marketplace/` 长目录名** → namespace 表特例映射到 `marketplace.*`（不是 `mod_marketplace.*`，更短更合理）
- **`crates/i18n/i18n/` 自身**（引擎 crate） → 独占 `app.common.*` prefix（loading… / error / unknown_locale 等跨模块共享 chrome）

### 文件结构示例

`modules/finance/i18n/zh-CN.json`：

```json
{
  "finance.page.module": "FINANCE · 财务管理",
  "finance.page.title": "Finance",
  "finance.page.title_cn": "财务管理",
  "finance.page.subtitle": "账户、预算、收支、投资。支持跨模块关联与自动分类。",
  "finance.page.action_record": "记一笔",
  "finance.kpi.monthly_budget": "本月预算",
  "finance.tab.ledger": "总账 / Ledger",
  "finance.err.txn_not_found": "交易 '{doc_id}' 不存在",
  "finance.err.tfr_not_editable": "转账记录不可编辑，请删除后重建"
}
```

JSON 用**扁平 key**（不嵌套）。优点：
- phf_codegen 直接吃 `BTreeMap<String, String>`
- key 命名即 path（`finance.page.title`）一目了然
- 合并时无需递归 merge，shadowing/冲突 build.rs 一行检测出来

---

## 7. DB 种子数据策略（v1 不变）

`migrations/001_finance.sql` 中的 `'招商银行 · 主卡' / '餐饮' / 'Blue Bottle · 上海'` **保留不动**。详见 v1 方案 §7（用户域内容不该 i18n 化）。

---

## 8. 服务端错误信息策略（**v3 实战版**）

错误码方案 + **二分原则**（backend-dev 在 finance/server_fns 19 处迁移中提炼，已 commit `49ab330`）。

### 二分原则（**Task #6 fitness/learning/marketplace 必须套用**）

server_fn 现有 `args_err(...)` 调用按以下规则迁移：

| 类型 | 内容 | 处理 | 例子 |
|---|---|---|---|
| **E1** 机器消费型英文 | PAT API 契约 / 内部不变量 / 数值合法性 | **保留英文不翻译** | `"merchant is required"` / `"unknown category_code 'X'"` / `"amount must be a positive number"` / `"opening_balance must be finite"` |
| **E2** 用户文案型中文 | 业务规则消息 / 引用不存在的资源 / 用户操作约束 | 走 `ep_i18n::err()/err_with()` + `<namespace>.err.<code>` keys | `"交易 'FIN-26092' 不存在"` / `"转账记录不可编辑..."` / `"分类 TFR 不存在或已归档..."` |

判定准则：
- **该消息是否会被 PAT 调用方编程匹配？** → 是 = E1 保留英文
- **该消息只展示给浏览器用户看？** → E2 走错误码 + i18n
- 边界情况：先 grep `tests/` 看测试里是否 assert 该字符串字面量 → 如果有 → E1（破坏测试 = 破坏契约）

### Phase A 已固化的 helper API（不动）

```rust
// crates/i18n/src/errors.rs（已实施 commit 49ab330）
use leptos::server_fn::ServerFnError;

pub const ERR_PREFIX: &str = "err:";

/// 单纯错误码（无运行时参数）
pub fn err(code: &str) -> ServerFnError {
    ServerFnError::Args(format!("{ERR_PREFIX}{code}"))
}

/// 错误码 + 单 payload（如 doc_id）
pub fn err_with(code: &str, payload: impl std::fmt::Display) -> ServerFnError {
    ServerFnError::Args(format!("{ERR_PREFIX}{code}:{payload}"))
}

/// 客户端 view 解析：模式匹配 ServerFnError::Args 变体（不依赖 Display 字符串）
pub fn parse_err(e: &ServerFnError) -> Option<(&str, Option<&str>)> {
    let ServerFnError::Args(s) = e else { return None };
    let rest = s.strip_prefix(ERR_PREFIX)?;
    let mut it = rest.splitn(2, ':');
    let code = it.next()?;
    Some((code, it.next()))
}
```

### View 端用法（finance 已实施，模板复制即可）

```rust
// modules/finance/src/view.rs::fmt_server_err
use ep_i18n::{parse_err, tf, use_locale};

fn fmt_server_err(e: &leptos::server_fn::ServerFnError) -> String {
    let locale = use_locale();
    match parse_err(e) {
        Some((code, payload)) => {
            // code 例如 "finance.txn_not_found"
            // payload 是 err_with 提供的运行时值
            let key = format!("{}.err.{}", code.split('.').next().unwrap_or("app.common"),
                              code.rsplit('.').next().unwrap_or(code));
            // 实际落地用更直接的方式：直接传 code 作为完整 key，资源里就用 code 当 key
            tf(locale, code, &[("payload", payload.unwrap_or(""))])
        }
        None => e.to_string(),  // 系统级错误（network / sqlx）原样吐
    }
}
```

> 实施 note: backend-dev 在 finance/view.rs 直接用 `tf(locale, code, &[("payload", p)])`，不做 `<ns>.err.<code>` 的二次拼接 — 资源 key 直接就是错误码本身（如 zh-CN: `"finance.txn_not_found": "交易 '{payload}' 不存在"`）。这样 view 端只需 1 个函数调用，无任何字符串处理 logic。

### 所有 view 里 `e.to_string()` 渲染点的迁移

grep 模式：`{e\.to_string()}` 或 `Err(e) => view! { <p>"加载失败 · " {e.to_string()}` 等
替换为：`{fmt_server_err(&e)}` （helper 自身闭包 `use_locale()`）

---

---

## 9. 实施分批策略（**简化版**）

每批之间硬要求：`cargo check --workspace` 干净 + 服务能启动 + 涉及页面 smoke 通过 + 独立 commit。

### Phase A · 基础设施（**i18n crate + 中间件 + Topbar 切换 + `<html lang>`**）

1. 新建 `crates/i18n/`（Cargo.toml / lib.rs / cookie.rs / middleware.rs / client.rs / errors.rs / build.rs）
2. workspace `Cargo.toml` 注册 `ep-i18n = { path = "crates/i18n" }`
3. 写一份最小 `crates/i18n/i18n/{en,zh-CN}.json` 含 3 个 demo key（`ui.topbar.lang_toggle.title`, `app.common.loading`, `app.common.load_failed`）
4. `crates/auth/src/middleware.rs` 旁边加 `locale_layer`（也可以放 `crates/i18n/src/middleware.rs`，axum middleware 用 `from_fn_with_state`）：
   - 解析优先级链 → 把 `Locale` 注入 `req.extensions_mut()`
   - 首次访问无 cookie → 在响应附 Set-Cookie 头
5. `app/src/main.rs` 注册中间件（`Router::<AppState>::new().layer(...)`）+ 在 `leptos_routes_with_context` 里 `provide_context(locale)`
6. `app/src/app.rs:69` 改 `<html lang="zh-CN">` → `<html lang={expect_context::<Locale>().as_str()}>`
7. `crates/ui/src/topbar.rs` 加切换按钮（设计见 §5）
8. `migrations/0002_user_locale.sql`（虽然 Phase A 还不用，建一起省事）
9. 测 wasm bundle 尺寸基线（main 分支）vs 当前分支 → 记录差值
10. **Smoke**: `EP_ADMIN_PASSWORD=dev cargo leptos watch` → 切语言 → reload 后 `<html lang>` 变了 → cookie 持久 ✅

**退出门槛**：bundle 增量 ≤ +30KB；`cargo check --workspace` 通过；切语言可见效果。

### Phase B · app/views（共 ~155 中文 key）

`app/src/views/{dashboard,today,reports,notifications,settings/*}.rs`。逐文件搬，每个文件一个独立 commit。

- 每搬一个文件都更新对应 `app/i18n/{en,zh-CN}.json`，单批 commit 三个文件（Rust + 两 JSON）
- 文件搬完跑 `grep -nrE '"[一-鿿][^"]*"' app/src/views/<file>.rs` → 应零 hit（除注释行）
- `app/src/login.rs` 是 axum handler 不是 Leptos 组件，从 `Extension(Locale)` 提取 → 调 `t(locale, key)`

### Phase C · crates/ui + crates/core/nav（~17 key）

- `crates/ui/src/{topbar,sidebar}.rs` 把 hardcode 中文换成 `t!(locale, key)`
- `crates/core/src/nav.rs::NavSection::label` 改成接 i18n
- `crates/ui/src/sidebar.rs::NAV` 静态数组的 `name`/`name_cn` 字段处理：选项 A 删字段 + view 端 `t!(locale, format!("core.nav.{}.{name|cn}", code))`；**选项 B 推荐**：保留字段但值改成 i18n key（如 `name: "core.nav.dsh.name"`），view 端用 `t!(locale, item.name)`

### Phase D · 业务模块（~268 中文 key）

按密度从大到小：finance (168) → fitness (54) → learning (43) → mod_marketplace (35) → finance/server_fns (30, 错误码改造)。每模块独立 commit。

### Phase E · 测试 (Task #7)

- `crates/i18n/tests/locale_parse.rs` —— `Locale::parse` 边界（"zh-Hans" / "zh-CN" / "en-GB" / "fr"）
- `crates/i18n/tests/cookie.rs` —— cookie 编解码
- `app/tests/smoke.rs` 扩 —— SSR 端 `Accept-Language: en` 请求 `/` → 响应 HTML 含 `<html lang="en">` + `Set-Cookie: ep_locale=en`
- `app/tests/smoke.rs` 扩 —— `Cookie: ep_locale=en` 请求 `/finance` → 响应包含 "Finance" 字面量但**不**包含 "财务管理"

### Phase F · Playwright (Task #8)

- 切语言交互测试：访问 → 点 Topbar 按钮 → 等 reload → 断言中文消失、英文出现
- 跨页保留测试：切到 en 后导航 `/dashboard` → `/finance` → `/settings` → 每页都是 en
- `<html lang>` 断言

---

## 10. 风险与备案（**v3 — Phase A 实测后 retire 多项**）

| 风险 | 概率 | 影响 | 状态 / 备案 |
|---|---|---|---|
| ~~wasm bundle 体积爆炸（leptos_i18n 框架自身重）~~ | ~~中~~ | ~~超 450KB 红线~~ | ✅ **RETIRED** — 选 v2 手写 phf 后实测 +4 KB gzipped (vs baseline 662KB)，远低于 §1 估算 < 10KB |
| `phf 0.11` 生成的查表对长 key 性能不达预期 | 极低 | 渲染慢 | phf 是 perfect hash，O(1)；500 key 规模瞬秒 |
| build.rs 编译期校验漏掉某些 key 路径 | 低 | 运行时 missing-key fallback | Phase A 已实施 3 条校验（key-set 一致 + namespace prefix + duplicate panic）。运行时 fallback 返回 key 字面量本身，缺失 key 在 UI 上视觉显眼 |
| **`t()` miss path `Box::leak(String)` 累积内存泄漏**（architect review nit 1）| 低 | 单测/dev 反复调时持续泄漏 | follow-up commit 处理。方案候选：(a) panic 强制 build 期发现；(b) `OnceLock<RwLock<HashMap>>` interner 复用 leak 指针；(c) 保留 fallback 但加入测试期 `cfg(test)` 跳过 leak |
| 切语言 reload 闪一帧让用户疑惑 | 低 | 体验 | reload ~150-300ms。可接受（低频操作），未来若有反馈再加 `body.style.opacity` 过渡 |
| Topbar 切换按钮在 SSR/hydrate 表现不一致 | 低 | 按钮 label 闪 | ✅ Phase A 实施确认：SSR 用对的 locale 渲染按钮 label，hydrate 端 `use_locale()` 读 `<html lang>` 与 SSR 一致，无闪 |
| **toggle 单向 bug**（Codex stop-time review 发现）| 高 | 从 en 切不回 zh | ✅ Phase A 修复：`use_locale()` hydrate 端读 `<html lang>` 而非 fallback `Locale::DEFAULT`（lib.rs:140），Playwright 双向验证通过 |
| 首次访问 SSR 中间件 Set-Cookie 与 leptos_axum 路由互相影响 | 中 | 首次访问无 cookie | login.rs::submit 一次性种 cookie（DB → cookie），避开 middleware 副作用响应路径。Phase A 已实施 |
| `<html lang>` 在 hydrate 后被某个第三方 lib 改回 | 极低 | a11y 略劣 | 我们没有这种 lib，且 reload 模式下 hydrate 端不改 lang |
| `app_user.locale` 列与 cookie 优先级冲突 | 低 | 跨设备登录看到不一致 | cookie 优先于 DB（§2），本机持久；cross-device 首次登录由 login.rs 一次性把 DB locale 种到 cookie。Phase A 已实施 |
| build.rs grep 视图代码假阳性 | n/a | 不适用 | v3 不做 view 代码 grep 校验（key-set 一致 + namespace prefix 已足够）。后续若有 missing key 投诉再加 |

---

## 11. 文件清单与计数（v1 §11 不变）

详见 `.dev-team/i18n-keys.md`（v1 已生成，**v2 不需要重生成**——key 命名/分布与框架解耦）。

namespace 与 v2 物理布局一一对应，**已经一致**：
- `app.* / app.common.*` → `app/i18n/` + `crates/i18n/i18n/`
- `ui.*` → `crates/ui/i18n/`
- `core.*` → `crates/core/i18n/`
- `finance.* / fitness.* / learning.* / marketplace.*` → 各 `modules/<x>/i18n/`

文件计数仍然是 ~480 真翻译 key (~960 翻译条目)。

---

## 12. 接下来谁干什么（**v4 — Phase B/C/D + smoke 后状态**）

| Task | Owner | 状态 | 关键产物 |
|---|---|---|---|
| #1 i18n 框架选型与方案设计 | architect | ✅ completed | 本 plan v1→v3 |
| #2 枚举所有需要翻译的硬编码字符串 | architect | ✅ completed | `.dev-team/i18n-keys.md` |
| #3 引入 i18n 框架并建立基础设施 | frontend-dev | ✅ completed (49ab330) | `crates/i18n/` 全套 + Topbar 按钮 + `<html lang>` 动态 |
| #4 实现 locale 持久化与中间件 | backend-dev | ✅ completed (49ab330) | 4 级 `locale_layer` + `app/src/main.rs` 集成 + `migrations/0002_user_locale.sql` |
| #5 迁移 app/views + crates/ui 文案 | frontend-dev | ✅ completed | Phase B + C 合并（含 `app/src/login.rs` axum handler i18n + architect nit 2/3 + gap 修复） |
| #6 迁移各 module 的 view + server_fns 文案 | backend-dev | ✅ completed | Phase D（finance/fitness/learning/marketplace view + server_fn 错误文案） |
| #7 单元 + 集成测试 | tester | ✅ completed | Phase E：i18n locale/cookie 单测 + app SSR locale smoke |
| #8 Playwright 视觉验收 | ux-tester | ✅ completed | Phase F：MCP 浏览器验证语言切换 reload + 跨页保持 + settings 子页 console |
| #9-#12 backend-dev sub-tasks | backend-dev | ✅ all completed (49ab330) | set_user_locale + login cookie 种子 + finance 错误码 + finance fmt_server_err |

**Phase B/C/D/E/F 已闭环**。当前验证：
- `cargo test --workspace`
- `cargo test -p eigenpulse --features ssr --test smoke -- --nocapture`
- `rg -n '"[^"]*[一-龿][^"]*"' app/src crates modules --glob '*.rs'` 零命中
- Playwright MCP：`zh-CN` 登录态点击 Topbar 语言按钮 → reload 后 `html.lang = "en"`、`ep_locale=en`；`/finance`、`/settings` 英文状态保持；`/settings/notifications`、`/settings/security` console 0 errors（仅浏览器 meta deprecation warning）
