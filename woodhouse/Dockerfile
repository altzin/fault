

# ---------------------------------------------------
# Stage 1: The Go Builder
# ---------------------------------------------------
FROM golang:1.27-alpine AS builder
WORKDIR /app

# Cache dependencies
COPY go.mod go.sum ./
RUN go mod download

# Copy the rest of the code
COPY . .

# Build a statically linked, stripped Go binary (removes C dependencies)
RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-w -s" -o woodhouse .

# ---------------------------------------------------
# Stage 2: The Minimal Chromedp Runtime
# ---------------------------------------------------
FROM alpine:latest AS runtime

# Install Chromium and the exact font/rendering libraries it needs to survive
RUN apk add --no-cache \
    chromium \
    nss \
    freetype \
    harfbuzz \
    ca-certificates \
    ttf-freefont \
    tzdata \
    font-noto

# Create a non-root user with UID 1000 using useradd
RUN useradd -m -u 1000 -s /bin/bash appuser

# Copy ONLY the compiled binary from the builder
COPY --from=builder /app/woodhouse /usr/local/bin/woodhouse

# Ensure chromedp knows where the Alpine chromium binary lives
ENV CHROME_BIN=/usr/bin/chromium-browser \
    CHROME_PATH=/usr/lib/chromium/

# Lock down execution to the non-root user
USER appuser

# Run the app
CMD ["woodhouse"]
