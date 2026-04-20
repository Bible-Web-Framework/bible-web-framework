FROM lukemathwalker/cargo-chef:latest-rust-1 AS chef
WORKDIR /app
COPY rust-toolchain.toml rust-toolchain.toml

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
#RUN cat recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN cargo build --release --bin bwf-backend

FROM debian:bookworm-slim AS baker
WORKDIR /app
COPY ./bibles ./bibles
COPY --from=builder /app/target/release/bwf-backend .
ENV BIBLES_DIR="./bibles"
ENV BIBLE_BAKE="generate"
ENV BAKE_DIR="./bake"
RUN ./bwf-backend

FROM debian:bookworm-slim AS runtime
WORKDIR /app
COPY --from=builder /app/target/release/bwf-backend /usr/local/bin
COPY --from=baker /app/bake /app/bake
COPY ./deploy/bwf-deploy.db /app/bwf.db
ENV BIND_HOST="0.0.0.0"
ENV BIND_PORT="8080"
ENV BIBLE_BAKE="load"
ENV BAKE_DIR="/app/bake"
ENV DATABASE_URL="sqlite:///app/bwf.db?immutable=true"
ENV DATABASE_READ_ONLY="true"
ENTRYPOINT ["/usr/local/bin/bwf-backend"]
