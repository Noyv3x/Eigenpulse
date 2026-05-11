#!/usr/bin/env bash
# Eigenpulse open-API smoke from a regular shell.
#
# Use this to verify your PAT works *before* fighting the iOS Shortcuts UI.
# Each subcommand mirrors what the equivalent `quick-*.json` shortcut posts.
#
#   export EP_BASE='http://192.168.1.50:3000'
#   export EP_TOKEN='ep_pat_xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'
#   export EP_ACCOUNT_CODE='YOUR_ACCOUNT_CODE'
#   export EP_CATEGORY_CODE='YOUR_CATEGORY_CODE'
#   ./test.sh whoami
#   ./test.sh today
#   ./test.sh list-txn
#   ./test.sh expense
#   ./test.sh workout
#   ./test.sh note
#   ./test.sh book
#   ./test.sh course
#   ./test.sh notify
#
# Requires: curl. `jq` is auto-detected and used for prettier output if present.

set -euo pipefail

: "${EP_BASE:?set EP_BASE, e.g. EP_BASE=http://192.168.1.50:3000}"
: "${EP_TOKEN:?set EP_TOKEN, e.g. EP_TOKEN=ep_pat_xxxxxxxx}"

cmd=${1:-help}

pretty() {
    if command -v jq >/dev/null 2>&1; then jq .; else cat; fi
}

api() {
    # api <METHOD> <path> [json-body]
    local method=$1 path=$2 body=${3:-}
    if [[ -n $body ]]; then
        curl -fsS -X "$method" \
            -H "Authorization: Bearer $EP_TOKEN" \
            -H "Content-Type: application/json" \
            -d "$body" \
            "$EP_BASE$path"
    else
        curl -fsS -X "$method" \
            -H "Authorization: Bearer $EP_TOKEN" \
            "$EP_BASE$path"
    fi
}

case "$cmd" in
    whoami)
        api GET /api/v1/whoami | pretty
        ;;
    today)
        api GET /api/v1/today | pretty
        ;;
    expense)
        : "${EP_ACCOUNT_CODE:?set EP_ACCOUNT_CODE to an existing finance account code}"
        : "${EP_CATEGORY_CODE:?set EP_CATEGORY_CODE to an existing expense category code}"
        # Tiny test transaction against your own account/category. Fresh
        # installs do not ship demo accounts/categories.
        body='{
            "merchant": "shortcuts-test",
            "category_code": "'"$EP_CATEGORY_CODE"'",
            "account_code": "'"$EP_ACCOUNT_CODE"'",
            "amount": -1.0,
            "tag": "exp",
            "note": "from examples/shortcuts/test.sh"
        }'
        api POST /api/v1/fin/txn "$body" | pretty
        ;;
    notify)
        body='{
            "title": "Shortcuts · 测试通知",
            "body":  "from examples/shortcuts/test.sh",
            "severity": "info"
        }'
        api POST /api/v1/notify "$body" | pretty
        ;;
    list-txn)
        api GET /api/v1/fin/txn | pretty
        ;;
    workout)
        body='{
            "occurred_on": "2026-05-10",
            "kind": "Shortcuts · Test Workout",
            "program": "API",
            "duration_m": 30,
            "load_text": "5km",
            "strain": "M",
            "rpe": 7,
            "notes": "from examples/shortcuts/test.sh"
        }'
        api POST /api/v1/fit/workout "$body" | pretty
        ;;
    list-workout)
        api GET /api/v1/fit/workout | pretty
        ;;
    note)
        body='{
            "title": "Shortcuts · Test Note",
            "body": "from examples/shortcuts/test.sh"
        }'
        api POST /api/v1/lrn/note "$body" | pretty
        ;;
    list-note)
        api GET /api/v1/lrn/note | pretty
        ;;
    book)
        body='{
            "name": "Shortcuts · Test Book",
            "author": "Eigenpulse",
            "status": "reading"
        }'
        api POST /api/v1/lrn/book "$body" | pretty
        ;;
    list-book)
        api GET /api/v1/lrn/book | pretty
        ;;
    course)
        body='{
            "name": "Shortcuts · Test Course",
            "provider": "Eigenpulse",
            "progress_pct": 25.0,
            "due_on": "2026-06-30",
            "tone": "amber"
        }'
        api POST /api/v1/lrn/course "$body" | pretty
        ;;
    list-course)
        api GET /api/v1/lrn/course | pretty
        ;;
    help|*)
        cat <<EOF
usage: $0 <subcommand>

  whoami       GET /api/v1/whoami       — verify token + see granted scopes
  today        GET /api/v1/today        — recent activity feed (needs activity:read)
  list-txn     GET /api/v1/fin/txn      — last 50 transactions (needs fin:read)
  expense      POST /api/v1/fin/txn     — create ¥1.00 test expense (needs fin:write)
  list-workout GET /api/v1/fit/workout  — last 50 workouts (needs fit:read)
  workout      POST /api/v1/fit/workout — create a test workout (needs fit:write)
  list-note    GET /api/v1/lrn/note     — last 50 notes (needs lrn:read)
  note         POST /api/v1/lrn/note    — create a test note (needs lrn:write)
  list-book    GET /api/v1/lrn/book     — last 50 books (needs lrn:read)
  book         POST /api/v1/lrn/book    — create a test book (needs lrn:write)
  list-course  GET /api/v1/lrn/course   — last 50 courses (needs lrn:read)
  course       POST /api/v1/lrn/course  — create a test course (needs lrn:write)
  notify       POST /api/v1/notify      — push an info notification (needs notify:write)

env:
  EP_BASE   = $EP_BASE
  EP_TOKEN  = ${EP_TOKEN:0:12}…   (sha-256 hashed server-side; rotate via /settings/security)
  EP_ACCOUNT_CODE   existing fin_account.code for expense
  EP_CATEGORY_CODE  existing fin_category.code for expense
EOF
        ;;
esac
