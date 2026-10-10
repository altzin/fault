FROM golang:1.27-alpine AS builder
WORKDIR /app

# Cache dependencies
COPY go.mod go.sum ./
RUN go mod download

# Copy the rest of the code
COPY . .

# Build a statically linked, stripped Go binary (removes C dependencies)
RUN CGO_ENABLED=0 GOOS=linux go build -ldflags="-w -s" -o woodhouse .


FROM alpine:latest AS runtime

# We keep ca-certificates just in case you ever make outbound HTTPS calls,
# but we deleted Chromium and the 300MB of fonts/libraries.
RUN apk add --no-cache ca-certificates

# FIX: Alpine uses `adduser -D` (Don't assign a password) instead of `useradd`
RUN adduser -D -u 1000 appuser

# Copy ONLY the compiled binary from the builder
COPY --from=builder /app/woodhouse /usr/local/bin/woodhouse

# Lock down execution to the non-root user
USER appuser

# Run the app
CMD ["woodhouse"]
