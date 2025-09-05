# Integration Testing Guide

This document explains how to run the comprehensive integration tests for OAT-DB Rust in a fully containerized environment.

## Overview

The integration tests use a complete containerized setup that:

1. **Starts a fresh PostgreSQL database** in its own container
2. **Runs all database migrations** to ensure schema is up-to-date  
3. **Builds and starts the OAT-DB API service** in a container
4. **Executes the full bike store workflow test** against the live API
5. **Cleans up all containers** after testing

## Quick Start

### Basic Usage

```bash
# Run all integration tests (recommended)
./scripts/run-integration-test.sh

# Run with verbose output for debugging
./scripts/run-integration-test.sh --verbose

# Run specific test only
./scripts/run-integration-test.sh --test-name test_api_connection

# Keep containers running after tests for debugging  
./scripts/run-integration-test.sh --no-cleanup --verbose

# Clean up containers and exit
./scripts/run-integration-test.sh --clean-only
```

### Advanced Options

```bash
# Use Docker build cache for faster subsequent runs
./scripts/run-integration-test.sh --build-cache

# Get help with all available options
./scripts/run-integration-test.sh --help
```

### Manual Docker Compose (if needed)

```bash
# Start the test environment
docker-compose -f docker-compose.integration-test.yml up -d

# Wait for services to be healthy, then run tests
export TEST_API_BASE_URL="http://localhost:3002"
cargo test bike_store_integration --test bike_store_integration

# Clean up
docker-compose -f docker-compose.integration-test.yml down --volumes
```

## Test Environment Details

### Services

- **PostgreSQL Test DB**: `localhost:5433` (isolated from dev environment on port 5432)
- **OAT-DB API**: `http://localhost:3002` (isolated from dev environment on port 3001)
- **API Health Check**: `http://localhost:3002/health`
- **API Documentation**: `http://localhost:3002/docs`

### Environment Variables

The integration test script sets:
- `TEST_API_BASE_URL=http://localhost:3002` - Points tests to the containerized API
- `DATABASE_URL=postgres://postgres:password@localhost:5433/oatdb_test` - For migrations

### What the Test Does

The `test_bike_store_complete_workflow()` test implements a comprehensive 23-step workflow:

1. **Database Setup**: Creates `bike-store` database
2. **Schema Creation**: Creates Color, Wheels, and Bike schemas with relationships
3. **Instance Creation**: Creates color instances (red, blue), wheel instances (standard, premium)
4. **Bike Instance**: Creates a bike with pool-based relationships
5. **Branch Operations**: Creates feature branches for adding green color and pool filtering
6. **Merge Operations**: Tests branch merging with conflict resolution
7. **Pool Resolution**: Tests combinatorial optimization with different pool strategies
8. **Validation**: Verifies all data integrity throughout the workflow

## Troubleshooting

### Common Issues

1. **Port Conflicts**: The test environment uses ports 5433 and 3002 to avoid conflicts with development
2. **Docker Compose Version**: The script automatically detects and uses either `docker-compose` (v1) or `docker compose` (v2)
3. **Docker Permissions**: You may need `sudo` for some Docker operations depending on your setup  
4. **Memory/Disk Space**: Make sure you have sufficient resources for multiple containers

### Debugging Failed Tests

If tests fail, the script automatically shows container logs:

```bash
# Manual log inspection (use docker-compose OR docker compose depending on your version)
docker compose -f docker-compose.integration-test.yml logs postgres-test
docker compose -f docker-compose.integration-test.yml logs oat-db-rust-test
```

### Cleanup Issues

If containers get stuck:

```bash
# Force cleanup (use docker-compose OR docker compose depending on your version)
docker compose -f docker-compose.integration-test.yml down --volumes --remove-orphans
docker system prune -f

# Or use the script (automatically detects Docker Compose version)
./scripts/run-integration-test.sh --clean-only
```

## Development Workflow

### Running Tests During Development

```bash
# Quick unit tests only
cargo test --lib

# Full integration test (takes longer)
./scripts/run-integration-test.sh

# Fast integration test using build cache  
./scripts/run-integration-test.sh --build-cache

# Debug failed tests by keeping containers running
./scripts/run-integration-test.sh --no-cleanup --verbose

# Development environment (separate from tests)
docker compose up -d        # Start dev environment (or docker-compose on v1)
docker compose down         # Stop dev environment  
docker compose logs -f      # View dev logs
```

### Adding New Integration Tests

1. Add test functions to `tests/bike_store_integration.rs`
2. Use the `TestClient` helper for API calls
3. Follow the existing pattern of step-by-step workflow testing
4. Test against `std::env::var("TEST_API_BASE_URL")` for flexibility

### Database Migrations

The integration test automatically runs migrations against the test database. For manual migration management:

```bash
# Install sqlx-cli if needed
cargo install sqlx-cli --no-default-features --features postgres

# Run migrations against test DB (port 5433)
export DATABASE_URL="postgres://postgres:password@localhost:5433/oatdb_test"
sqlx migrate run --source ./migrations

# Run migrations against dev DB (port 5432)
export DATABASE_URL="postgres://postgres:password@localhost:5432/oatdb"
sqlx migrate run --source ./migrations
```

## Architecture

```
Integration Test Environment:
┌─────────────────────────────────────────┐
│ Host Machine                            │
│ ┌─────────────────┐ ┌─────────────────┐ │
│ │ postgres-test   │ │ oat-db-rust-    │ │
│ │ Port: 5433      │ │ test            │ │
│ │ DB: oatdb_test  │ │ Port: 3002      │ │
│ └─────────────────┘ └─────────────────┘ │
│           │                   │         │
│           └─────── Network ───┘         │
│                                         │
│ ┌─────────────────────────────────────┐ │
│ │ Rust Test Process                   │ │
│ │ - Runs migrations                   │ │
│ │ - Executes API tests via HTTP      │ │
│ │ - Validates full workflow          │ │
│ └─────────────────────────────────────┘ │
└─────────────────────────────────────────┘
```

This setup ensures complete isolation from development environment and provides a reproducible test environment for CI/CD pipelines.