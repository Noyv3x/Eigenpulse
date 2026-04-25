# syntax=docker/dockerfile:1.7-labs
########## chef ##########
FROM --platform=$BUILDPLATFORM rust:1-bookworm AS chef
RUN apt-get update && apt-get install -y --no-install-recommends \
      pkg-config libssl-dev clang lld ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef --locked
RUN cargo install cargo-leptos --locked
RUN rustup target add wasm32-unknown-unknown
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
RUN cargo leptos build --release

########## runtime ##########
FROM gcr.io/distroless/cc-debian12:nonroot AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/eigenpulse /app/eigenpulse
COPY --from=builder /app/target/site /app/site
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
