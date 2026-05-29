# Eigenpulse · Personal Life ERP

> 模块化的个人生活管理系统 — 财务 / 健身 / 学习 / 通知 / 开放 API。
> 全 Rust 栈（Leptos SSR + Hydration），单二进制，单容器，能在 NAS 上跑得动。

---

## 设计哲学

**"模块化严谨 × 生活温度"** —— 把企业 ERP 的结构感（单号、KPI 网格、跨模块关联）和个人应用的柔和感（暖色、衬线中文、圆角）缝合在一起。

- **色彩**：oklch 暖米白底 + 鼠尾草绿主色 + 琥珀/玫瑰/蓝/紫 柔和辅色，Light/Dark 双主题
- **字体**：Inter (UI) · JetBrains Mono (编号/数值) · Noto Serif SC (中文标题点缀)
- **栅格**：4px 基准（紧凑模式 3px），`<App data-theme=… data-density=…>`
- **ERP 基因**：单号 (`FIN-24091`、`FIT-S-0412`、`LRN-N-221`)、模块代码、KPI 网格、跨模块关联矩阵、四段式 (`KPI · Ledger · Reports · Settings`)

---

## 技术栈

| 层 | 选型 |
|---|---|
| 前端 | **Leptos 0.7**（Rust SSR + Hydration） |
| 后端 | **axum** + Tokio（通过 `leptos_axum` 集成） |
| 数据库 | **SQLite + sqlx** (WAL，runtime query)；`sqlx::migrate!()` 启动时自动跑全局迁移，模块迁移由 `_ep_module_migration` 账本幂等执行 |
| 鉴权 | Argon2id 密码 + 签名 cookie session（30d 滑动） |
| 开放 API | Personal Access Tokens (`Authorization: Bearer ep_pat_…`) |
| 通知 | 站内 SSE + SMTP + Bark + Telegram + Discord |
| 移动端 | PWA（`manifest.webmanifest` + service worker） |
| 部署 | 单二进制 + 多阶段 Dockerfile + distroless + 多架构 (amd64+arm64) |
| 模块化 | Cargo workspace + `Module` trait，每模块独立 crate |

---

## 工程结构

```
Eigenpulse/
├── crates/
│   ├── core/               # Module trait + Registry + AppState + IDs
│   ├── ui/                 # 共享 Leptos 组件 + Icon 集
│   ├── db/                 # sqlx pool + 全局迁移
│   ├── auth/               # Argon2id + cookie session + PAT 中间件
│   ├── notify/             # Notifier trait + 5 种实现 + NotifyBus
│   └── api/                # /api/v1/* 开放 API
├── modules/
│   ├── finance/            # ✅ 完整：schema + CRUD + 4 tab UI + open API
│   ├── fitness/            # ✅ 训练记录 CRUD + 趋势/连续训练 + open API
│   ├── learning/           # ✅ 书籍/笔记/课程 CRUD + 热力图 + open API
│   └── mod_marketplace/    # 已注册模块总览；未来模块构想入口
├── app/                    # SSR binary + <App/> shell + 视图
├── assets/                 # styles.css / manifest / sw.js / icons
├── migrations/             # 0001_init.sql (核心 + 通知 + PAT)
└── Dockerfile / docker-compose.yml
```

---

## 快速开始

### 本地开发

```bash
# `rust-toolchain.toml` pins stable; rustup picks it up automatically.
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --locked --version 0.3.6

# `.cargo/config.toml` pins the `mold` linker for Linux-GNU targets — much
# lower RAM (~3x) and faster on this multi-crate workspace. Required on
# Linux dev hosts. macOS / Windows targets aren't matched and use their
# default linker, so this is a Linux-only prerequisite.
sudo apt install mold        # Debian/Ubuntu
# brew install mold           # ← do NOT — mold has no macOS support; on Mac
                              #   the config doesn't activate, no install needed

export EP_ADMIN_PASSWORD='dev-password'
# DATABASE_URL has a built-in default (sqlite://data/eigenpulse.db?mode=rwc);
# the app creates the parent directory and .db file on first connect.
# EP_SECRET is optional; if omitted, the app reads or creates data/secret.key.

cargo leptos watch          # http://127.0.0.1:3000
```

> 首次启动会用 `EP_ADMIN_PASSWORD` 创建 OWNER 账户。该变量缺失则进程拒绝启动。

> **WASM 命名说明**：cargo-leptos 0.3.6 产出 `eigenpulse.wasm`，当前 Leptos
> 的 `<HydrationScripts/>` 正好加载 `eigenpulse.wasm`。部分版本的加载器（以及
> wasm-bindgen 默认）会改用 `eigenpulse_bg.wasm`，为此服务器在
> `app/src/main.rs` 的 `/pkg` 路由加了 `ServeDir::fallback`：当 `_bg.wasm`
> 不存在时回退到 `eigenpulse.wasm`（保持 `200` + `application/wasm`）。于是
> 无论加载器请求哪个名字都能解析，**无需任何 postbuild 拷贝步骤**。

### Docker（NAS）

```bash
# 构建多架构镜像
docker buildx create --name epbx --use
docker buildx build --platform linux/amd64,linux/arm64 -t eigenpulse:0.1.0 --push .

# 运行
docker run --rm -p 3000:3000 -v ep-data:/data \
  -e EP_ADMIN_PASSWORD='changeme' \
  eigenpulse:0.1.0

# 或
EP_ADMIN_PASSWORD=changeme docker compose up -d
```

> distroless 容器以 `nonroot` (uid 65532) 运行。挂载主机目录时需 `chown -R 65532:65532 <host-path>`。
> 镜像内置 `HEALTHCHECK`，直接调用 `/app/eigenpulse --healthcheck` 探测
> `LEPTOS_SITE_ADDR` 对应的本机 `/healthz`，不依赖 curl/wget。

> **生产部署 / 运维**：反向代理 + TLS、完整环境变量参考、备份恢复、升级回滚见
> [`docs/ops.md`](docs/ops.md)。生产暴露前至少完成：反代 TLS + `EP_COOKIE_SECURE=1`、
> 设 `TZ` 为本地时区（否则「今日」/连续训练/热力图按 UTC 翻篇而错位）、配好离机备份。
> 已内置的硬化：`VACUUM INTO` 备份（手动 + 迁移前自动快照）、安全响应头 + CSP、
> HSTS（`EP_COOKIE_SECURE=1` 时）、登录暴力破解限流（默认 15 分钟内 5 次失败 → 429）。

### 环境变量

| 变量 | 必填 | 说明 |
|---|---|---|
| `EP_ADMIN_PASSWORD` | 首次启动 | OWNER 账户初始密码（≥6 字符）；之后可在 UI 中轮换 |
| `EP_SECRET` | 推荐 | ≥64 字符的 **session cookie 签名密钥**；缺失则首启生成 `EP_SECRET_FILE` 指向的文件（本地默认 `data/secret.key`，Docker 默认 `/data/secret.key`）。轮换会让所有浏览器登录失效，但**不影响 PAT** —— PAT 直接 `sha256(token)` 比对，与本变量无关。 |
| `EP_SECRET_FILE` | 否 | 未设置 `EP_SECRET` 时用于读取/持久化自动生成密钥；本地默认 `data/secret.key`，Docker 镜像内默认 `/data/secret.key`。文件不存在时会自动生成；文件已存在但内容无效时会拒绝启动，避免静默轮换会话签名密钥。 |
| `EP_COOKIE_SECURE` | 否 | 设为 `1` / `true` 时 session cookie 标记 `Secure`（仅 HTTPS 发送），并对页面响应附加 HSTS。**默认 false**：本地 HTTP / NAS 内网部署会话才能持久化。生产 HTTPS 环境务必设为 `1`。 |
| `DATABASE_URL` | 否 | 本地默认 `sqlite://data/eigenpulse.db?mode=rwc`；Docker 镜像内覆盖为 `sqlite:///data/eigenpulse.db?mode=rwc` |
| `TZ` | 生产建议 | 决定本地日/周/月边界（「今日」聚合、连续训练、热力图，经 SQLite `'localtime'`）。distroless 镜像无 tzdata，未设则按 **UTC** 翻篇；非 UTC 用户务必设为本地 IANA 区，如 `Asia/Shanghai`。 |
| `RUST_LOG` | 否 | tracing 过滤，例如 `info,sqlx=warn` |

---

## 通知系统

支持 5 种通道，统一 `Notifier` trait，由 `NotifyBus::dispatch` 一次性扇出：

| 通道 | 配置字段 (`config_json`) |
|---|---|
| **inapp** | `{}` — 自动写入 `notification` 表，铃铛实时 +1 (SSE) |
| **smtp** | `{host, port, username, password, from, to, starttls}` |
| **bark** | `{base_url, device_key, sound?, group?, icon_url?}` |
| **telegram** | `{bot_token, chat_id}` |
| **discord** | `{webhook_url}` |

在 `/settings/notifications` 中添加通道，可设置 `min_severity`（info/warn/crit）过滤阈值。

模块代码触发通知：
```rust
let n = ep_core::NotifyMessage::warn("餐饮预算已用 92%")
    .module("FIN")
    .body("建议本周在家用餐 3 次，预计节省 ¥240")
    .link("/finance");
let _ = state.notify.dispatch(n).await;
```

---

## 开放 API（`/api/v1/*`）

iOS Shortcuts / 脚本 / 第三方集成通过 **Personal Access Token** 鉴权：

1. 在 `/settings/security` 生成 PAT，**仅一次性展示完整值**（之后只显示 `ep_pat_xxxx` 前缀）。
2. 调用时附 `Authorization: Bearer ep_pat_…`。
3. 按 scope 控制权限：`activity:read` / `fin:read` / `fin:write` / `fit:read` / `fit:write` / `lrn:read` / `lrn:write` / `notify:write` / `*`。

### 端点

| 路径 | 方法 | scope | 说明 |
|---|---|---|---|
| `/api/v1/healthz` | GET | — | 健康检查 |
| `/api/v1/whoami` | GET | 任意有效 PAT | 当前 token 信息 |
| `/api/v1/notify` | POST | `notify:write` | 推送一条通知到所有渠道 |
| `/api/v1/today` | GET | `activity:read` | 今日跨模块聚合 |
| `/api/v1/fin/txn` | POST | `fin:write` | 记一笔交易，返回 `{doc_id}` |
| `/api/v1/fin/txn` | GET | `fin:read` | 列出最近 50 笔 |
| `/api/v1/fin/txn/{doc_id}` | PATCH / DELETE | `fin:write` | 修改或删除交易 |
| `/api/v1/fin/transfer` | POST | `fin:write` | 新建一组双边转账 |
| `/api/v1/fin/account` | GET / POST | `fin:read` / `fin:write` | 账户列表 / 新建账户 |
| `/api/v1/fin/account/{code}` | PATCH / DELETE | `fin:write` | 修改或删除账户 |
| `/api/v1/fin/category` | GET / POST | `fin:read` / `fin:write` | 分类列表 / 新建分类 |
| `/api/v1/fin/category/{code}` | PATCH / DELETE | `fin:write` | 修改或删除分类 |
| `/api/v1/fin/budget?period=YYYY-MM` | GET | `fin:read` | 预算列表 |
| `/api/v1/fin/budget` | POST | `fin:write` | 设置预算 |
| `/api/v1/fin/budget/{period}/{category_code}` | DELETE | `fin:write` | 删除预算 |
| `/api/v1/fit/workout` | POST | `fit:write` | 记录一次训练，返回 `{doc_id}` |
| `/api/v1/fit/workout` | GET | `fit:read` | 列出最近 50 次训练 |
| `/api/v1/fit/workout/{doc_id}` | PATCH / DELETE | `fit:write` | 修改或删除训练记录 |
| `/api/v1/lrn/note` | POST | `lrn:write` | 记录一条学习笔记，返回 `{doc_id}` |
| `/api/v1/lrn/note` | GET | `lrn:read` | 列出最近 50 条学习笔记 |
| `/api/v1/lrn/note/{doc_id}` | PATCH / DELETE | `lrn:write` | 修改或删除学习笔记 |
| `/api/v1/lrn/book` | GET / POST | `lrn:read` / `lrn:write` | 书籍列表 / 新建书籍 |
| `/api/v1/lrn/book/{doc_id}` | PATCH / DELETE | `lrn:write` | 修改或删除书籍 |
| `/api/v1/lrn/course` | GET / POST | `lrn:read` / `lrn:write` | 课程列表 / 新建课程 |
| `/api/v1/lrn/course/{doc_id}` | PATCH / DELETE | `lrn:write` | 修改或删除课程 |

PATCH 端点遵循统一语义：字段缺省表示保留原值；可空字段传 `null` 表示清空；传具体值表示覆盖。字符串字段也会按服务端规则 trim，空字符串通常等价于清空。

### iOS Shortcuts 示例

```bash
curl -X POST -H "Authorization: Bearer $EP_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"merchant":"Blue Bottle","category_code":"YOUR_CATEGORY","account_code":"YOUR_ACCOUNT","amount":-42,"tag":"exp"}' \
     https://eigenpulse.your-nas.local/api/v1/fin/txn
```

`category_code` / `account_code` 必须是你在 Finance 中创建的真实代码；完整 shortcut 文件见 `examples/shortcuts/`。

`/api/v1/fin/txn` 接受已带符号的金额：`tag="exp"` 时 `amount` 必须小于 0，`tag="inc"` 时 `amount` 必须大于 0。`tag="tfr"` 不允许走该单笔接口；可选 `linked_doc_id` 用于把这笔财务记录关联到 Fitness / Learning 等模块单号。校验错误统一返回：

```json
{ "error": { "code": "finance.err.amount_sign_invalid", "message": "..." } }
```

转账不要通过 `/api/v1/fin/txn` 发送单腿 `tag=tfr`，应使用成对接口：

```bash
curl -X POST -H "Authorization: Bearer $EP_TOKEN" \
     -H "Content-Type: application/json" \
     -d '{"from_account":"CHECKING","to_account":"SAVINGS","amount":500,"note":"monthly allocation"}' \
     https://eigenpulse.your-nas.local/api/v1/fin/transfer
```

该接口会自动生成一出一入两笔 `tfr` 记录并建立配对链接；删除任一腿都会回滚整组转账。

---

## PWA

- iOS Safari / Android Chrome 打开站点 → "添加到主屏幕"
- 启动后无浏览器 chrome（standalone display）
- Service Worker 对 `/pkg/*` 与 `/static/styles.css` 使用 network-first，对普通 `/static/*` 使用 stale-while-revalidate；HTML/navigation、API、SSE 与 `/static/theme-init.js` 始终走网络，避免登出、会话或首屏主题脚本变化后展示陈旧内容
- 推送通过 Bark（iOS）/ Telegram（Android）实现，无需 Web Push VAPID

更新策略：静态资源会在在线访问时后台刷新；大版本变更或需要清理旧离线缓存时，再升级 `assets/sw.js` 顶部的 `CACHE` 版本号，旧 cache 会在 `activate` 时清理。

---

## 添加新模块

1. `cargo new --lib modules/<new>` + 在工作区 `Cargo.toml` 加 `members`、`workspace.dependencies`。
2. 写 `migrations/001_<new>.sql`，所有表前缀 `<code>_`。
3. 在 `src/lib.rs` 中：
   - 普通 `mod view; pub use view::<New>View;` 保持 hydrate/SSR 都可见。
   - `Module` 实现、`MODULE` 静态和 Open API 放进 `#[cfg(feature = "ssr")]` 模块，避免 `axum/sqlx` 进入 hydrate 目标。
   - 如需开放 API，再覆盖 `open_api(state)` 并在模块 crate 的 `ssr` feature 里打开对应依赖。
4. 在 `app/Cargo.toml` 添加依赖，在 `app/src/main.rs` 中注册：
   ```rust
   let registry = ModuleRegistry::new()
       .with(ep_finance::MODULE)
       .with(ep_<new>::MODULE);
   ```
5. 在 `app/src/app.rs` 的 `<Routes>` 中加一行 `<Route path=path!("<new>") view=ep_<new>::<New>View/>`。
6. 在 `crates/ui/src/sidebar.rs` 的 `NAV` 数组里加一项。
7. 为新模块补至少一个 migration 烟测；如果有写操作，补 server-fn/API 层的输入规范化测试。

最小验收：

```bash
cargo check -p ep_<new> --no-default-features --locked
cargo check -p ep_<new> --features ssr --no-default-features --locked
cargo check -p eigenpulse --lib --target wasm32-unknown-unknown --features hydrate --no-default-features --locked
cargo test -p ep_<new> --lib --features ssr --no-default-features --locked
```

最后再跑 `cargo check --workspace --locked` 或 CI 同等检查。

---

## 性能预期（现代 NAS）

Eigenpulse 的目标是现代 NAS 上轻量、快速、可维护；不是按嵌入式设备做极限裁剪。下面指标用于监控和回归判断，功能与可维护性优先于为了少量体积收益而删减能力。

| 指标 | 目标 |
|---|---|
| 镜像大小（压缩） | < 60 MB |
| 空闲内存 RSS | < 100 MB |
| p95 页面延迟（暖缓存） | < 200 ms |
| WASM Hydration 包（gzip） | 监控项；历史 < 450 KB 可作为依赖泄漏排查线索，实际接受范围按功能价值和首屏体验复核 |
| 冷启动到 `/healthz` | < 1 s |

---

## 已知限制 & 后续

- 自定义模块构建器、命令面板等扩展入口尚未实现，因此 UI 中不展示对应的非功能按钮。
- 通知凭证以明文存于 SQLite。建议启用 NAS 的文件加密 / Volume 加密。
- 字体栈默认使用系统已安装字体，不依赖外部 CDN；如需统一 Inter / JetBrains Mono / Noto Serif SC 字形，可下载子集放入 `assets/fonts/` 并在 `styles.css` 末尾添加 `@font-face`。

---

## License

MIT
