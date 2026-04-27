#!/bin/sh
# Reconcile the three-way disagreement on the hydrate-WASM filename:
#
#   - wasm-bindgen-emitted `<name>.js` fetches `<name>_bg.wasm`.
#   - leptos_meta::HydrationScripts renders a SSR
#     `<link rel="preload" href="/pkg/<name>_bg.wasm" as="fetch" ...>`.
#   - cargo-leptos 0.3.6 publishes the wasm artifact in the site dir as
#     `<name>.wasm` (no `_bg` suffix).
#
# Disk and the two HTML/JS references disagree, so every hydrating page
# 404s on the wasm download and the page silently degrades to its SSR
# snapshot — Tweaks toggle, ActionForm refetch, and SSE counter stop
# working. Editing only the JS won't help: the `<link preload>` in HTML
# still hard-codes `_bg.wasm`. Cheapest reconciliation is to `cp` the
# wasm into place under both names.
#
# Run after every `cargo leptos build`. Watch mode: re-run after each
# rebuild (cargo-leptos has no post-build hook in 0.3.6). Also invoked
# from the Dockerfile's builder stage.
#
# Usage: `scripts/leptos-postbuild.sh [SITE_PKG_DIR] [NAME]`
set -eu
PKG_DIR="${1:-target/site/pkg}"
NAME="${2:-eigenpulse}"
SRC="$PKG_DIR/$NAME.wasm"
DST="$PKG_DIR/${NAME}_bg.wasm"

if [ ! -f "$SRC" ]; then
    echo "leptos-postbuild: $SRC not found; nothing to do" >&2
    exit 0
fi
# Skip when the destination is already a fresh copy of the source —
# avoids needless writes during `cargo leptos watch` rebuilds.
if [ -f "$DST" ] && cmp -s "$SRC" "$DST"; then
    exit 0
fi
cp "$SRC" "$DST"
echo "leptos-postbuild: staged $DST as a copy of $SRC"
