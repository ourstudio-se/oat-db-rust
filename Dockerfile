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

# Detect architecture and set paths accordingly for cross-platform builds
RUN ARCH=$(dpkg --print-architecture) && \
    if [ "$ARCH" = "amd64" ]; then \
        LIBDIR="/usr/lib/x86_64-linux-gnu"; \
    elif [ "$ARCH" = "arm64" ]; then \
        LIBDIR="/usr/lib/aarch64-linux-gnu"; \
    else \
        LIBDIR="/usr/lib"; \
    fi && \
    echo "Architecture: $ARCH, Library directory: $LIBDIR" && \
    export PKG_CONFIG_PATH="/usr/lib/pkgconfig:$LIBDIR/pkgconfig:/usr/share/pkgconfig" && \
    export GLPK_LIB_DIR="$LIBDIR" && \
    export GLPK_INCLUDE_DIR="/usr/include" && \
    mkdir -p "$LIBDIR/pkgconfig" && \
    echo "prefix=/usr" > "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "exec_prefix=\${prefix}" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "libdir=$LIBDIR" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "includedir=\${prefix}/include" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "Name: GLPK" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "Description: GNU Linear Programming Kit" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "Version: 5.0" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "Libs: -L$LIBDIR -lglpk" >> "$LIBDIR/pkgconfig/glpk.pc" && \
    echo "Cflags: -I\${includedir}" >> "$LIBDIR/pkgconfig/glpk.pc"

# Set environment variables for the build (support both architectures)
ENV PKG_CONFIG_PATH=/usr/lib/pkgconfig:/usr/lib/x86_64-linux-gnu/pkgconfig:/usr/lib/aarch64-linux-gnu/pkgconfig:/usr/share/pkgconfig

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
EXPOSE 7061

# Run the application
CMD ["./oat-db-rust"]