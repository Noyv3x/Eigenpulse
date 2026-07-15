#!/usr/bin/env bash
set -euo pipefail

wasm="${1:-target/site/pkg/eigenpulse.wasm}"
# The three-module 0.1 baseline measures about 739 KiB (756,797 bytes) with gzip -9.
# This is a leak ceiling with room for stable-toolchain codegen drift, not a
# target for removing useful behavior. Re-measure after hydrate changes.
limit="${EP_WASM_GZIP_LIMIT:-950000}"

if [ ! -f "$wasm" ]; then
  echo "WASM artifact not found: $wasm" >&2
  exit 1
fi

raw_bytes="$(wc -c < "$wasm" | tr -d ' ')"
gzip_bytes="$(gzip -9 -c "$wasm" | wc -c | tr -d ' ')"
echo "WASM size: raw=${raw_bytes}B gzip-9=${gzip_bytes}B limit=${limit}B"

if [ "$gzip_bytes" -gt "$limit" ]; then
  echo "Hydration bundle exceeds gzip budget by $((gzip_bytes - limit)) bytes" >&2
  exit 1
fi
