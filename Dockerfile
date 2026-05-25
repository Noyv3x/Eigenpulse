# syntax=docker/dockerfile:1.7-labs
########## chef ##########
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS chef
# `mold` is required because `.cargo/config.toml` pins `-fuse-ld=mold` for
# Linux-GNU targets — without it BFD `ld` peaks > 8 GB RSS on a release link
# of this workspace. Bookworm ships mold 1.10.x in main, recent enough for
# both x86_64 and aarch64 host arches.
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev clang lld mold ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
RUN cargo install cargo-leptos --locked --version 0.3.6
RUN rustup target add wasm32-unknown-unknown
# wasm-opt no-op shim. cargo-leptos 0.3.6 always tries to run wasm-opt and has
# no opt-out flag. Its bundled downloader (LEPTOS_WASM_OPT_VERSION version_123
# in the pinned release; version_129 in adjacent cargo-leptos builds) has no
# aarch64 prebuilt and hangs on --platform linux/arm64. The Debian
# `binaryen` apt package is too old to validate sign-ext WASM features Rust
# 1.95 emits ("unexpected false: all used features should be allowed"). The
# rustc LLVM already runs -Oz; gzip size is monitored as a performance signal,
# not used as a hard gate.
COPY wasm-opt-shim.sh /usr/local/cargo/bin/wasm-opt-version_123/wasm-opt
COPY wasm-opt-shim.sh /usr/local/cargo/bin/wasm-opt-version_129/wasm-opt
RUN chmod +x /usr/local/cargo/bin/wasm-opt-version_123/wasm-opt \
    /usr/local/cargo/bin/wasm-opt-version_129/wasm-opt
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
# The SSR binary serves `/pkg/<name>_bg.wasm` from `<name>.wasm` via a
# ServeDir fallback (app/src/main.rs), so the hydration bundle resolves
# whatever name the loader requests — no postbuild copy step needed.
RUN cargo leptos build --release
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
    EP_SECRET_FILE=/data/secret.key \
    RUST_LOG=info
VOLUME ["/data"]
EXPOSE 3000
USER nonroot:nonroot
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
  CMD ["/app/eigenpulse", "--healthcheck"]
ENTRYPOINT ["/app/eigenpulse"]
