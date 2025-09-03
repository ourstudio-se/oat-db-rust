# Integration Tests

This directory contains integration tests for the oat-db-rust project.

## Bike Store Integration Test

The `bike_store_integration.rs` file contains a comprehensive integration test that validates the complete user story for a bike store scenario. This test covers:

### Test Scenario

1. **Database Setup**: Create a new database called "bike-store"
2. **Schema Creation**: Create schemas for Color, Wheels, and Bike with relationships
3. **Instance Creation**: Create color instances (red, blue), wheel instances (standard, premium), and bike instances
4. **Initial Commit**: Commit all changes to establish baseline
5. **Domain Verification**: Verify bike1 instance shows correct relationship domains
6. **Branch Operations**: Create feature branch, add green color, update bike1, commit and merge
7. **Pool Filtering**: Create another branch, apply price-based filter to color relationship
8. **Final Verification**: Ensure the complete workflow produces the expected final state

### Test Structure

The test validates:
- ✅ Database and schema creation
- ✅ Instance creation with typed properties
- ✅ Relationship management with pool-based selections
- ✅ Git-like branch operations (create, commit, merge, delete)
- ✅ Pool resolution with filtering (colors with price > 120)
- ✅ Domain verification at each step
- ✅ End-to-end workflow integrity

### Running the Tests

#### Option 1: Automated Script (Recommended)

```bash
# Run the complete integration test with automatic setup
./run_integration_test.sh
```

This script will:
1. Start PostgreSQL with Docker Compose
2. Start the API server
3. Run the integration test
4. Clean up resources

#### Option 2: Manual Setup

1. **Start PostgreSQL**:
   ```bash
   docker-compose up -d postgres
   ```

2. **Start the API server**:
   ```bash
   cargo run
   ```

3. **Run the integration test** (in another terminal):
   ```bash
   cargo test test_bike_store_complete_workflow -- --exact --nocapture
   ```

4. **Connection Test** (optional):
   ```bash
   cargo test test_api_connection -- --exact --nocapture
   ```

### Test Requirements

- Docker and Docker Compose installed
- PostgreSQL running on port 5432
- API server running on port 3001
- Clean database state (no conflicting data)

### Test Output

The test provides detailed logging of each step:
```
🚀 Starting Bike Store Integration Test
1. Creating bike-store database
2. Creating Color schema
3. Creating Wheels schema
...
✅ Bike Store Integration Test completed successfully!
🎉 All 23 steps passed - the complete user story works as expected!
```

### Troubleshooting

- **Port conflicts**: Ensure ports 3001 and 5432 are available
- **Database state**: The test assumes a clean database; existing data may cause conflicts
- **Docker issues**: Ensure Docker is running and `docker-compose up` works
- **API startup time**: The script waits 10 seconds for API startup; adjust if needed

### Test Coverage

This integration test serves as:
- **End-to-end validation** of the complete system
- **User story verification** for the bike store scenario  
- **Regression testing** for core functionality
- **API contract validation** for all major endpoints
- **Git-like operations testing** for branch workflows

The test is marked with `#[ignore]` by default since it requires external setup. Run explicitly with the script or manual setup as described above.