# iOS Shortcuts × Eigenpulse

Templates and curl reference for driving Eigenpulse from the iOS / iPadOS
**Shortcuts** app (and macOS, watchOS — same engine). The two flows here cover
the common day-to-day uses: jot down an expense, push a one-line notification.

## What's in here

| File | Purpose |
|---|---|
| `quick-expense.json` | Spec for the **"记一笔"** shortcut → `POST /api/v1/fin/txn` |
| `quick-notify.json`  | Spec for the **"推送通知"** shortcut → `POST /api/v1/notify` |
| `test.sh`            | curl reference — run it from a laptop first to verify your PAT |

The `*.json` files describe the shortcut step-by-step (action types, payload
shape) so you can recreate them in Shortcuts.app in under five minutes. We
deliberately don't ship binary `.shortcut` files: those are signed plists that
only Apple's tooling can produce, and a downloadable plist would have to bake
in an example PAT that immediately becomes a phishing risk.

## Prerequisites — generate a PAT

1. Sign in to your Eigenpulse instance (e.g. `http://<nas>:3000`).
2. Open **Settings → 安全管理** (`/settings/security`).
3. Click **生成 PAT**, give it a name (e.g. `iOS Shortcuts`), and grant the
   minimum scopes you need:
   - `activity:read` — read the cross-module Today feed
   - `fin:read` — list recent Finance transactions
   - `fin:write` — record expenses / income
   - `notify:write` — push notifications
   - `*` only if you want one token to do everything (less safe)
4. **Copy the token shown** (`ep_pat_…`). Eigenpulse stores only a SHA-256
   hash; you cannot retrieve the plaintext later. If you lose it, revoke and
   regenerate.

## Verify the token from a terminal first

Don't fight an iOS UI when curl can prove the server side in one line:

```bash
export EP_BASE='http://192.168.1.50:3000'      # your instance
export EP_TOKEN='ep_pat_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'
export EP_ACCOUNT_CODE='YOUR_ACCOUNT_CODE'     # existing Finance account
export EP_CATEGORY_CODE='YOUR_CATEGORY_CODE'   # existing Finance category

./test.sh whoami        # prints user + granted scopes
./test.sh today         # reads the Today feed
./test.sh list-txn      # lists recent Finance transactions
./test.sh expense       # posts a small test expense
./test.sh notify        # pushes a test notification
```

If `whoami` returns 401 → token bad / revoked. 403 → scope missing.
Any other curl failure → check `EP_BASE`, network, and `/healthz`
(unauthenticated) reachability first.

## Building the shortcut on iOS

The Shortcuts.app primitives we use:

- **Ask for Input** — collect a text/number from the user
- **Choose from Menu** — fixed list (categories, severity)
- **Get Contents of URL** — actual HTTP call, configured to POST JSON with
  the `Authorization: Bearer …` header
- **Show Result** / **Show Notification** — display the doc id (or error)

### Quick Expense — recreate from `quick-expense.json`

Open the JSON file alongside Shortcuts.app and add actions in the order
listed under `steps[]`. Each step's `action` field maps to the Shortcuts
action of the same name; `bind` is the variable name to assign the output
to so later steps can reference it.

The HTTP call (`Get Contents of URL`, step 11 in `quick-expense.json`) uses these settings:

- **URL**: `<EP_BASE>/api/v1/fin/txn`
- **Method**: `POST`
- **Headers**:
  - `Authorization: Bearer <EP_TOKEN>`
  - `Content-Type: application/json`
- **Request Body**: JSON, with the fields shown in the JSON spec's
  `request_body` section. Use **Magic Variables** to splice in the values
  collected by earlier Ask-for-Input / Menu steps.

After saving, run it once. If it works, drag it to your home screen / add it
to the iOS share sheet / give it a Siri phrase (`Hey Siri 记一笔`).

### Quick Notify — recreate from `quick-notify.json`

Same pattern, three Ask-for-Input prompts → one HTTP POST →
display the returned `id`.

## API reference (the endpoints these shortcuts call)

### `POST /api/v1/fin/txn` — record a transaction

Required scope: `fin:write` (for read use `fin:read`).

```jsonc
{
  "merchant":      "Blue Bottle · 上海",  // required, non-empty
  "category_code": "FOOD",                // existing fin_category.code
  "account_code":  "CASH",                // existing fin_account.code
  "amount":        -42.0,                 // negative = expense, positive = income
  "tag":           "exp",                 // exp / inc; transfers use /api/v1/fin/transfer
  "note":          "Latte · 16oz",        // optional
  "linked_doc_id": "FIT-S-0412",          // optional — cross-link to fitness/learning
  "occurred_at":   1745209320              // optional; unix seconds, defaults to now
}
```

Returns `200 { "doc_id": "FIN-26092" }`.

If the amount is below `-500.0` the server also fans out a "大额支出"
notification through every enabled channel — useful for "did I really mean to
swipe this" auditing.

### `POST /api/v1/notify` — push a notification

Required scope: `notify:write`.

```jsonc
{
  "title":    "提醒事项",        // required
  "body":     "明早 7:00 健身",  // optional
  "severity": "info",          // info | warn | crit (default: info)
  "module":   "FIT",           // optional — source module code
  "link":     "/fitness",      // optional — in-app absolute path only
  "doc_ref":  "FIT-S-0412"     // optional — single doc id
}
```

Returns `200 { "id": 42 }` and fans the message out across all enabled
notification channels (站内 / SMTP / Bark / Telegram / Discord) whose
`min_severity` allows it.

### `GET /api/v1/whoami` — sanity check

Any valid PAT can call this; no specific scope is required. Returns the bound
user and the token's granted scopes — useful as the first action of any new
shortcut to confirm the request actually authenticated.

```bash
curl -sH "Authorization: Bearer $EP_TOKEN" "$EP_BASE/api/v1/whoami" | jq .
# → {"user":{"handle":"admin","name":"Owner","role":"OWNER"},
#    "token":{"name":"iOS Shortcuts","scopes":["fin:write","notify:write"]}}
```

### `GET /api/v1/today` — today's activity feed

Requires `activity:read`. Returns the same items you see on the in-app
**Today** view. Handy for a Lock-Screen widget that just shows the latest doc id.

## Troubleshooting

**`401 unauthorized`** — token missing, malformed, expired, or revoked. Check
`/settings/security`; the **最近使用** column updates on every successful
request.

**`403 requires scope: ...`** — token doesn't have the needed scope. Generate a
new one (you can have many) with the right scopes; old one stays usable for what
it was authorized for.

**Shortcut hangs at "Get Contents of URL"** — `EP_BASE` not reachable from the
phone (most often: shortcut tested over cellular but server is LAN-only). Add
your NAS to a VPN / Tailscale / Cloudflare Tunnel and use that hostname
instead.

**`ServerFnError("ssr-only")`** — you accidentally pointed the shortcut at the
SSR server-fn endpoint (`/api/_internal/...`). Those use cookie auth and won't
accept Bearer tokens. The `/api/v1/*` endpoints documented above are the
right ones.

## Why no .shortcut bundle?

A `.shortcut` file is an Apple-signed plist; recreating one outside macOS is
brittle and a bundled file would have to embed a placeholder URL + token,
both of which are stale the moment they ship. The JSON specs here let
Shortcuts.app handle the signing while keeping the recipe under version
control alongside the API that backs it.
