# Multi-stage Dockerfile for sv2d toolkit with minimal attack surface
FROM rust:1.75-slim as builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libsqlite3-dev \
    && rm -rf /var/lib/apt/lists/*

# Create app user for build
RUN useradd --create-home --shell /bin/bash app

# Set working directory
WORKDIR /app

# Copy source code
COPY . .

# Change ownership to app user
RUN chown -R app:app /app
USER app

# Build the applications
RUN cargo build --release --bin sv2d
RUN cargo build --release --bin sv2-cli
RUN cargo build --release --bin sv2-web

# Runtime stage - use distroless for minimal attack surface
FROM gcr.io/distroless/cc-debian12:latest

# Copy CA certificates for TLS
COPY --from=builder /etc/ssl/certs/ca-certificates.crt /etc/ssl/certs/

# Set working directory
WORKDIR /app

# Copy binaries from builder stage
COPY --from=builder /app/target/release/sv2d /usr/local/bin/
COPY --from=builder /app/target/release/sv2-cli /usr/local/bin/
COPY --from=builder /app/target/release/sv2-web /usr/local/bin/

# Copy configuration examples
COPY --from=builder /app/sv2-core/examples/*.toml /app/config/

# Create non-root user and switch to it
USER 1000:1000

# Health check
HEALTHCHECK --interval=30s --timeout=10s --start-period=5s --retries=3 \
    CMD ["/usr/local/bin/sv2-cli", "status", "--quiet"] || exit 1

# Default command
CMD ["/usr/local/bin/sv2d", "--config", "/app/config/sv2d.toml"]

# Expose default ports
EXPOSE 4254 8080

# Labels for metadata
LABEL org.opencontainers.image.title="SV2D Stratum V2 Toolkit"
LABEL org.opencontainers.image.description="Stratum V2 mining daemon and tools"
LABEL org.opencontainers.image.vendor="Stratum V2 Project"
LABEL org.opencontainers.image.licenses="MIT OR Apache-2.0"
LABEL org.opencontainers.image.source="https://github.com/stratum-mining/stratum"