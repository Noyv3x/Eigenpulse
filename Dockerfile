# syntax=docker/dockerfile:1.7-labs
########## chef ##########
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev clang lld ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
RUN cargo install cargo-leptos --locked
RUN rustup target add wasm32-unknown-unknown
# wasm-opt no-op shim. cargo-leptos 0.3.6 always tries to run wasm-opt and has
# no opt-out flag. Its bundled downloader (LEPTOS_WASM_OPT_VERSION=version_123/
# 129) has no aarch64 prebuilt and hangs on --platform linux/arm64. The Debian
# `binaryen` apt package is too old to validate sign-ext WASM features Rust
# 1.95 emits ("unexpected false: all used features should be allowed"). The
# rustc LLVM pass already runs -Oz; the bundle is ~1.2 MB raw vs ~815 KB after
# wasm-opt -Oz, but gzip closes most of that gap.
COPY wasm-opt-shim.sh /usr/local/cargo/bin/wasm-opt-version_123/wasm-opt
RUN chmod +x /usr/local/cargo/bin/wasm-opt-version_123/wasm-opt
WORKDIR /app

########## planner ##########
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

########## builder ##########
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
RUN cargo chef cook --release --target wasm32-unknown-unknown --recipe-path recipe.json
COPY . .
# Build + reconcile the wasm filename mismatch (cargo-leptos publishes
# `<name>.wasm`, but both the wasm-bindgen JS loader and Leptos's
# HydrationScripts preload `<name>_bg.wasm`). See scripts/leptos-postbuild.sh
# for the full rationale.
RUN cargo leptos build --release \
 && /app/scripts/leptos-postbuild.sh /app/target/site/pkg eigenpulse
# Pre-create an empty data directory we can copy into the runtime stage
# with nonroot ownership (distroless has no shell to mkdir/chown at runtime).
RUN mkdir -p /data-empty

########## runtime ##########
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/eigenpulse /app/eigenpulse
COPY --from=builder /app/target/site /app/site
COPY --from=builder --chown=nonroot:nonroot /data-empty /data
ENV LEPTOS_OUTPUT_NAME=eigenpulse \
    LEPTOS_SITE_ROOT=/app/site \
    LEPTOS_SITE_PKG_DIR=pkg \
    LEPTOS_SITE_ADDR=0.0.0.0:3000 \
    DATABASE_URL=sqlite:///data/eigenpulse.db?mode=rwc \
    RUST_LOG=info
VOLUME ["/data"]
EXPOSE 3000
USER nonroot:nonroot
ENTRYPOINT ["/app/eigenpulse"]
