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
# already run LLVM `-Oz`; skipping wasm-opt costs some bundle size, but gzip
# size is monitored as a performance signal rather than treated as a hard gate.
set -eu

in=""
out=""
while [ $# -gt 0 ]; do
    case "$1" in
        -o|--output)
            shift
            if [ $# -eq 0 ]; then
                echo "wasm-opt-shim: missing argument for --output" >&2
                exit 2
            fi
            out=$1
            ;;
        -*) ;;
        *) in=$1 ;;
    esac
    shift
done
if [ -z "$in" ] || [ -z "$out" ]; then
    echo "wasm-opt-shim: expected input wasm and --output path" >&2
    exit 2
fi
if [ ! -f "$in" ]; then
    echo "wasm-opt-shim: input wasm not found: $in" >&2
    exit 1
fi
if [ "$in" != "$out" ]; then
    cp "$in" "$out"
fi
if [ ! -f "$out" ] || ! cmp -s "$in" "$out"; then
    echo "wasm-opt-shim: failed to preserve wasm at $out" >&2
    exit 1
fi
exit 0
