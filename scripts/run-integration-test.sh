#!/bin/bash

# Integration Test Runner Script
# This script sets up a complete containerized environment for running integration tests
#
# Usage:
#   ./scripts/run-integration-test.sh [OPTIONS]
#
# Options:
#   --help, -h              Show this help message
#   --clean-only            Only clean up containers and exit
#   --no-cleanup            Keep containers running after tests
#   --verbose, -v           Show verbose output
#   --test-name NAME        Run specific test by name (default: all integration tests)
#   --build-cache           Use Docker build cache (faster, but may use stale builds)
#
# Examples:
#   ./scripts/run-integration-test.sh                                    # Run all integration tests
#   ./scripts/run-integration-test.sh --test-name test_api_connection    # Run specific test
#   ./scripts/run-integration-test.sh --no-cleanup --verbose            # Debug mode
#   ./scripts/run-integration-test.sh --clean-only                      # Just cleanup

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
USE_CACHE=false
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
        --build-cache)
            USE_CACHE=true
            shift
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

echo -e "${BLUE}🚀 OAT-DB Rust Integration Test Environment${NC}"

# Function to cleanup containers
cleanup() {
    if [ "$CLEANUP_AFTER" = true ]; then
        echo -e "\n${YELLOW}🧹 Cleaning up containers...${NC}"
        log_verbose "Stopping and removing containers with volumes"
        $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml down --volumes --remove-orphans
        
        # Clean up any test logs
        if [ -d "./test-logs" ]; then
            log_verbose "Removing test logs directory"
            sudo rm -rf ./test-logs 2>/dev/null || rm -rf ./test-logs 2>/dev/null || true
        fi
        
        echo -e "${GREEN}✅ Cleanup completed${NC}"
    else
        echo -e "\n${YELLOW}⚠️  Containers left running (--no-cleanup specified)${NC}"
        echo -e "${BLUE}ℹ️  To clean up later, run: ./scripts/run-integration-test.sh --clean-only${NC}"
    fi
}

# Handle clean-only mode
if [ "$CLEAN_ONLY" = true ]; then
    echo -e "${YELLOW}🧹 Clean-only mode: removing containers and volumes...${NC}"
    $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml down --volumes --remove-orphans 2>/dev/null || true
    if [ -d "./test-logs" ]; then
        sudo rm -rf ./test-logs 2>/dev/null || rm -rf ./test-logs 2>/dev/null || true
    fi
    echo -e "${GREEN}✅ Cleanup completed${NC}"
    exit 0
fi

# Trap to ensure cleanup on script exit (only if cleanup is enabled)
if [ "$CLEANUP_AFTER" = true ]; then
    trap cleanup EXIT
fi

# Step 1: Stop any existing containers
echo -e "${YELLOW}📦 Stopping any existing test containers...${NC}"
log_verbose "Running docker-compose down with volumes cleanup"
$DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml down --volumes --remove-orphans 2>/dev/null || true

# Step 2: Build fresh images
if [ "$USE_CACHE" = true ]; then
    echo -e "${YELLOW}🔨 Building application image (using cache)...${NC}"
    log_verbose "Building with cache enabled for faster builds"
    $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml build
else
    echo -e "${YELLOW}🔨 Building fresh application image (no cache)...${NC}"
    log_verbose "Building without cache to ensure latest changes"
    $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml build --no-cache
fi

# Step 3: Start PostgreSQL first
echo -e "${YELLOW}🗄️ Starting PostgreSQL test database...${NC}"
log_verbose "Starting postgres-test container"
$DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml up -d postgres-test

# Step 4: Wait for PostgreSQL to be healthy
echo -e "${YELLOW}⏳ Waiting for PostgreSQL to be ready...${NC}"
log_verbose "Waiting up to 60 seconds for PostgreSQL health check"
timeout 60 bash -c 'until $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml exec postgres-test pg_isready -U postgres -d oatdb_test >/dev/null 2>&1; do
    if [ "$VERBOSE" = true ]; then echo "Waiting for PostgreSQL..."; fi
    sleep 2
done'

# Step 5: Run database migrations
echo -e "${YELLOW}📋 Running database migrations...${NC}"
export DATABASE_URL="postgres://postgres:password@localhost:5433/oatdb_test"
log_verbose "Installing sqlx-cli if needed"
cargo install sqlx-cli --no-default-features --features postgres 2>/dev/null || echo "sqlx-cli already installed"
log_verbose "Running migrations against $DATABASE_URL"
sqlx migrate run --source ./migrations

# Step 6: Start the API service
echo -e "${YELLOW}🌐 Starting OAT-DB API service...${NC}"
log_verbose "Starting oat-db-rust-test container"
$DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml up -d oat-db-rust-test

# Step 7: Wait for API service to be healthy
echo -e "${YELLOW}⏳ Waiting for API service to be ready...${NC}"
log_verbose "Waiting up to 120 seconds for API health check"
timeout 120 bash -c 'until curl -s http://localhost:3002/health > /dev/null; do
    if [ "$VERBOSE" = true ]; then echo "Waiting for API service..."; fi
    sleep 2
done'

# Give it an extra moment to fully initialize
log_verbose "Giving API service extra time to fully initialize"
sleep 3

echo -e "${GREEN}✅ Environment is ready!${NC}"
if [ "$VERBOSE" = true ]; then
    echo -e "${BLUE}📊 Services running:${NC}"
    echo -e "  - PostgreSQL Test DB: ${GREEN}localhost:5433${NC}"
    echo -e "  - OAT-DB API: ${GREEN}http://localhost:3002${NC}"
    echo -e "  - API Health: ${GREEN}http://localhost:3002/health${NC}"
    echo -e "  - API Docs: ${GREEN}http://localhost:3002/docs${NC}"
fi

# Step 8: Run the integration tests
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

# Step 9: Show logs if tests failed or in verbose mode
if [ $exit_code -ne 0 ] || [ "$VERBOSE" = true ]; then
    echo -e "\n${YELLOW}📋 Showing service logs:${NC}"
    
    if [ "$VERBOSE" = true ]; then
        echo -e "${BLUE}=== PostgreSQL Logs ===${NC}"
        $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml logs postgres-test
        echo -e "\n${BLUE}=== API Service Logs ===${NC}"
        $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml logs oat-db-rust-test
    else
        # Only show recent logs for failed tests
        echo -e "${BLUE}=== Recent PostgreSQL Logs ===${NC}"
        $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml logs --tail=50 postgres-test
        echo -e "\n${BLUE}=== Recent API Service Logs ===${NC}"
        $DOCKER_COMPOSE_CMD -f docker-compose.integration-test.yml logs --tail=50 oat-db-rust-test
    fi
fi

# Final status message
if [ $exit_code -eq 0 ]; then
    echo -e "\n${GREEN}✅ All integration tests completed successfully!${NC}"
    if [ "$CLEANUP_AFTER" = false ]; then
        echo -e "${BLUE}ℹ️  Services are still running for debugging:${NC}"
        echo -e "  - PostgreSQL: ${GREEN}localhost:5433${NC}"
        echo -e "  - API: ${GREEN}http://localhost:3002${NC}"
        echo -e "  - To stop: ${YELLOW}./scripts/run-integration-test.sh --clean-only${NC}"
    fi
else
    echo -e "\n${RED}❌ Integration tests failed with exit code $exit_code${NC}"
    if [ "$CLEANUP_AFTER" = false ]; then
        echo -e "${BLUE}ℹ️  Services left running for debugging${NC}"
    fi
fi

exit $exit_code