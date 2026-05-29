# Eigenpulse 运维手册 · Deployment & Ops Runbook

部署、反向代理 / TLS、环境变量参考、备份恢复、升级回滚。
This runbook covers reverse-proxy + TLS, the full environment-variable
reference, backup/restore, and the upgrade/rollback procedure.

面向 NAS / 单机 / 家庭服务器部署。生产环境对外暴露前，请至少完成
[反向代理 + TLS](#1-反向代理--tls) 和 [备份](#3-备份与恢复--backup--restore) 两节。

---

## 0. 部署形态速览

Eigenpulse 是单二进制 + 单 distroless 容器。所有持久状态都在一个目录 `/data` 下：

| 路径 | 内容 |
|---|---|
| `/data/eigenpulse.db`（+ `-wal` / `-shm`） | 主 SQLite 数据库（WAL 模式） |
| `/data/secret.key` | 自动生成的 **session cookie 签名密钥**（未设 `EP_SECRET` 时）。丢失它会让所有浏览器会话失效，但不影响 PAT。 |
| `/data/eigenpulse.db.pre-migration-<schema_version>.bak` | 启动时迁移前自动快照（见 [§4](#4-升级与回滚--upgrade--rollback)） |
| `/data/backups/eigenpulse-v<user_version>.db` | 在 `/settings`「数据」面板手动触发的备份（见 [§3](#3-备份与恢复--backup--restore)） |

**只要完整备份 `/data` 卷，就备份了全部用户数据 + 会话密钥。**

容器以 `nonroot` (uid 65532) 运行。绑定宿主目录到 `/data` 时先
`chown -R 65532:65532 <host-path>`，否则容器无法写库。

镜像内置健康检查：`/app/eigenpulse --healthcheck` 直接探测本机
`LEPTOS_SITE_ADDR` 上的 `/healthz`，返回 `200 ok` 即健康（不依赖 curl/wget）。
Compose 已配置 `healthcheck`；`docker ps` 的 `STATUS` 列会显示 `healthy`。

---

## 1. 反向代理 + TLS

对公网或跨主机访问，**务必**在 Eigenpulse 前面放一个做 TLS 终止的反向代理，
并设置 `EP_COOKIE_SECURE=1`。这一个变量同时做三件事：

1. session cookie 标记 `Secure`（仅 HTTPS 发送）；
2. 应用对所有页面响应发送 `Strict-Transport-Security`（HSTS，`max-age=31536000; includeSubDomains`，不含 `preload`）；
3. 与 LAN/HTTP 默认行为区分开 —— 默认（未设或 `0`）**不会**标记 `Secure`、**不会**发 HSTS，这样内网 HTTP 访问时 cookie 才能持久化。

> 不要把 `EP_COOKIE_SECURE=1` 用在没有 TLS 的内网 HTTP 部署上：浏览器会拒绝
> 存 `Secure` cookie，导致每次刷新都掉登录。

应用本身已经默认发送一组安全响应头（无需代理补充，重复也无害）：
`Content-Security-Policy`（含内联 `theme-init.js` 的 sha256 白名单，不开
`'unsafe-inline'` 脚本）、`X-Content-Type-Options: nosniff`、
`X-Frame-Options: DENY`、`Referrer-Policy: same-origin`。HSTS 仅在
`EP_COOKIE_SECURE=1` 时附加。

代理需要转发的要点：
- 升级/转发 `Upgrade`/`Connection` 头不是必须的（应用用的是 **SSE**，不是
  WebSocket），但**必须关闭对 `/events/notifications` 的响应缓冲**，否则铃铛
  实时计数会卡住。
- 转发 `X-Forwarded-For`：登录限流按客户端 IP 计数（见 [§2](#2-环境变量参考--env-reference) `RUST_LOG` 旁注与 [§5](#5-安全加固一览--hardening)）。
- 透传原始 `Host`，并保持上游为明文 HTTP（容器内不跑 TLS）。

### Caddy（推荐，自动签发 / 续期证书）

```caddyfile
eigenpulse.example.com {
    encode zstd gzip

    # SSE 通知流：关闭 flush 缓冲，保持长连接实时推送。
    @sse path /events/*
    reverse_proxy @sse 127.0.0.1:3000 {
        flush_interval -1
    }

    reverse_proxy 127.0.0.1:3000
    # Caddy 自动管理 Let's Encrypt 证书并强制 HTTPS。
    # HSTS 由应用在 EP_COOKIE_SECURE=1 时发送，这里无需重复。
}
```

### nginx

```nginx
server {
    listen 443 ssl http2;
    server_name eigenpulse.example.com;

    ssl_certificate     /etc/letsencrypt/live/eigenpulse.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/eigenpulse.example.com/privkey.pem;

    # 应用已发送 HSTS（EP_COOKIE_SECURE=1 时）。若想在代理层统一兜底，可加：
    # add_header Strict-Transport-Security "max-age=31536000; includeSubDomains" always;

    location / {
        proxy_pass         http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header   Host              $host;
        proxy_set_header   X-Forwarded-For   $proxy_add_x_forwarded_for;
        proxy_set_header   X-Forwarded-Proto $scheme;
    }

    # SSE 通知流：关闭缓冲，保持实时。
    location /events/ {
        proxy_pass         http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header   Host $host;
        proxy_buffering    off;
        proxy_cache        off;
        proxy_read_timeout 1h;
    }
}

# 80 → 443 跳转
server {
    listen 80;
    server_name eigenpulse.example.com;
    return 301 https://$host$request_uri;
}
```

设好代理后，在容器环境里设 `EP_COOKIE_SECURE=1` 并重启。

---

## 2. 环境变量参考 · Env reference

| 变量 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `EP_ADMIN_PASSWORD` | 仅首次启动 | — | OWNER 账户初始密码（≥6 字符）。**仅在 `app_user` 为空（首启）时读取**；bootstrap 后该行常驻，变量再无作用。缺失 → 进程拒绝启动（设计如此）。轮换走 `/settings/security` 或恢复 CLI。 |
| `EP_SECRET` | 推荐 | — | ≥64 字符的 **session cookie 签名密钥**。未设则读取/生成 `EP_SECRET_FILE`。轮换会让所有浏览器登录失效，但 **PAT 不受影响**（PAT 是无密钥 `sha256(token)`）。**不要**用轮换 `EP_SECRET` 来吊销泄漏的 PAT —— 改去 `pat` 表把对应行 `revoked_at` 置位。 |
| `EP_SECRET_FILE` | 否 | 本地 `data/secret.key`；Docker `/data/secret.key` | 未设 `EP_SECRET` 时用于读取/持久化自动生成的密钥。文件不存在会自动生成；已存在但内容无效则拒绝启动（避免静默轮换会话密钥）。 |
| `EP_COOKIE_SECURE` | 否 | `0`（off） | `1`/`true` 时：session cookie 标记 `Secure` **且** 响应发送 HSTS。仅在 HTTPS（反代 TLS）下设为 `1`；内网 HTTP 保持默认。 |
| `DATABASE_URL` | 否 | 本地 `sqlite://data/eigenpulse.db?mode=rwc`；Docker `sqlite:///data/eigenpulse.db?mode=rwc` | SQLite 连接串。`mode=rwc` 首次打开时建库文件 + 父目录。备份目录是该文件父目录下的 `backups/`。 |
| `TZ` | **生产强烈建议** | `UTC`（容器内无 `/etc/localtime`） | **决定本地日/周/月边界**。应用用 SQLite `'localtime'` 计算「今日」聚合、Fitness 连续训练、Learning 热力图。distroless 镜像没有 tzdata 文件，未设 `TZ` → 全部按 UTC 滚动，非 UTC 用户的「今天」/连续天数/热力图会在错误的本地钟点翻篇。设为本地 IANA 区，如 `Asia/Shanghai`、`Europe/Berlin`。 |
| `RUST_LOG` | 否 | `info`（compose 已设） | tracing 过滤，例如 `info,sqlx=warn`。 |
| `LEPTOS_SITE_ADDR` | 否 | Docker `0.0.0.0:3000` | 监听地址。`--healthcheck` 也据此探测本机 `/healthz`。 |
| `LEPTOS_OUTPUT_NAME` | 否 | `eigenpulse` | hydration 包名前缀。Docker 已设，**通常不要改**。 |
| `LEPTOS_SITE_ROOT` | 否 | Docker `/app/site` | 静态资源（`pkg/`、`static/`）根目录。 |
| `LEPTOS_SITE_PKG_DIR` | 否 | `pkg` | `site_root` 下的 WASM/JS 子目录。 |

> `LEPTOS_*` 由 cargo-leptos 在 dev/release 构建时注入；运行容器镜像时 Dockerfile
> 已把它们固定为上表的值。除非你知道在做什么，不要在 compose 里覆盖它们。

---

## 3. 备份与恢复 · Backup & Restore

### 3.1 备份是怎么做的

Eigenpulse 用 SQLite 的 **`VACUUM INTO`** 生成备份快照（`crates/db/src/backup.rs`）。
相比直接 `cp` 数据库文件，`VACUUM INTO`：

- 产出一个**一致、紧凑**的副本（已合并 WAL 内容，无需单独拷 `-wal`/`-shm`）；
- 在线即可执行，无需停服。

两类快照：

1. **手动备份** —— 登录后在 `/settings` 的「数据」面板点「备份」（OWNER-only
   server fn `run_backup`）。落地到 `/data/backups/eigenpulse-v<user_version>.db`，
   文件名带 SQLite `user_version`，因此每个 schema 版本一个文件。状态面板会显示
   最近一次备份的路径、大小、是否存在，以及 `PRAGMA quick_check` 完整性结果。
2. **迁移前自动快照** —— 见 [§4](#4-升级与回滚--upgrade--rollback)。

### 3.2 把备份取出容器 / 定时离机

快照只在 `/data` 卷内，和主库同盘。**至少**定期把它复制到另一台机器/外部介质：

```bash
# 列出卷内备份
docker compose exec eigenpulse ls -la /data/backups

# 取一份到宿主机（容器名 eigenpulse）
docker cp eigenpulse:/data/backups/eigenpulse-v4.db ./eigenpulse-v4.db

# 或直接整卷快照（停服更稳，但 VACUUM INTO 出来的文件在线拷也安全）
docker run --rm -v ep-data:/data -v "$PWD":/backup busybox \
  tar czf /backup/ep-data-$(date +%F).tar.gz -C /data .
```

推荐用 cron / NAS 计划任务，每天 `docker cp` 最新 `eigenpulse-v*.db` 到异地。

### 3.3 恢复一份备份

> 恢复会**覆盖现有数据库**。先停服。

```bash
docker compose down

# 用一份备份替换主库，并清掉旧的 WAL/SHM（VACUUM INTO 产物自带完整数据）。
docker run --rm -v ep-data:/data -v "$PWD":/restore busybox sh -c '
  cp /restore/eigenpulse-v4.db /data/eigenpulse.db &&
  rm -f /data/eigenpulse.db-wal /data/eigenpulse.db-shm &&
  chown 65532:65532 /data/eigenpulse.db'

docker compose up -d
```

启动时 `open_pool` 会先跑 `PRAGMA quick_check`；若恢复的文件损坏，进程会
拒绝在其上跑迁移并报错退出 —— 这是有意的「快失败」。

恢复后 schema 版本若低于当前二进制，启动会**自动补跑缺失迁移**（并先打一份
迁移前快照，见下节）。

### 3.4（可选）Litestream 持续复制 —— 升级路径

内置的 `VACUUM INTO` 快照是**时点**备份；两次备份之间的写入有丢失窗口。若需要
近乎连续的复制（RPO≈秒级）到 S3 / MinIO / 另一磁盘，可外挂
[Litestream](https://litestream.io)（对 SQLite 透明，无需改应用）：

- 让 Litestream 持续复制 `/data/eigenpulse.db`（WAL 模式天然适配）。
- 典型做法：sidecar 容器或同卷的 litestream 进程，把 `replicate` 指向对象存储。
- 恢复用 `litestream restore` 重建库文件，再按 [§3.3](#33-恢复一份备份) 上线。

这是**可选增强**，当前镜像未内置 Litestream；上面的 `VACUUM INTO` + 离机
`docker cp` 对个人 NAS 已足够。需要更强 RPO 时再上 Litestream。

---

## 4. 升级与回滚 · Upgrade & Rollback

### 4.1 升级

```bash
docker compose pull          # 或重新 build 新镜像 tag
docker compose up -d
```

升级时若镜像带了新迁移，应用在打开连接池后、跑迁移前会**自动**把现有非空库
快照成 `/data/eigenpulse.db.pre-migration-<schema_version>.bak`
（`crates/db/src/pool.rs`）。文件名带迁移前的 `schema_version`，所以同版本重启
只是刷新这份快照。快照失败（如只读 FS）不致命，仅打 `warn` 日志后继续。

> 升级流程建议：先 [§3.1](#31-备份是怎么做的) 手动点一次「备份」（落到
> `backups/`，不会被下次迁移前快照覆盖），再 `up -d`。迁移前自动快照是兜底，
> 手动备份是你自己掌控的还原点。

迁移规则（开发者侧，运维了解即可）：已提交的迁移文件**永不可改**，schema 演进
靠新增 `00N_<reason>.sql`；sqlx 用字节校验和记录已应用迁移，改旧文件会在下次启动
触发 `VersionMismatch`。

### 4.2 回滚

如果新版本迁移后行为异常，需要回到升级前：

1. `docker compose down`
2. 用迁移前快照恢复主库（与 [§3.3](#33-恢复一份备份) 同理）：
   ```bash
   docker run --rm -v ep-data:/data busybox sh -c '
     cp /data/eigenpulse.db.pre-migration-<N>.bak /data/eigenpulse.db &&
     rm -f /data/eigenpulse.db-wal /data/eigenpulse.db-shm &&
     chown 65532:65532 /data/eigenpulse.db'
   ```
   （`<N>` 是回滚目标对应的 schema 版本；状态面板/日志里能看到。）
3. 把镜像 tag 切回**旧版本**再 `docker compose up -d`。

> 关键：回滚数据库**必须**同时把镜像降回旧版本。新二进制 + 旧库会再次跑迁移，
> 等于没回滚。降级镜像后旧库 schema 与旧二进制匹配，正常启动。
> SQLite 没有「down 迁移」—— 回滚靠**还原迁移前快照**，不是反向 SQL。

---

## 5. 安全加固一览 · Hardening

生产暴露前的清单：

- [ ] 反向代理做 TLS 终止，`EP_COOKIE_SECURE=1`（→ Secure cookie + HSTS）。见 [§1](#1-反向代理--tls)。
- [ ] 设 `TZ` 为本地区，否则日界/连续/热力图错位。见 [§2](#2-环境变量参考--env-reference)。
- [ ] 设 `EP_SECRET`（或确认 `EP_SECRET_FILE` 在持久卷上）以免重启掉登录。
- [ ] 配好离机备份（cron `docker cp` 或 Litestream）。见 [§3](#3-备份与恢复--backup--restore)。
- [ ] 绑定挂载 `/data` 时 `chown -R 65532:65532`。

已内置、无需配置的硬化：

- **登录限流**：登录 POST 按客户端 IP 做内存级暴力破解限流，默认 **15 分钟窗口内
  5 次失败**；超限返回 `429 Too Many Requests` + `Retry-After`。登录成功会重置该
  IP 计数（`crates/auth/src/login_guard.rs`、`app/src/login.rs`）。注意：限流按
  axum 看到的对端地址计数，务必让反代透传 `X-Forwarded-For` 并据此识别真实客户端。
- **CSRF**：登录表单用双提交 cookie（`ep_csrf`，signed + HttpOnly）+ 表单字段校验。
- **安全响应头 / CSP**：见 [§1](#1-反向代理--tls)。CSP 不开 `'unsafe-inline'` 脚本；
  唯一内联脚本（`theme-init.js`）按 sha256 白名单。
- **Argon2id** 密码哈希，在 `spawn_blocking` 中验证（NAS 级 CPU 上约 150–250ms）。
- **密钥卫生**：server fn / 开放 API 返回的 DTO 一律不带密钥列
  （`pat.hash`、`notify_channel.config_json`、`app_user.password_hash`）；数据导出
  也在源头剔除这些列。

> 注意：通知通道凭证（SMTP 密码、Bark device key、Telegram bot token、Discord
> webhook）以明文存于 SQLite。建议启用 NAS 的卷加密 / 文件加密保护 `/data`。

---

## 6. 发布镜像 · Releasing

发布流水线在 `.github/workflows/release.yml`，由 **`v*` 语义化版本 tag** 触发。
它复用与 CI `docker` job 相同的 `Dockerfile`（同一份 wasm-opt shim / mold /
多架构 workaround），用 QEMU + buildx 构建 `linux/amd64,linux/arm64`，
然后**可选**推送到 GHCR、用 cosign 无密钥签名、并附 syft 生成的 SBOM。

The release pipeline lives in `.github/workflows/release.yml` and is triggered
by a **semver `v*` tag push**. It reuses the same `Dockerfile` as the CI
`docker` job (identical wasm-opt shim / mold / multi-arch workarounds), builds
`linux/amd64,linux/arm64` via QEMU + buildx, then *optionally* pushes to GHCR,
signs with cosign (keyless), and attaches a syft SBOM.

### 6.1 发布是默认安全的（gate）· Publishing is opt-in

未配置任何东西时，tag 触发的 workflow 只做**构建期多架构校验**（`push: false`），
不碰任何 registry —— 与 CI `docker` job 同等安全。推送 / 签名 / SBOM 这些步骤
统一由 `gate` step 输出的一个布尔门控，门控为「真」需要**同时满足**：

1. repo Actions **变量** `ENABLE_RELEASE_PUSH` 设为 `true`；**且**
2. 存在可用的 registry token（`GHCR_TOKEN` PAT 优先，否则用内置 `GITHUB_TOKEN`）。

任一不满足 → workflow 退回 build-only，fork / dry-run tag 不会误发布。

### 6.2 维护者需要配置的 GitHub 仓库设置 · Required repo settings

要启用发布，在仓库 **Settings → Secrets and variables → Actions** 配置：

| 类型 | 名称 | 必填 | 用途 |
|---|---|---|---|
| **Variable** | `ENABLE_RELEASE_PUSH` | 启用推送必填 | 设为 `true` 打开发布门控。其他值 / 未设 → build-only。 |
| **Secret** | `GHCR_TOKEN` | 否（推荐用于跨 org / 私有发布） | `write:packages` 的 PAT。设了就优先用它登录 GHCR；不设则用内置 `GITHUB_TOKEN`。 |
| **Secret** | `COSIGN_PRIVATE_KEY` | 否 | 仅当改用 **基于密钥**的 cosign 签名时需要（见 6.5）。默认走 OIDC 无密钥签名，**无需**该 secret。 |

权限（**Permissions**）—— workflow 的 `release` job 已在 `permissions:` 块内声明，
无需在 UI 额外勾选，但仓库设置不能更严格地撤销它们：

- `contents: read` —— 检出代码。
- `packages: write` —— 用内置 `GITHUB_TOKEN` 推送 GHCR。
  若 **Settings → Actions → General → Workflow permissions** 设成了
  “Read repository contents permission”，需要改为允许 job 级
  `packages: write`（默认 GitHub 允许 job 在 `permissions:` 块内提升）。
- `id-token: write` —— cosign 无密钥签名向 Sigstore 申请 OIDC token。

> 推送的镜像首次会创建一个 GHCR package，默认继承仓库可见性。私有仓库 → 私有
> 镜像；要公开拉取需在 package 设置里改 visibility 为 public，并（可选）把
> package 链接回本仓库。

### 6.3 切一个版本 · Cutting a release

```bash
# 在干净的 main（或发布分支）上，确认版本号无误后：
git tag v0.1.0
git push origin v0.1.0
```

tag push 即触发 `release` workflow。门控打开时产物：

- `ghcr.io/<owner>/eigenpulse:v0.1.0` 和 `:latest`（多架构 manifest）；
- cosign 签名（keyless，记录在 Rekor 透明日志）；
- SPDX SBOM attestation（同时作为 workflow artifact `sbom-v0.1.0` 上传）。

`workflow_dispatch` 也能手动跑（无 tag 时镜像 tag 退化为 `manual-<sha7>`），
便于在打正式 tag 前演练一遍构建。

### 6.4 拉取并运行已发布镜像 · Pull & run the published image

```bash
# 拉取（私有镜像需先登录：echo <PAT> | docker login ghcr.io -u <user> --password-stdin）
docker pull ghcr.io/<owner>/eigenpulse:v0.1.0

# 最小运行（首启需要 EP_ADMIN_PASSWORD；生产参数见 §1/§2）
docker run -d --name eigenpulse \
  -p 3000:3000 \
  -v ep-data:/data \
  -e EP_ADMIN_PASSWORD='<first-boot-owner-password>' \
  -e TZ='Asia/Shanghai' \
  ghcr.io/<owner>/eigenpulse:v0.1.0
```

生产部署请用 compose + 反代 + `EP_COOKIE_SECURE=1`，见 [§1](#1-反向代理--tls)、
[§2](#2-环境变量参考--env-reference)。绑定宿主目录到 `/data` 时记得
`chown -R 65532:65532`。

### 6.5 验证签名与 SBOM · Verifying signature & SBOM

无密钥签名可用 cosign 按 OIDC 身份校验（identity = 触发该 tag 的 workflow，
issuer = GitHub Actions OIDC）：

```bash
cosign verify ghcr.io/<owner>/eigenpulse:v0.1.0 \
  --certificate-identity-regexp "https://github.com/<owner>/.*/release.yml@.*" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com

# 取出 SBOM attestation（SPDX JSON）
cosign verify-attestation --type spdxjson \
  --certificate-identity-regexp "https://github.com/<owner>/.*/release.yml@.*" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  ghcr.io/<owner>/eigenpulse:v0.1.0
```

> 若改用**基于密钥**的签名：设 `COSIGN_PRIVATE_KEY` secret，把 workflow 里
> `cosign sign --yes "${IMAGE}@${DIGEST}"` 换成
> `cosign sign --key env://COSIGN_PRIVATE_KEY --yes "${IMAGE}@${DIGEST}"`
> （workflow 内已注释标注），验证时用 `cosign verify --key <pub.key>`。

> 实际推送 / 签名 / SBOM 只能由具备上述 secrets/permissions 的维护者在真实
> 仓库里跑通；本仓库未配置时 workflow 仅做构建校验。
