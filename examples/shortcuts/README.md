# iOS Shortcuts × Eigenpulse 0.1

这里的 JSON 是可读配方，不是可直接导入的 `.shortcut` 二进制。它们帮助你在 iOS、
iPadOS 或 macOS Shortcuts 中搭建两个常用动作：记一笔支出和发送一条平台通知。

| 文件 | 用途 |
|---|---|
| `quick-expense.json` | `POST /api/v1/finance/transactions`，返回整数交易 `id` |
| `quick-notify.json` | `POST /api/v1/notifications`，返回整数通知 `id` |
| `test.sh` | 先在普通终端验证 PAT、Finance、Fitness 和 Journal API |

## 生成 PAT

登录后打开 `/settings/security`，生成一个只含所需权限的 PAT。可用 scope 是：

- `finance:read` / `finance:write`
- `fitness:read` / `fitness:write`
- `journal:read` / `journal:write`
- `notifications:write`
- `*`（不推荐给日常快捷指令）

完整 `ep_pat_…` 只显示一次。Eigenpulse 只保存 token 的 SHA-256；丢失后应撤销旧 PAT
并重新生成，而不是轮换 `EP_SECRET`。

## 先用 curl 验证

```bash
export EP_BASE='http://192.168.1.50:3000'
export EP_TOKEN='ep_pat_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'

./test.sh whoami
./test.sh currencies
./test.sh exercises
./test.sh journal-entries
```

要创建测试数据，再从列表中选择真实的整数 ID：

```bash
export EP_CURRENCY_ID=1
export EP_ACCOUNT_ID=2
export EP_CATEGORY_ID=3
./test.sh expense
./test.sh list-txn

export EP_EXERCISE_ID=4
./test.sh workout
./test.sh list-workout

export EP_JOURNAL_DATE=2026-07-12
./test.sh journal-create

./test.sh notify
```

新安装只预置主币种，不预置账户、分类或动作。应先在网页中创建它们。`401` 表示 PAT
无效或已撤销，`403` 表示 scope 不足。

## 在 Shortcuts 中创建“记一笔”

按 `quick-expense.json` 的 `steps` 顺序添加动作。核心是一个 **Get Contents of URL**：

- URL：`<EP_BASE>/api/v1/finance/transactions`
- Method：`POST`
- Header：`Authorization: Bearer <EP_TOKEN>`
- Body：JSON

示例请求：

```json
{
  "currency_id": 1,
  "merchant": "Blue Bottle",
  "category_id": 3,
  "account_id": 2,
  "amount": "-42.00",
  "tag": "exp",
  "note": "Latte"
}
```

`amount` 是主单位十进制字符串；支出必须为负数，收入必须为正数。响应是完整交易
对象，从其中读取整数 `id`。转账使用 `/api/v1/finance/transfers`，不能向交易接口发送
孤立的 `tfr`。Finance 数据不会与 Fitness 动作或训练建立关联。

## 在 Shortcuts 中创建“推送通知”

`quick-notify.json` 使用相同流程调用 `POST /api/v1/notifications`：

```json
{
  "title": "提醒事项",
  "body": "明早 7:00 训练",
  "severity": "info",
  "source": "shortcuts",
  "link": "/fitness"
}
```

`severity` 可为 `info`、`warn`、`crit`。`source` 只能使用 2–32 个小写字母、数字或
连字符；`link` 必须是安全的站内绝对路径。通知不接受业务记录 ID，也不会与 Finance、
Fitness 或 Journal 表建立外键。

## Fitness API 提示

`test.sh workout` 调用 `/api/v1/fitness/quick-log`，把一组完成结果写入已存在的动作。
更完整的 Fitness API 还包括：

- `/api/v1/fitness/exercises`：动作库及 GIF/MP4/WebM 元数据；
- `/api/v1/fitness/plans`：训练模板；
- `/api/v1/fitness/sessions/active`、`/api/v1/fitness/sessions/start` 及其
  pause/resume/finish/discard 子路径：实时训练；
- `/api/v1/fitness/workouts`、`/api/v1/fitness/measurements`、
  `/api/v1/fitness/analytics/strength`、`/api/v1/fitness/analytics/body`：历史和趋势。

实时会话的写操作携带 `expected_revision`。收到 `409` 时应重新 GET 活动会话并采用
最新 revision，不要盲目覆盖。媒体二进制上传走登录后的同源 Web UI，不通过 PAT
传递服务器对象路径。

## Journal API 提示

`test.sh journal-create` 创建一篇带日期、心情和标签的独立日记。列表接口
`/api/v1/journal/entries` 返回正文预览，并支持 `q`、`date_from`、`date_to`、
`include_archived`、`offset` 和 `limit` 查询；完整正文通过单篇资源
`/api/v1/journal/entries/:id` 读取。更新、归档与恢复都对单篇资源发送 `PATCH`，成功返回
204；删除也使用单篇资源。它们不与 Finance 或 Fitness 记录建立关系。

## 为什么不提供 `.shortcut`

`.shortcut` 是 Apple 签名的 plist，通常还会固化示例 URL 或 token。文本 JSON 配方
更容易审查、不会误带凭证，也能和 API 版本一起维护。
