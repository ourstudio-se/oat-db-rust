#!/bin/bash

# Simple Integration Test Runner Script
# This version uses a locally built binary instead of Docker build
#
# Usage:
#   ./scripts/run-integration-test-simple.sh [OPTIONS]
#
# Options:
#   --help, -h              Show this help message
#   --clean-only            Only clean up containers and exit
#   --no-cleanup            Keep containers running after tests
#   --verbose, -v           Show verbose output
#   --test-name NAME        Run specific test by name (default: all integration tests)
#
# This version builds the binary locally and runs it in a container with the test database

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default options
CLEANUP_AFTER=true
VERBOSE=false
TEST_NAME=""
CLEAN_ONLY=false

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --help|-h)
            grep '^#' "$0" | sed 's/^# //' | head -20
            exit 0
            ;;
        --clean-only)
            CLEAN_ONLY=true
            shift
            ;;
        --no-cleanup)
            CLEANUP_AFTER=false
            shift
            ;;
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --test-name)
            TEST_NAME="$2"
            shift 2
            ;;
        *)
            echo -e "${RED}❌ Unknown option: $1${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Verbose logging function
log_verbose() {
    if [ "$VERBOSE" = true ]; then
        echo -e "${BLUE}[VERBOSE]${NC} $1"
    fi
}

# Detect Docker Compose command (v1 uses docker-compose, v2 uses docker compose)
detect_docker_compose() {
    if command -v docker-compose >/dev/null 2>&1; then
        echo "docker-compose"
    elif docker compose version >/dev/null 2>&1; then
        echo "docker compose"
    else
        echo -e "${RED}❌ Neither 'docker-compose' nor 'docker compose' found${NC}"
        echo -e "${YELLOW}Please install Docker Compose: https://docs.docker.com/compose/install/${NC}"
        exit 1
    fi
}

# Set Docker Compose command
DOCKER_COMPOSE_CMD=$(detect_docker_compose)
log_verbose "Using Docker Compose command: $DOCKER_COMPOSE_CMD"

echo -e "${BLUE}🚀 OAT-DB Rust Simple Integration Test Environment${NC}"

# Function to cleanup (no containers needed for in-memory store)
cleanup() {
    if [ "$CLEANUP_AFTER" = true ]; then
        echo -e "\n${YELLOW}🧹 Cleaning up...${NC}"
        log_verbose "No containers to clean up (using in-memory store)"
        echo -e "${GREEN}✅ Cleanup completed${NC}"
    else
        echo -e "\n${YELLOW}⚠️  No cleanup needed for in-memory store${NC}"
    fi
}

# Handle clean-only mode
if [ "$CLEAN_ONLY" = true ]; then
    echo -e "${YELLOW}🧹 Clean-only mode: nothing to clean for in-memory store...${NC}"
    echo -e "${GREEN}✅ Cleanup completed${NC}"
    exit 0
fi

# Trap to ensure cleanup on script exit (only if cleanup is enabled)
if [ "$CLEANUP_AFTER" = true ]; then
    trap cleanup EXIT
fi

# Step 1: No containers needed for in-memory store
echo -e "${YELLOW}📦 No containers needed for in-memory store...${NC}"
log_verbose "Skipping container management for in-memory store"

# Step 2: Build the application locally
echo -e "${YELLOW}🔨 Building application locally...${NC}"
log_verbose "Building with cargo build --release"
if [ "$VERBOSE" = true ]; then
    cargo build --release
else
    cargo build --release > /dev/null 2>&1
fi

# Step 3: Setup for in-memory store (no database needed)
echo -e "${YELLOW}🗄️ Using in-memory store - no database setup needed...${NC}"
log_verbose "Setting up for in-memory store usage"
export OAT_DATABASE_TYPE=memory

# Step 4: Start the API server using local binary
echo -e "${YELLOW}🌐 Starting OAT-DB API service (local binary)...${NC}"
log_verbose "Starting local binary with in-memory store"

# Set environment variables for the local API server  
export OAT_DATABASE_TYPE=memory
export OAT_SERVER_HOST=0.0.0.0
export OAT_SERVER_PORT=3002
export RUST_LOG=info
export ENVIRONMENT=test

# Start the server in the background
log_verbose "Starting server: ./target/release/oat-db-rust"
./target/release/oat-db-rust &
SERVER_PID=$!
log_verbose "Server started with PID: $SERVER_PID"

# Trap to kill server on exit
cleanup_server() {
    if [ -n "$SERVER_PID" ]; then
        log_verbose "Stopping server with PID: $SERVER_PID"
        kill $SERVER_PID 2>/dev/null || true
        wait $SERVER_PID 2>/dev/null || true
    fi
    cleanup
}

if [ "$CLEANUP_AFTER" = true ]; then
    trap cleanup_server EXIT
fi

# Step 5: Wait for API service to be ready
echo -e "${YELLOW}⏳ Waiting for API service to be ready...${NC}"
log_verbose "Waiting up to 60 seconds for API health check"
for i in {1..30}; do
    if curl -s http://localhost:3002/health > /dev/null 2>&1; then
        log_verbose "API service is ready after $i attempts"
        break
    fi
    if [ "$VERBOSE" = true ]; then echo "Waiting for API service... (attempt $i/30)"; fi
    sleep 2
    if [ $i -eq 30 ]; then
        echo -e "${RED}❌ API service failed to start after 60 seconds${NC}"
        exit 1
    fi
done

# Give it an extra moment to fully initialize
log_verbose "Giving API service extra time to fully initialize"
sleep 3

echo -e "${GREEN}✅ Environment is ready!${NC}"
if [ "$VERBOSE" = true ]; then
    echo -e "${BLUE}📊 Services running:${NC}"
    echo -e "  - OAT-DB API: ${GREEN}http://localhost:3002${NC} (in-memory store)"
    echo -e "  - API Health: ${GREEN}http://localhost:3002/health${NC}"
    echo -e "  - API Docs: ${GREEN}http://localhost:3002/docs${NC}"
fi

# Step 6: Run the integration tests
echo -e "\n${YELLOW}🧪 Running integration tests...${NC}"

# Set environment variable for test to use the correct port
export TEST_API_BASE_URL="http://localhost:3002"

# Determine test command based on options
if [ -n "$TEST_NAME" ]; then
    echo -e "${BLUE}ℹ️  Running specific test: $TEST_NAME${NC}"
    log_verbose "Test command: cargo test $TEST_NAME --test bike_store_integration"
    test_cmd="cargo test $TEST_NAME --test bike_store_integration"
else
    echo -e "${BLUE}ℹ️  Running all integration tests${NC}"
    log_verbose "Test command: cargo test --test bike_store_integration"
    test_cmd="cargo test --test bike_store_integration"
fi

# Add nocapture for verbose output
if [ "$VERBOSE" = true ]; then
    test_cmd="$test_cmd -- --nocapture"
fi

# Run the tests
log_verbose "Executing: $test_cmd"
if eval "$test_cmd"; then
    echo -e "\n${GREEN}🎉 Integration tests PASSED!${NC}"
    exit_code=0
else
    echo -e "\n${RED}❌ Integration tests FAILED!${NC}"
    exit_code=1
fi

# Step 7: Show logs if tests failed or in verbose mode
if [ $exit_code -ne 0 ] || [ "$VERBOSE" = true ]; then
    echo -e "\n${YELLOW}📋 Service info:${NC}"
    echo -e "${BLUE}=== API Service Info ===${NC}"
    echo "Server running locally with PID: $SERVER_PID (in-memory store)"
    echo "Check server logs in the terminal output above"
fi

# Final status message
if [ $exit_code -eq 0 ]; then
    echo -e "\n${GREEN}✅ All integration tests completed successfully!${NC}"
    if [ "$CLEANUP_AFTER" = false ]; then
        echo -e "${BLUE}ℹ️  Services are still running for debugging:${NC}"
        echo -e "  - PostgreSQL: ${GREEN}localhost:5433${NC}"
        echo -e "  - API: ${GREEN}http://localhost:3002${NC} (PID: $SERVER_PID)"
        echo -e "  - To stop: ${YELLOW}./scripts/run-integration-test-simple.sh --clean-only${NC} and kill $SERVER_PID"
    fi
else
    echo -e "\n${RED}❌ Integration tests failed with exit code $exit_code${NC}"
    if [ "$CLEANUP_AFTER" = false ]; then
        echo -e "${BLUE}ℹ️  Services left running for debugging (API PID: $SERVER_PID)${NC}"
    fi
fi

exit $exit_code