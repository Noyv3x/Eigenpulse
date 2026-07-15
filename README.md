# Eigenpulse · Self-hosted Personal Hub

Eigenpulse 是面向个人与家庭服务器的自部署多功能入口。当前聚合三个彼此独立的
应用：**记账（Finance）**、**健身管理（Fitness）**和**日记（Journal）**。

三个应用共享登录、导航、通知、备份和部署基础设施，但不共享业务记录。它们只是被
编译进同一个 Rust 二进制，并从同一入口访问。

## 当前功能

### 记账

- 多币种、账户、收支分类和精确小单位金额存储
- 收入/支出流水、原子双边转账、月度预算
- 月度摘要、现金流组合图、消费分类环图、预算利用率和 CSV 导出
- 基于 PAT 的开放 API

### 健身管理

- 自定义动作库、器械、肌群、追踪方式、备注和归档
- 每个动作最多 12 个有序示范媒体，支持 GIF、MP4 和 WebM
- 可复用训练计划及每组次数、重量、时长、距离、RPE、休息配置
- 实时训练的开始、暂停、恢复、逐组保存、完成和放弃
- 快速补录、训练历史、身体数据、力量/训练量趋势和估算 1RM
- 52 周训练总览、每周目标、身体指标与按动作力量交互图表
- 公制/英制显示和个人纪录通知

### 日记

- 按日期记录标题、正文、心情和标签
- 关键词检索，以及条目的编辑、归档和恢复
- 月度记录节奏、年度日历热力图和近一年标签排行
- 独立摘要和基于 PAT 的开放 API

### 平台能力

- Argon2id 密码、签名 Cookie 会话和 Personal Access Token（PAT）
- 站内 SSE 通知，以及 SMTP、Bark、Telegram、Discord 外部通道
- 浅色/深色主题、舒适/紧凑密度和 PWA
- 真实时间点在后端统一存为 UTC Unix 秒；记账日、补录训练日和日记日期作为纯日期保存，
  跨时区切换不会漂移；浏览器默认自动检测 IANA 展示时区，也可在设置中手动覆盖
- 自托管 SVG 图表、精确 tooltip、键盘控件和无 JavaScript 数据表降级
- `.epbackup` 便携备份，包含一致性 SQLite 快照、Fitness 媒体和校验清单
- 单二进制、单 distroless 容器、amd64/arm64 NAS 部署

## 模块边界

Finance、Fitness 与 Journal 为了部署简单而使用同一个 SQLite 文件，但各自完整拥有
自己的业务模型：

- Finance 表使用 `fin_*` 前缀，外键和查询只能指向其他 `fin_*` 表。
- Fitness 表使用 `fit_*` 前缀，外键和查询只能指向其他 `fit_*` 表。
- Journal 表使用 `jrn_*` 前缀，外键和查询只能指向其他 `jrn_*` 表。
- 平台层只负责用户、会话、PAT、通知、模块迁移账本和备份。
- 不存在跨模块活动流、关联表、统一业务单号或跨模块报表。
- 首页分别加载三个模块的小型摘要；其中一个失败不会阻塞其他模块。
- 通知只携带来源、文本和安全的站内链接，不保存业务外键或记录 ID。

`app/src/modules.rs` 是唯一的编译期组合目录。首页、导航、PAT scope 展示和 SSR 模块
注册都从这里派生；未来模块仍是编译期增加，不是运行时插件。

## 技术栈

| 层 | 选型 |
|---|---|
| 前端 | Leptos 0.7（Rust SSR + WASM hydration） |
| 图表 | Apache ECharts 6.1（tree-shaken、SVG、自托管） |
| 后端 | axum + Tokio + `leptos_axum` |
| 数据库 | SQLite + sqlx runtime query，WAL 模式 |
| 鉴权 | Argon2id + 签名 Cookie session + PAT |
| 通知 | 持久化 outbox + SSE + SMTP/Bark/Telegram/Discord |
| 备份 | Zip64 `.epbackup` + `VACUUM INTO` + SHA-256 |
| 部署 | 单二进制、distroless、多架构容器 |

## 快速开始

### 本地开发

需要 Rust 1.88+、`wasm32-unknown-unknown` 和 cargo-leptos 0.3.6；Linux 还需要
`mold`。

```bash
rustup target add wasm32-unknown-unknown
cargo install cargo-leptos --locked --version 0.3.6
sudo apt install mold # Debian/Ubuntu；macOS/Windows 不需要

export EP_ADMIN_PASSWORD='dev-password'
cargo leptos watch
# http://127.0.0.1:3000
```

`DATABASE_URL` 默认是 `sqlite://data/eigenpulse.db?mode=rwc`。首次启动必须提供
`EP_ADMIN_PASSWORD`；OWNER 已存在后不再读取它。未设置 `EP_SECRET` 时，应用会在
`data/secret.key` 创建并持久化 Cookie 签名密钥。
首次使用时浏览器会自动检测 IANA 展示时区；也可在“设置 → 区域与时间”中手动覆盖，
配置会持久化到数据库并立即生效，无需重启容器。

### Docker / NAS

直接运行官方发布的 amd64/arm64 镜像：

```bash
docker run -d --name eigenpulse --restart unless-stopped \
  -p 3000:3000 -v ep-data:/data \
  -e EP_ADMIN_PASSWORD='replace-this' \
  ghcr.io/noyv3x/eigenpulse:v0.2.0
```

首次启动后访问 `http://127.0.0.1:3000`。也可以使用 `latest` 跟随最新稳定版；生产环境
建议固定版本标签。若要从当前源码本地构建：

```bash
docker build -t eigenpulse:0.2.0 .

docker run -d --name eigenpulse --restart unless-stopped \
  -p 3000:3000 -v ep-data:/data \
  -e EP_ADMIN_PASSWORD='replace-this' \
  eigenpulse:0.2.0
```

Compose 默认使用本地构建；也可设置
`EP_IMAGE=ghcr.io/noyv3x/eigenpulse:v0.2.0` 使用发布镜像。

`0.2.0` 是 schema generation 2 的全新基线，不接受更早的预生产数据卷或
`.epbackup`。首次部署请使用空数据卷；此后版本再按正常迁移流程升级。

distroless 容器以 uid 65532 运行。绑定宿主目录前执行：

```bash
chown -R 65532:65532 /path/to/eigenpulse-data
```

公网访问应放在 TLS 反向代理之后，并设置 `EP_COOKIE_SECURE=1`。完整环境变量、反代、
备份和恢复操作见 [运维手册](docs/ops.md)。

### 健康端点

- `GET /livez`：仅检查进程存活。
- `GET /readyz`：检查数据库和 hydration 产物，供容器与编排探针使用。
- `GET /api/v1/healthz`：开放 API 的公共健康端点。

## 开放 API

iOS Shortcuts、脚本和第三方工具通过 `Authorization: Bearer ep_pat_…` 调用。PAT 的
完整值只在生成时显示一次；服务端只保存 SHA-256 哈希。

可授权的 scope：

- `finance:read` / `finance:write`
- `fitness:read` / `fitness:write`
- `journal:read` / `journal:write`
- `notifications:write`
- `*`

业务资源和创建响应使用正整数 `id`。

| 路径 | 能力 |
|---|---|
| `/api/v1/healthz` | API 健康检查，无需 PAT |
| `/api/v1/whoami` | 当前 PAT 身份和 scope |
| `/api/v1/notifications` | 投递平台通知 |
| `/api/v1/finance/currencies` | 币种 |
| `/api/v1/finance/accounts` | 账户 |
| `/api/v1/finance/categories` | 分类 |
| `/api/v1/finance/transactions` | 收支流水和 keyset 分页 |
| `/api/v1/finance/transfers` | 原子双边转账 |
| `/api/v1/finance/budgets` | 月度预算 |
| `/api/v1/finance/summary` | 记账摘要 |
| `/api/v1/finance/reports/months` | 月度统计 |
| `/api/v1/finance/export.csv` | CSV 导出 |
| `/api/v1/fitness/exercises` | 动作库和媒体元数据 |
| `/api/v1/fitness/exercises/:id/media` | 单个动作的媒体元数据 |
| `/api/v1/fitness/exercises/:id/media/order` | 调整媒体顺序 |
| `/api/v1/fitness/plans` | 训练计划 |
| `/api/v1/fitness/sessions/active` | 当前实时训练 |
| `/api/v1/fitness/sessions/start` | 开始实时训练 |
| `/api/v1/fitness/sessions/:id/*` | 暂停、恢复、组记录、完成或放弃 |
| `/api/v1/fitness/workouts` | 训练历史 |
| `/api/v1/fitness/quick-log` | 快速补录 |
| `/api/v1/fitness/measurements` | 身体数据 |
| `/api/v1/fitness/summary` | 健身摘要 |
| `/api/v1/fitness/analytics/strength` | 力量趋势 |
| `/api/v1/fitness/analytics/body` | 身体趋势 |
| `/api/v1/journal/entries` | 分页日记摘要列表、检索和创建 |
| `/api/v1/journal/entries/:id` | 日记读取、更新、归档、恢复和删除 |
| `/api/v1/journal/summary` | 日记摘要 |

创建一笔支出：

```bash
curl -X POST "$EP_BASE/api/v1/finance/transactions" \
  -H "Authorization: Bearer $EP_TOKEN" \
  -H 'Content-Type: application/json' \
  -d '{
    "currency_id": 1,
    "merchant": "Coffee",
    "category_id": 2,
    "account_id": 3,
    "amount": "-42.00",
    "tag": "exp"
  }'
```

`amount` 是主单位十进制字符串，Finance 会按币种精度转换为准确的小单位整数。转账
必须调用 `/api/v1/finance/transfers`。更多可运行示例见
[examples/shortcuts](examples/shortcuts/README.md)。

## 健身动作媒体

Docker 中的动作示范文件保存在 `/data/modules/fitness/media/objects/`。SQLite 只保存
随机对象 key、媒体类型、大小、SHA-256、标题和顺序，不保存 BLOB、原始文件名或客户端
提供的路径。

- 支持 GIF、MP4、WebM，并检查文件结构而非只信任扩展名或 MIME。
- 视频使用 controls、muted、loop、playsinline 和 metadata preload，不自动播放。
- 读取支持 HEAD、Range 和 ETag，设置 `nosniff`，且不进入 Service Worker 缓存。
- Cookie 会话路由为 `POST /fitness/media/exercises/:exercise_id`（multipart 字段
  `media`，可选 `title`）和 `GET|HEAD|DELETE /fitness/media/:media_id`。
- `EP_FITNESS_MEDIA_MAX_FILE_BYTES` 默认 128 MiB；
  `EP_FITNESS_MEDIA_QUOTA_BYTES` 默认 20 GiB。
- 存储根由 `EP_MODULE_DATA_ROOT` 控制，本地默认 `data/modules`，Docker 为
  `/data/modules`。

## 备份与恢复

在 `/status` 创建并下载 `.epbackup`。归档包含：

- `VACUUM INTO` 生成的一致性 SQLite 快照；
- 全部 Fitness 动作媒体对象；
- 记录 schema generation、大小和 SHA-256 的 `manifest.json`。

恢复必须停服，并通过当前二进制离线执行：

```bash
eigenpulse --restore /path/to/backup.epbackup
```

恢复会在 staging 中验证路径、大小、哈希、媒体签名、SQLite `quick_check` 和 schema
generation，再替换数据。密码和 PAT 会保留，浏览器 session 会被清空。归档不包含
`secret.key`，因此仍应单独备份整个 `/data` 卷。

## 项目结构

```text
Eigenpulse/
├── app/                         # Web shell、组合目录、SSR binary
├── crates/
│   ├── core/                    # ModuleDescriptor/Module、AppState、平台协议
│   ├── auth/                    # Cookie session、PAT、中间件
│   ├── db/                      # pool、迁移门禁、.epbackup
│   ├── notify/                  # 通知总线、outbox 与外部通道
│   ├── api/                     # 平台 /api/v1 端点
│   ├── i18n/                    # 国际化
│   └── ui/                      # 共享 Leptos 组件
├── modules/
│   ├── finance/                 # 独立记账领域、UI、迁移与 API
│   ├── fitness/                 # 独立健身领域、UI、迁移与 API
│   └── journal/                 # 独立日记领域、UI、迁移与 API
├── assets/vendor/               # 已生成、自托管的浏览器图表产物与许可证
├── tools/charts/                # ECharts 精简 bundle 源码、构建脚本和测试
└── migrations/0001_platform.sql # schema generation 2 平台基线
```

模块迁移位于各自的 `modules/<name>/migrations/`。当前基线一旦提交即视为不可变；
后续演进增加新的有序 SQL 文件，不修改已经提交的迁移。

## 开发检查

```bash
cargo fmt --check
cargo check --workspace --locked
cargo check -p eigenpulse --features ssr --no-default-features --locked
cargo check -p eigenpulse --lib --target wasm32-unknown-unknown \
  --no-default-features --features hydrate --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p ep-finance --lib --features ssr --no-default-features --locked
cargo test -p ep-fitness --lib --features ssr --no-default-features --locked
cargo test -p ep-journal --lib --features ssr --no-default-features --locked

cd tools/charts
npm ci --no-audit --no-fund
npm test
npm run check:bundle
```

发布构建使用 `cargo leptos build --release`。Linux 工具链通过 `.cargo/config.toml` 使用
mold；Docker 多架构构建保留 `rust:1-bookworm`。

业务模块只向 `ep-ui::Chart` 提交高层、可序列化的图表规格；ECharts 不进入 Rust/WASM
依赖图。`assets/vendor/eigenpulse-charts-6.1.0.js` 由 `tools/charts` 可复现生成，修改图表
适配层后运行 `npm run build` 并一并提交产物。CI 会检查精确依赖版本、许可证、生成差异
和 320 KiB gzip 上限。每个图表的 HTML 数据表由 SSR 直接生成，即使脚本加载失败仍可
查看精确值。

## 增加未来模块

1. 新 crate 完整拥有其模型、`<prefix>_*` 表、视图、server functions 和 API。
2. 外键和业务查询只能指向本模块表；不得读取其他模块的业务表。
3. 导出 hydrate-safe `DESCRIPTOR`，SSR 侧实现 `Module` 并嵌入自己的迁移。
4. 在 `app/src/modules.rs` 的组合目录注册一次，并显式注册 Leptos 页面路由。
5. 为模块边界、SSR、hydrate、迁移和 API 输入增加测试。

Eigenpulse 当前只包含 Finance、Fitness 和 Journal，不提供运行时安装或卸载。
