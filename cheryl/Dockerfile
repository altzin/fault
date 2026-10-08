
FROM rust:1.94-slim-bookworm AS builder
WORKDIR /app
ENV SQLX_OFFLINE=true

COPY Cargo.toml Cargo.lock ./

RUN mkdir -p src && printf 'fn main() {}\n' > src/main.rs

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo fetch --locked

COPY src ./src
COPY migrations ./migrations
COPY .sqlx ./.sqlx

RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/app/target \
    cargo build --release && \
    cp target/release/cheryl /usr/local/bin/cheryl

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/bin/cheryl /usr/local/bin/cheryl

CMD ["cheryl"]
