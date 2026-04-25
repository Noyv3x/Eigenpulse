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
│   ├── finance/            # ✅ 完整：schema + 种子 + CRUD + 4 tab UI + open API
│   ├── fitness/            # 骨架：schema + 只读视图
│   ├── learning/           # 骨架：schema + 只读视图
│   └── mod_marketplace/    # 渲染模块卡片 + MOD-MTX-01 关联矩阵
├── app/                    # SSR binary + <App/> shell + 视图
├── app-client/             # cdylib (wasm32) — hydrate_body(App)
├── assets/                 # styles.css (设计稿逐字节复制) / manifest / sw.js / icons
├── migrations/             # 0001_init.sql (核心 + 通知 + PAT)
└── Dockerfile / docker-compose.yml
```

---

## 快速开始

### 本地开发

```bash
# `rust-toolchain.toml` pins stable; rustup picks it up automatically.
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --locked

export EP_ADMIN_PASSWORD='dev-password'
export EP_SECRET="$(openssl rand -hex 64)"
mkdir -p data
# DATABASE_URL has a built-in default (sqlite://data/eigenpulse.db?mode=rwc);
# `mode=rwc` auto-creates the .db file on first connect — no `sqlx database create` needed.

cargo leptos watch          # http://127.0.0.1:3000
```

> 首次启动会用 `EP_ADMIN_PASSWORD` 创建 OWNER 账户。该变量缺失则进程拒绝启动。

### Docker（NAS）

```bash
# 构建多架构镜像
docker buildx create --name epbx --use
docker buildx build --platform linux/amd64,linux/arm64 -t eigenpulse:0.1.0 --push .

# 运行
docker run --rm -p 3000:3000 -v ep-data:/data \
  -e EP_SECRET="$(openssl rand -hex 64)" \
  -e EP_ADMIN_PASSWORD='changeme' \
  eigenpulse:0.1.0

# 或
EP_SECRET=$(openssl rand -hex 64) EP_ADMIN_PASSWORD=changeme docker compose up -d
```

> distroless 容器以 `nonroot` (uid 65532) 运行。挂载主机目录时需 `chown -R 65532:65532 <host-path>`。

### 环境变量

| 变量 | 必填 | 说明 |
|---|---|---|
| `EP_ADMIN_PASSWORD` | 首次启动 | OWNER 账户初始密码（≥6 字符）；之后可在 UI 中轮换 |
| `EP_SECRET` | 推荐 | ≥64 字符的 **session cookie 签名密钥**；缺失则首启生成 `data/secret.key`。轮换会让所有浏览器登录失效，但**不影响 PAT** —— PAT 直接 `sha256(token)` 比对，与本变量无关。 |
| `EP_COOKIE_SECURE` | 否 | 设为 `1` / `true` 时 session cookie 标记 `Secure`（仅 HTTPS 发送）。**默认 false**：本地 HTTP / NAS 内网部署会话才能持久化。生产 HTTPS 环境务必设为 `1`。 |
| `DATABASE_URL` | 否 | 默认 `sqlite:///data/eigenpulse.db?mode=rwc` |
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
    .link("/finance/budget");
let _ = state.notify.dispatch(n).await;
```

---

## 开放 API（`/api/v1/*`）

iOS Shortcuts / 脚本 / 第三方集成通过 **Personal Access Token** 鉴权：

1. 在 `/settings/security` 生成 PAT，**仅一次性展示完整值**（之后只显示 `ep_pat_xxxx` 前缀）。
2. 调用时附 `Authorization: Bearer ep_pat_…`。
3. 按 scope 控制权限：`fin:read` / `fin:write` / `notify:write` / `*`。

### 端点

| 路径 | 方法 | scope | 说明 |
|---|---|---|---|
| `/api/v1/healthz` | GET | — | 健康检查 |
| `/api/v1/whoami` | GET | `*` | 当前 token 信息 |
| `/api/v1/notify` | POST | `notify:write` | 推送一条通知到所有渠道 |
| `/api/v1/today` | GET | `*` | 今日跨模块聚合 |
| `/api/v1/fin/txn` | POST | `fin:write` | 记一笔交易，返回 `{doc_id}` |
| `/api/v1/fin/txn` | GET | `fin:read` | 列出最近 50 笔 |

### iOS Shortcuts 示例

```bash
curl -X POST -H "Authorization: Bearer $EP_PAT" \
     -H "Content-Type: application/json" \
     -d '{"merchant":"Blue Bottle","category_code":"F&B","account_code":"ACC-01","amount":-42,"tag":"exp"}' \
     https://eigenpulse.your-nas.local/api/v1/fin/txn
```

完整 shortcut 文件见 `examples/shortcuts/`。

---

## PWA

- iOS Safari / Android Chrome 打开站点 → "添加到主屏幕"
- 启动后无浏览器 chrome（standalone display）
- Service Worker 缓存 app shell + 静态资源；API 与 SSE 直通网络
- 推送通过 Bark（iOS）/ Telegram（Android）实现，无需 Web Push VAPID

更新策略：每次发布升级 `assets/sw.js` 顶部的 `CACHE` 版本号，旧 cache 在 `activate` 时清理。

---

## 添加新模块

1. `cargo new --lib modules/<new>` + 在工作区 `Cargo.toml` 加 `members`、`workspace.dependencies`。
2. 写 `migrations/001_<new>.sql`，所有表前缀 `<code>_`。
3. 在 `src/lib.rs` 中：
   - `pub static MODULE: &dyn Module = &<New>Module;`
   - 实现 `Module` trait（migrations、routes、open_api、dashboard_widgets、links）
   - 视图组件在 `view.rs` 中以 `#[component] fn <New>View()` 暴露
4. 在 `app/Cargo.toml` 添加依赖，在 `app/src/main.rs` 中注册：
   ```rust
   let registry = ModuleRegistry::new()
       .with(ep_finance::MODULE)
       .with(ep_<new>::MODULE);
   ```
5. 在 `app/src/app.rs` 的 `<Routes>` 中加一行 `<Route path=path!("<new>") view=ep_<new>::<New>View/>`。
6. 在 `crates/ui/src/sidebar.rs` 的 `NAV` 数组里加一项。

完成。`cargo check` 通过即可启动。

---

## 性能预算（NAS DS920+ 级别）

| 指标 | 目标 |
|---|---|
| 镜像大小（压缩） | < 60 MB |
| 空闲内存 RSS | < 100 MB |
| p95 页面延迟（暖缓存） | < 200 ms |
| WASM Hydration 包（gzip） | < 450 KB |
| 冷启动到 `/healthz` | < 1 s |

---

## 已知限制 & 后续

- 自定义模块构建器（"+ 创建自定义模块" UI）暂未实现，先提供模板生成器路径。
- ⌘K 命令面板仅占位。
- 通知凭证以明文存于 SQLite。建议启用 NAS 的文件加密 / Volume 加密。
- 字体（Inter / JetBrains Mono / Noto Serif SC）目前从 Google Fonts CDN 加载；NAS 网络受限场景请下载子集放入 `assets/fonts/` 并在 `styles.css` 末尾添加 `@font-face`。

---

## License

MIT
