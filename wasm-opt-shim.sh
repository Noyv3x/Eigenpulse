#!/bin/sh
# Pass-through "wasm-opt" shim used inside the Docker builder stage.
#
# Why: cargo-leptos 0.3.6 hard-codes binaryen v123 and only ships prebuilt
# binaries for x86_64 — on aarch64 it hangs while fetching its asset. The
# Debian-bookworm `binaryen` package would be a workable substitute on x86_64,
# but on aarch64 it predates the WASM features (sign-extension, mutable globals
# etc.) emitted by recent rustc, so it rejects the bundle with
# `unexpected false: all used features should be allowed`.
#
# This script preserves the input wasm verbatim. The Rust toolchain has
# already run LLVM `-Oz`; skipping wasm-opt costs only an extra ~5–10% bundle
# size, well within the 450KB-gzipped budget.
in=""
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o|--output) shift; out=$1 ;;
        -*) ;;
        *) in=$1 ;;
    esac
    shift
done
if [ -n "$in" ] && [ -n "$out" ]; then
    cp "$in" "$out"
fi
exit 0
