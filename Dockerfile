# Build stage
FROM rust:1.94-slim AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    libpq-dev \
    build-essential \
    g++ \
    && rm -rf /var/lib/apt/lists/*

# Copy Cargo files (but not the lock file)
COPY Cargo.toml ./

# Generate a new lock file compatible with this Rust version
RUN cargo update

# Enable SQLX offline mode
ENV SQLX_OFFLINE=true

# Create dummy sources to build dependencies
RUN mkdir -p src/bin/asenix \
    && echo "fn main() {}" > src/main.rs \
    && echo "fn main() {}" > src/bin/asenix/main.rs
RUN cargo build --release && rm -rf src

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build the application
RUN cargo build --release

# Runtime stage
FROM rust:1.94-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    libssl3 \
    libpq5 \
    libstdc++6 \
    ca-certificates \
    curl \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd --create-home --shell /bin/bash asenix

WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/asenix-server /usr/local/bin/asenix-server

# Copy migrations
COPY --from=builder /app/migrations ./migrations

# Create config directory
RUN mkdir -p /app/config && chown -R asenix:asenix /app

# Copy example config and rename it
COPY config.example.toml /app/config/
RUN mv /app/config/config.example.toml /app/config/config.toml
RUN sed -i 's/embedding_dimension = 1536/embedding_dimension = 384/' /app/config/config.toml

# Switch to non-root user
# USER mote

# Expose port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=10s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Run the application
CMD ["asenix-server", "--config", "/app/config/config.toml"]
