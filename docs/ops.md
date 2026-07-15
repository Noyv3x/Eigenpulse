# Eigenpulse 0.1 运维手册 · Deployment & Ops Runbook

本文面向 NAS、家庭服务器和单机部署，覆盖持久数据、TLS 反向代理、环境变量、健康
检查、`.epbackup` 备份/恢复以及版本升级回滚。

## 0. 部署与数据目录

Eigenpulse 是单二进制、单 distroless 容器。Finance、Fitness 和 Journal 是独立业务
模块，但为便于个人部署共用一个 SQLite 文件；Fitness 动作示范媒体保存在文件系统。

| 路径 | 内容 |
|---|---|
| `/data/eigenpulse.db`（及 `-wal`/`-shm`） | schema generation 2 SQLite 数据库 |
| `/data/modules/fitness/media/objects/` | 随机 key 命名的 GIF/MP4/WebM 对象 |
| `/data/secret.key` | 未设置 `EP_SECRET` 时生成的 Cookie 签名密钥 |
| `/data/backups/*.epbackup` | 数据库 + Fitness 媒体便携备份 |
| `/data/backups/*.bak` | 自动迁移前 SQLite 安全快照，不包含媒体 |

`.epbackup` 不包含 `secret.key`。完整卷备份会同时保护数据库、媒体和签名密钥；只复制
`eigenpulse.db` 会漏掉 WAL 中尚未 checkpoint 的事务和所有动作媒体。

容器以 `nonroot`（uid 65532）运行。使用绑定挂载时先执行：

```bash
chown -R 65532:65532 /path/to/eigenpulse-data
```

镜像内置 `/app/eigenpulse --healthcheck`，它探测本机 `/readyz`，不依赖 curl/wget。
Compose 已配置该检查。外部编排使用：

- `/livez`：进程存活；
- `/readyz`：数据库与 hydration 产物已就绪。

## 1. 反向代理与 TLS

对公网或跨主机访问时，应在 Eigenpulse 前终止 TLS，并设置：

```dotenv
EP_COOKIE_SECURE=1
```

这会为 session Cookie 设置 `Secure`，并让应用发送 HSTS。纯 LAN HTTP 部署保持默认
`0`，否则浏览器不会回传 Cookie。

应用发送 CSP、`X-Content-Type-Options: nosniff`、`X-Frame-Options: DENY` 和
`Referrer-Policy: same-origin`。代理必须：

- 保留原始 `Host` 和外部 `X-Forwarded-Proto`；
- 覆盖而不是追加客户端提交的 `X-Forwarded-For`；
- 关闭 `/events/notifications` 的响应缓冲；
- 只把代理的精确直连 CIDR 写入 `EP_TRUSTED_PROXY_CIDRS`。

### Caddy

```caddyfile
eigenpulse.example.com {
    encode zstd gzip

    @sse path /events/*
    reverse_proxy @sse 127.0.0.1:3000 {
        flush_interval -1
    }

    reverse_proxy 127.0.0.1:3000
}
```

### nginx

```nginx
server {
    listen 443 ssl http2;
    server_name eigenpulse.example.com;

    ssl_certificate     /etc/letsencrypt/live/eigenpulse.example.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/eigenpulse.example.com/privkey.pem;

    location / {
        proxy_pass         http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header   Host              $http_host;
        proxy_set_header   X-Forwarded-For   $remote_addr;
        proxy_set_header   X-Forwarded-Proto $scheme;
    }

    location /events/ {
        proxy_pass         http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header   Host              $http_host;
        proxy_set_header   X-Forwarded-For   $remote_addr;
        proxy_set_header   X-Forwarded-Proto $scheme;
        proxy_buffering    off;
        proxy_cache        off;
        proxy_read_timeout 1h;
    }
}
```

宿主机代理经 compose 发布端口访问容器时，直连 peer 是 Docker bridge gateway。仓库
默认固定为 `172.30.96.1`：

```dotenv
EP_BIND_IP=127.0.0.1
EP_COOKIE_SECURE=1
EP_TRUSTED_PROXY_CIDRS=172.30.96.1/32
```

如修改 `EP_DOCKER_SUBNET` / `EP_DOCKER_GATEWAY`，同步修改 trusted CIDR。不要信任
`0.0.0.0/0` 或 `::/0`。

## 2. 环境变量

| 变量 | 必填 | 默认 | 说明 |
|---|---|---|---|
| `EP_ADMIN_PASSWORD` | 仅首次启动 | — | 创建 OWNER 的初始密码，至少 6 字符；账户存在后不再读取 |
| `EP_SECRET` | 推荐 | — | 至少 64 字符的 Cookie 签名密钥；轮换只会让浏览器 session 失效，不影响 PAT |
| `EP_SECRET_FILE` | 否 | 本地 `data/secret.key`；Docker `/data/secret.key` | 未设 `EP_SECRET` 时读取或生成 |
| `DATABASE_URL` | 否 | 本地 `sqlite://data/eigenpulse.db?mode=rwc`；Docker `sqlite:///data/eigenpulse.db?mode=rwc` | SQLite 连接串 |
| `EP_COOKIE_SECURE` | 否 | `0` | HTTPS 下设为 `1`；同时启用 Secure Cookie 和 HSTS |
| `EP_TRUSTED_PROXY_CIDRS` | 否 | 空 | 逗号分隔的直连反代 CIDR；无效或全网 CIDR 会拒绝启动 |
| `EP_MODULE_DATA_ROOT` | 否 | 本地 `data/modules`；Docker `/data/modules` | 模块二进制数据根目录 |
| `EP_FITNESS_MEDIA_MAX_FILE_BYTES` | 否 | `134217728`（128 MiB） | 单个 GIF/MP4/WebM 最大字节数 |
| `EP_FITNESS_MEDIA_QUOTA_BYTES` | 否 | `21474836480`（20 GiB） | Fitness 媒体总配额 |
| `EP_DELIVERY_RETENTION_DAYS` | 否 | `30` | 已结束通知投递记录保留天数，1–3650 |
| `EP_READ_NOTIFICATION_RETENTION_DAYS` | 否 | `365` | 已读站内通知保留天数，1–3650；未读不受影响 |
| `EP_ALLOW_UNBACKED_MIGRATION` | 仅应急 | off | 自动安全快照失败时仍继续迁移；正常部署不要启用 |
| `EP_ALLOW_INSECURE_FILE_PERMISSIONS` | 仅应急 | off | 仅供已有等效 ACL、但不支持 Unix mode 的文件系统 |
| `RUST_LOG` | 否 | `info` | tracing filter，例如 `info,sqlx=warn` |
| `LEPTOS_SITE_ADDR` | 否 | Docker `0.0.0.0:3000` | 监听地址和 CLI healthcheck 目标 |

时区不由部署环境变量控制。后端把真实时间点存为 UTC Unix 秒，把记账日、补录训练日和
日记日期存为业务纯日期。浏览器首次使用时自动检测 IANA 展示时区；需要固定其他地区时，
可在“设置 → 区域与时间”中手动覆盖，持久化后立即生效。

Fitness 动作媒体每个动作最多 12 个文件。浏览器用 multipart 字段 `media` 调用
`POST /fitness/media/exercises/:exercise_id`，可选文本字段为 `title`；读取/删除使用
`GET|HEAD|DELETE /fitness/media/:media_id`。写操作要求同源 `Origin`。上传过程流式检查
大小和配额、计算 SHA-256，并验证 GIF、MP4 或 WebM 结构。反向代理 body limit 必须
至少覆盖应用侧单文件上限；不要把对象目录作为静态目录公开。

`EP_SECRET` 只用于 `ep_sid` Cookie。PAT 保存为无密钥 `sha256(token)`；泄漏时应在
`/settings/security` 撤销对应 PAT，轮换 `EP_SECRET` 不会撤销它。

## 3. `.epbackup` 备份与恢复

### 3.1 归档内容与一致性

在 `/status` 创建的 `.epbackup` 是 Zip64 归档，包含：

- `database/eigenpulse.db`：`VACUUM INTO` 生成的一致、紧凑 SQLite 快照；
- `modules/fitness/media/objects/*`：全部动作媒体；
- `manifest.json`：格式版本、schema generation、创建时间，以及每个 payload 的大小
  和 SHA-256。

创建期间持有 Fitness 媒体变更锁，因此数据库元数据与对象树来自同一个受控窗口。
归档先写临时文件，校验并 fsync 后再原子发布。

### 3.2 取出备份

产物位于：

```text
/data/backups/eigenpulse-<timestamp>.epbackup
```

从 compose 卷查看或复制：

```bash
docker compose run --rm data-helper ls -la /data/backups
docker cp eigenpulse:/data/backups/eigenpulse-<timestamp>.epbackup ./
```

卷内备份与主数据同盘，不是离机备份。应使用 NAS 计划任务或 cron 把最新归档复制到
另一台机器或外部介质，并保存哈希：

```bash
sha256sum eigenpulse-<timestamp>.epbackup > eigenpulse-<timestamp>.epbackup.sha256
```

### 3.3 停机恢复

恢复会覆盖数据库和 Fitness 媒体，必须先停止应用。不要手工解压归档，也不要提前删除
SQLite 的 `-wal`/`-shm` sidecar。

原生二进制：

```bash
systemctl stop eigenpulse
DATABASE_URL='sqlite:///srv/eigenpulse/eigenpulse.db?mode=rwc' \
  /usr/local/bin/eigenpulse --restore /backup/eigenpulse.epbackup
systemctl start eigenpulse
```

Compose：

```bash
docker compose down
docker compose run --rm \
  -v "$PWD/eigenpulse.epbackup:/restore/input.epbackup:ro" \
  eigenpulse --restore /restore/input.epbackup
docker compose up -d
```

恢复程序会在 staging 中完成以下检查后才替换现有路径：

1. Zip entry 数量、路径和总大小限制，拒绝 Zip Slip/Zip Bomb；
2. manifest 格式和当前 schema generation；
3. 数据库与每个媒体对象的大小、SHA-256；
4. SQLite 文件头和 `PRAGMA quick_check`；
5. GIF/MP4/WebM 文件签名。

发布前会清空归档中的浏览器 session，并再次校验数据库和媒体索引。恢复使用持久化
日志处理进程中断。OWNER 密码哈希和 PAT 会保留，浏览器需重新登录。归档不包含
`secret.key`。

### 3.4 整卷冷备

停服后复制完整 `/data` 卷会额外包含 `secret.key`：

```bash
docker compose down
docker compose run --rm -v "$PWD":/backup data-helper \
  tar czf /backup/eigenpulse-data-$(date +%F).tar.gz -C /data .
docker compose up -d
```

直接在线复制 `eigenpulse.db` 不安全，也不会包含媒体。若使用 Litestream 降低数据库
RPO，仍需独立复制媒体对象树，并定期制作 `.epbackup` 验证两者能一起恢复。

## 4. 升级与回滚

### 4.1 常规升级

`0.2.0` 是 schema generation 2 的全新基线，不兼容更早的预生产数据卷或
`.epbackup`；部署本基线时必须使用空数据卷。后续版本才使用下面的常规迁移流程。

```bash
# .env 示例
EP_IMAGE=ghcr.io/<owner>/eigenpulse:v0.2.0

docker compose pull
# 先在 /status 创建并复制一份离机 .epbackup
docker compose up -d
```

检测到待执行的平台或模块迁移时，应用会先创建 SQLite 安全快照；快照失败会拒绝
迁移，除非显式设置 `EP_ALLOW_UNBACKED_MIGRATION=1`。`.bak` 不含 Fitness 媒体，可靠
回滚点始终是升级前的 `.epbackup`。

### 4.2 回滚

1. `docker compose down`。
2. 将 `.env` 中的 `EP_IMAGE` 改回原版本。
3. 用 [§3.3](#33-停机恢复) 的 `--restore` 恢复升级前 `.epbackup`。
4. `docker compose up -d`。

数据库归档和镜像必须成对回滚。SQLite 没有 down migration；不要用旧二进制直接
打开已被新版本迁移的数据库。

## 5. 安全检查清单

- [ ] 外部访问经 TLS，设置 `EP_COOKIE_SECURE=1`。
- [ ] 只信任反向代理的精确 `EP_TRUSTED_PROXY_CIDRS`，并禁止绕过代理直连。
- [ ] 首次登录后确认浏览器自动检测的展示时区，必要时在“设置 → 区域与时间”覆盖。
- [ ] `/data` 权限只允许 uid 65532 及受信管理员访问。
- [ ] 定期创建 `.epbackup`，复制离机并演练停机恢复。
- [ ] Fitness 媒体目录不由 nginx/Caddy 直接公开。
- [ ] PAT 使用最小 scope；泄漏后逐个撤销。
- [ ] SMTP/Bark/Telegram/Discord 凭证所在卷启用 NAS 加密或等效保护。

应用包含登录限流、Argon2 并发上限、Cookie 写请求同源校验、安全响应头、秘密 DTO
边界、通知持久化 outbox，以及媒体签名、路径和配额校验。第三方客户端错误只记录在
服务端，不把可能含 token 或 webhook URL 的原始错误返回浏览器。

## 6. 发布镜像

`.github/workflows/release.yml` 由 `v*` tag 触发，构建 amd64/arm64 镜像。只有仓库
变量 `ENABLE_RELEASE_PUSH=true` 且存在可用 registry token 时才推送；推送时执行
cosign keyless 签名并生成 SPDX SBOM。

```bash
git tag v0.2.0
git push origin v0.2.0

docker pull ghcr.io/<owner>/eigenpulse:v0.2.0
cosign verify ghcr.io/<owner>/eigenpulse:v0.2.0 \
  --certificate-identity-regexp 'https://github.com/<owner>/.*/release.yml@.*' \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
```

Compose 默认镜像为 `eigenpulse:0.2.0`。使用发布镜像时，将以下内容写入 `.env`：

```dotenv
EP_IMAGE=ghcr.io/<owner>/eigenpulse:v0.2.0
EP_ADMIN_PASSWORD=replace-on-first-boot
```

OWNER 创建后可删除 `EP_ADMIN_PASSWORD`。浏览器会自动检测展示时区；手动选择一经
持久化，后续应在设置页调整，容器重启不会覆盖它。
