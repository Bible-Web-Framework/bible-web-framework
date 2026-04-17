FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
# Build dependencies - this is the caching Docker layer!
RUN cargo chef cook --release --recipe-path recipe.json
# Build application
COPY . .
ENV BIBLES_DIR="./bibles"
ENV BIBLE_BAKE="generate"
ENV BAKE_DIR="./bake"
ENV VERGEN_IDEMPOTENT="true"
RUN cargo run --release --bin bwf-backend

# We do not need the Rust toolchain to run the binary!
FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/bwf-backend /usr/local/bin
COPY --from=builder /app/bake /app/bake
COPY ./deploy/bwf-deploy.db /app/bwf.db
ENV BIND_HOST="0.0.0.0"
ENV BIND_PORT="8080"
ENV BIBLE_BAKE="load"
ENV BAKE_DIR="/app/bake"
ENV DATABASE_URL="sqlite:///app/bwf.db?immutable=true"
ENV DATABASE_READ_ONLY="true"
ENTRYPOINT ["/usr/local/bin/bwf-backend"]
