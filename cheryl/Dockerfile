# ---------------------------------------------------
# Stage 1: The Builder (Heavy)
# ---------------------------------------------------
# FROM rust:1.80-slim-bookworm AS builder
FROM rust:1.94-slim-bookworm AS builder
WORKDIR /app

RUN apt-get update && apt-get install -y curl unzip && rm -rf /var/lib/apt/lists/*


# Copy the source code
COPY . .

# Build the release binary. 
# (If building cheryl, ensure sqlx-data.json is present - see below)
RUN cargo build --release

# ---------------------------------------------------
# Stage 2: The Runtime (Slim & Sandboxed)
# ---------------------------------------------------
FROM debian:bookworm-slim AS runtime

# Install TLS certificates (Required if your app sends HTTPS requests to OpenObserve)
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

# Create a dedicated, non-root system user
RUN addgroup --system --gid 1000 appgroup && \
    adduser --system --uid 1000 --gid 1000 --no-create-home appuser

# Copy ONLY the compiled binary from the builder stage
COPY --from=builder /app/target/release/cheryl /usr/local/bin/cheryl

# Lock down execution to the non-root user
USER appuser

# The app boots and waits for Docker Compose to inject environment variables
CMD ["cheryl"]
