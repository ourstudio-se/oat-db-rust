# Build stage
FROM rust:1.86 as builder

WORKDIR /app

# Install build dependencies including GLPK
RUN apt-get update && apt-get install -y \
    libglpk-dev \
    pkg-config \
    build-essential \
    wget \
    && rm -rf /var/lib/apt/lists/*

# Set environment variables to help GLPK build
ENV PKG_CONFIG_PATH=/usr/lib/pkgconfig:/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig
ENV GLPK_LIB_DIR=/usr/lib/aarch64-linux-gnu
ENV GLPK_INCLUDE_DIR=/usr/include

# Create a pkg-config file for GLPK since it doesn't provide one
RUN mkdir -p /usr/lib/aarch64-linux-gnu/pkgconfig && \
    echo "prefix=/usr" > /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "exec_prefix=\${prefix}" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "libdir=\${exec_prefix}/lib/aarch64-linux-gnu" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "includedir=\${prefix}/include" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "Name: GLPK" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "Description: GNU Linear Programming Kit" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "Version: 5.0" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "Libs: -L\${libdir} -lglpk" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc && \
    echo "Cflags: -I\${includedir}" >> /usr/lib/aarch64-linux-gnu/pkgconfig/glpk.pc

# Copy Cargo files first for better layer caching
COPY Cargo.toml Cargo.lock ./

# Copy source code
COPY src ./src
COPY migrations ./migrations

# Copy SQLx offline data for compile-time verification
COPY .sqlx ./.sqlx

# Enable SQLx offline mode for Docker builds
ENV SQLX_OFFLINE=true

# Build the application
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    libglpk40 \
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