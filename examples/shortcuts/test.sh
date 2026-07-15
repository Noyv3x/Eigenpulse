#!/usr/bin/env bash
# Eigenpulse 0.1 PAT API smoke helper.
# Requires curl; jq is optional.

set -euo pipefail

: "${EP_BASE:?set EP_BASE, e.g. EP_BASE=http://192.168.1.50:3000}"
: "${EP_TOKEN:?set EP_TOKEN to an ep_pat_ token}"

cmd=${1:-help}

pretty() {
    if command -v jq >/dev/null 2>&1; then
        jq .
    else
        cat
    fi
}

api() {
    local method=$1
    local path=$2
    local body=${3:-}
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

positive_id() {
    local name=$1
    local value=${!name:-}
    if [[ ! $value =~ ^[1-9][0-9]*$ ]]; then
        printf 'error: %s must be a positive integer id\n' "$name" >&2
        exit 2
    fi
}

case "$cmd" in
    whoami)
        api GET /api/v1/whoami | pretty
        ;;
    currencies)
        api GET /api/v1/finance/currencies | pretty
        ;;
    accounts)
        api GET /api/v1/finance/accounts | pretty
        ;;
    categories)
        api GET /api/v1/finance/categories | pretty
        ;;
    expense)
        positive_id EP_CURRENCY_ID
        positive_id EP_ACCOUNT_ID
        positive_id EP_CATEGORY_ID
        body=$(printf '{"currency_id":%s,"merchant":"shortcuts-test","category_id":%s,"account_id":%s,"amount":"-1.00","tag":"exp","note":"from examples/shortcuts/test.sh"}' \
            "$EP_CURRENCY_ID" "$EP_CATEGORY_ID" "$EP_ACCOUNT_ID")
        api POST /api/v1/finance/transactions "$body" | pretty
        ;;
    list-txn)
        positive_id EP_CURRENCY_ID
        api GET "/api/v1/finance/transactions?currency_id=$EP_CURRENCY_ID" | pretty
        ;;
    exercises)
        api GET /api/v1/fitness/exercises | pretty
        ;;
    workout)
        positive_id EP_EXERCISE_ID
        body=$(printf '{"notes":"from examples/shortcuts/test.sh","exercises":[{"exercise_id":%s,"new_exercise_name":null,"tracking_mode":null,"sets":[{"reps":10,"weight_g":null,"duration_s":null,"distance_m":null,"rpe_x10":70,"set_type":"working"}]}]}' \
            "$EP_EXERCISE_ID")
        api POST /api/v1/fitness/quick-log "$body" | pretty
        ;;
    list-workout)
        api GET /api/v1/fitness/workouts | pretty
        ;;
    fitness-summary)
        api GET /api/v1/fitness/summary | pretty
        ;;
    journal-entries)
        api GET /api/v1/journal/entries | pretty
        ;;
    journal-create)
        : "${EP_JOURNAL_DATE:?set EP_JOURNAL_DATE as YYYY-MM-DD}"
        body=$(printf '{"title":"Shortcuts test journal","body":"from examples/shortcuts/test.sh","entry_date":"%s","mood":"calm","tags":"shortcuts"}' \
            "$EP_JOURNAL_DATE")
        api POST /api/v1/journal/entries "$body" | pretty
        ;;
    journal-summary)
        api GET /api/v1/journal/summary | pretty
        ;;
    notify)
        body='{"title":"Shortcuts · 测试通知","body":"from examples/shortcuts/test.sh","severity":"info","source":"shortcuts"}'
        api POST /api/v1/notifications "$body" | pretty
        ;;
    help|*)
        cat <<EOF
usage: $0 <subcommand>

  whoami          verify PAT and list granted scopes
  currencies      list Finance currencies
  accounts        list Finance accounts
  categories      list Finance categories
  expense         create a small expense
  list-txn        list transactions for EP_CURRENCY_ID
  exercises       list Fitness exercises and media metadata
  workout         quick-log one completed set for EP_EXERCISE_ID
  list-workout    list completed Fitness workouts
  fitness-summary show the independent Fitness home summary
  journal-entries list active Journal entries
  journal-create  create a Journal entry for EP_JOURNAL_DATE
  journal-summary show the independent Journal home summary
  notify          send a platform notification

required environment for writes:
  expense: EP_CURRENCY_ID, EP_ACCOUNT_ID, EP_CATEGORY_ID
  workout: EP_EXERCISE_ID
  journal-create: EP_JOURNAL_DATE (YYYY-MM-DD)
EOF
        ;;
esac
