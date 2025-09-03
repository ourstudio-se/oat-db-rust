# Build stage
FROM rust:1.75 as builder

WORKDIR /app

# Copy Cargo files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/oat-db-rust /app/oat-db-rust

# Copy migrations
COPY --from=builder /app/migrations /app/migrations

# Create logs directory
RUN mkdir -p /app/logs

# Expose port
EXPOSE 3001

# Run the application
CMD ["./oat-db-rust"]