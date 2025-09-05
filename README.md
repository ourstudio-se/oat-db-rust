# OAT-DB Rust: Git-like Combinatorial Database Backend

A Rust-based backend for a combinatorial database system with **git-like branching**, **typed properties**, and **class-based schemas**. Features conditional properties, derived fields, and flexible relationship management with branch-based version control.

## 🌟 Key Features

### Git-like Branch Model

- **Database → Branches → Schema + Instances** (like git repositories)
- **Working commit staging system** for grouping multiple changes into logical commits
- **Default branch** (typically "main") for each database
- **Branch lineage tracking** with parent-child relationships
- **Commit history** with hash, message, and author tracking
- **Branch status** management (Active, Merged, Archived)

### Schema & Data Features

- **Class-based schemas** with separate definitions for each entity type
- **Typed properties** with explicit data types (string, number, bool)
- **Conditional properties** using rule-based evaluation with relationship presence checking
- **Pool resolution system** for combinatorial optimization with default pool strategies
- **Derived fields** with expression evaluation (sum, count, arithmetic operations)
- **Flexible relationships** with quantifiers (EXACTLY, AT_LEAST, AT_MOST, RANGE, OPTIONAL, ANY, ALL)
- **Advanced relationship selection** with pool-based, filter-based, and explicit selection modes
- **PostgreSQL backend** with git-like commit storage and branch-aware queries
- **Immutable commits** with SHA-256 hashing and compressed binary data storage
- **Comprehensive audit trail system** with user tracking for all class and instance operations
- **REST API** built with Axum for complete CRUD operations

## Architecture

```
src/
├── api/            # Axum HTTP handlers and routes
├── model/          # Core data structures (Database, Branch, Schema, Instance)
│   ├── database.rs # Database and Branch models with git-like properties
│   ├── schema.rs   # Schema with class-based definitions
│   ├── instance.rs # Instances with typed property values
│   └── ...
├── logic/          # Business logic (validation, evaluation, resolution)
├── store/          # Storage traits and in-memory implementation
├── seed/           # Sample data for testing
└── lib.rs         # Module exports and tests
```

## Data Hierarchy

```
Database (with default_branch_id)
└── Branches (git-like: main, feature-xyz, etc.)
    ├── Schema (class-based with multiple ClassDef entries)
    │   ├── Class: "Underbed" (properties, relationships, derived)
    │   ├── Class: "Size" (properties, relationships, derived)
    │   ├── Class: "Fabric" (properties, relationships, derived)
    │   └── Class: "Leg" (properties, relationships, derived)
    └── Instances (many per branch, typed properties)
        ├── Underbed instances
        ├── Size instances
        ├── Fabric instances
        └── Leg instances
```

## Git-like Workflow

### Typical User Story

1. **Work on default branch** - Query/modify data on main branch
2. **Create feature branch** - Branch off main for new changes
3. **Make changes** - Edit schema and/or instances on feature branch
4. **Validate data** - Ensure data integrity on feature branch
5. **Commit changes** - Create commit with message and author
6. **Merge back** - Merge feature branch back to main when ready

### Granular Operations Workflow

1. **Add classes individually** - `POST /schema/classes` with just the new class data
2. **Modify specific classes** - `PATCH /schema/classes/{id}` for targeted updates
3. **Remove obsolete classes** - `DELETE /schema/classes/{id}` for clean schema management
4. **Instance-level control** - Individual CRUD operations on specific instances
5. **Branch-specific changes** - Apply granular operations to specific branches

## Quick Start

### Prerequisites

You need PostgreSQL running. Set up your database connection:

```bash
cp .env.example .env
# Edit .env with your PostgreSQL connection details
```

### Running the Server

```bash
# With PostgreSQL (recommended)
DATABASE_TYPE=postgres

# If you'd like to prepend data
LOAD_SEED_DATA=true
```

The server starts on `http://localhost:7061`. It uses the git-like schema:

- Databases with git-like branches and commit history
- SHA-256 commit hashes with compressed binary data
- Branch-aware instance queries and proper database isolation

### Running Tests

```bash
cargo test
```

## API Endpoints

### Databases

- `GET /databases` - List all databases
- `POST /databases` - Create database (auto-creates main branch)
- `GET /databases/{db_id}` - Get specific database
- `GET /databases/{db_id}/commits` - List all commits for database
- `DELETE /databases/{db_id}` - Delete database (only allows deletion of empty databases)

### Branches (Git-like)

- `GET /databases/{db_id}/branches` - List branches for database
- `POST /databases/{db_id}/branches` - Create new branch
- `GET /databases/{db_id}/branches/{branch_id}` - Get specific branch

### Database-level Endpoints (Auto-select Main Branch)

- `GET /databases/{db_id}/schema` - Get schema from main branch
- `POST /databases/{db_id}/schema` - Create/update schema on main branch
- `POST /databases/{db_id}/schema/classes` - Add new class to schema
- `GET /databases/{db_id}/schema/classes/{class_id}` - Get individual class
- `PATCH /databases/{db_id}/schema/classes/{class_id}` - Update individual class
- `DELETE /databases/{db_id}/schema/classes/{class_id}` - Delete individual class
- `GET /databases/{db_id}/instances` - List instances from main branch
- `POST /databases/{db_id}/instances` - Create instance on main branch
- `GET /databases/{db_id}/instances/{id}` - Get instance from main branch
- `PATCH /databases/{db_id}/instances/{id}` - Update instance on main branch
- `DELETE /databases/{db_id}/instances/{id}` - Delete instance from main branch
- `GET /databases/{db_id}/instances/{id}/derived` - Get derived values from main branch

### Branch-specific Endpoints (Explicit Branch Targeting)

- `GET /databases/{db_id}/branches/{branch_id}/schema` - Get schema for specific branch
- `POST /databases/{db_id}/branches/{branch_id}/schema` - Create/update schema on branch
- `POST /databases/{db_id}/branches/{branch_id}/schema/classes` - Add new class to schema
- `GET /databases/{db_id}/branches/{branch_id}/schema/classes/{class_id}` - Get individual class
- `PATCH /databases/{db_id}/branches/{branch_id}/schema/classes/{class_id}` - Update individual class
- `DELETE /databases/{db_id}/branches/{branch_id}/schema/classes/{class_id}` - Delete individual class
- `GET /databases/{db_id}/branches/{branch_id}/instances` - List instances from branch
- `POST /databases/{db_id}/branches/{branch_id}/instances` - Create instance on branch
- `GET /databases/{db_id}/branches/{branch_id}/instances/{id}` - Get instance from branch
- `PATCH /databases/{db_id}/branches/{branch_id}/instances/{id}` - Update instance on branch
- `DELETE /databases/{db_id}/branches/{branch_id}/instances/{id}` - Delete instance from branch
- `GET /databases/{db_id}/branches/{branch_id}/instances/{id}/derived` - Get derived values from branch

### Type Validation Endpoints

- `GET /databases/{db_id}/validate` - Validate all instances in database (main branch)
- `GET /databases/{db_id}/instances/{instance_id}/validate` - Validate single instance (main branch)
- `GET /databases/{db_id}/branches/{branch_id}/validate` - Validate all instances in specific branch
- `GET /databases/{db_id}/branches/{branch_id}/instances/{instance_id}/validate` - Validate single instance in branch

### Merge Validation Endpoints

- `GET /databases/{db_id}/branches/{source_branch_id}/validate-merge` - Validate merge into main branch
- `GET /databases/{db_id}/branches/{source_branch_id}/validate-merge/{target_branch_id}` - Validate merge between branches

### Rebase Endpoints

- `POST /databases/{db_id}/branches/{feature_branch_id}/rebase` - Rebase feature branch onto main branch
- `POST /databases/{db_id}/branches/{feature_branch_id}/rebase/{target_branch_id}` - Rebase feature branch onto specific target

### Rebase Validation Endpoints

- `GET /databases/{db_id}/branches/{feature_branch_id}/validate-rebase` - Validate rebase onto main branch
- `GET /databases/{db_id}/branches/{feature_branch_id}/validate-rebase/{target_branch_id}` - Validate rebase onto specific branch

### Working Commit Endpoints (Git-like Staging)

- `POST /databases/{db_id}/branches/{branch_id}/working-commit` - Create staging area for accumulating changes
- `GET /databases/{db_id}/branches/{branch_id}/working-commit` - View staged changes with **resolved relationships** (includes schema default pools)
- `GET /databases/{db_id}/branches/{branch_id}/working-commit/validate` - Validate all staged changes before committing
- `GET /databases/{db_id}/branches/{branch_id}/working-commit/raw` - View raw working commit data without relationship resolution
- `POST /databases/{db_id}/branches/{branch_id}/working-commit/commit` - Commit all staged changes as single commit
- `DELETE /databases/{db_id}/branches/{branch_id}/working-commit` - Abandon staged changes without committing

### Working Commit Staging Routes (Auto-created if needed)

- `PATCH /databases/{db_id}/branches/{branch_id}/working-commit/schema/classes/{class_id}` - Stage class schema updates
- `PATCH /databases/{db_id}/branches/{branch_id}/working-commit/instances/{instance_id}` - Stage instance property updates

### Configuration & Solve Endpoints

- `POST /databases/{db_id}/instances/{instance_id}/query` - Query configuration for specific instance (main branch)
- `POST /databases/{db_id}/branches/{branch_id}/instances/{instance_id}/query` - Query configuration for instance on specific branch
- `POST /solve` - Legacy solve endpoint (deprecated - use instance-specific endpoints)
- `GET /artifacts` - List configuration artifacts
- `GET /artifacts/{artifact_id}` - Get specific configuration artifact
- `GET /artifacts/{artifact_id}/summary` - Get artifact summary

### Query Parameters

- `?class=ClassID` - Filter instances by class ID
- `?expand=rel1,rel2&depth=N` - Expand relationships with depth control (expand defaults to all relationships)
- `?depth=N` - Control expansion depth for included instances (depth=0 shows relationships without nested instances)

## Model Structures

### Class Models

The API supports different models for different operations:

#### `ClassDef` (Full Class with ID)

Used for responses and internal storage:

```json
{
  "id": "class-chair",
  "name": "Chair",
  "description": "Chair furniture class",
  "properties": [...],
  "relationships": [...],
  "derived": [...]
}
```

#### `NewClassDef` (Input Model for Creation)

Used when creating classes (ID generated server-side):

```json
{
  "name": "Chair",
  "description": "Chair furniture class",
  "properties": [...],
  "relationships": [...],
  "derived": [...]
}
```

#### `ClassDefUpdate` (Partial Update Model)

Used for PATCH operations (all fields optional):

```json
{
  "description": "Updated description only"
}
```

### Instance Models

#### `Instance` (Full Instance with ID)

Used for responses:

```json
{
  "id": "chair-001",
  "branch_id": "main-branch-id",
  "class": "class-chair",
  "properties": {...},
  "relationships": {...}
}
```

#### `NewInstance` (Input Model for Creation)

Used when creating instances:

```json
{
  "class": "class-chair",
  "properties": {...},
  "relationships": {...}
}
```

#### `InstanceUpdate` (Partial Update Model)

Used for PATCH operations:

```json
{
  "properties": {
    "price": { "Literal": { "value": 299.99, "type": "Number" } }
  }
}
```

## Design Decisions

### Why Do Instances Have a `branch_id` Field?

You might wonder why each instance stores its branch ID. This design decision serves several important purposes in the git-like database system:

#### **Benefits of `branch_id` on Instances:**

1. **Branch Isolation** - Ensures instances are properly isolated between branches, preventing accidental cross-branch data access
2. **Performance** - Direct filtering by branch_id is faster than maintaining separate branch-instance relationship tables
3. **Data Integrity** - Clear ownership model prevents data corruption and ensures consistency
4. **Merge Operations** - Essential for branch merge logic to identify which instances belong to which branch
5. **Validation** - Handlers can quickly validate that an instance belongs to the expected branch

#### **Alternative Approaches Considered:**

- **Branch-agnostic instances** with separate mapping tables (more complex queries, harder to maintain consistency)
- **Context-based approach** where branch info is only in the URL (loses data integrity guarantees)
- **Implicit branch membership** (makes merge operations much more complex)

#### **Git-like Semantics:**

Just like git commits belong to specific branches, instances in this system belong to specific branches. This makes the git-like workflow intuitive and maintains clear data lineage.

The field is serialized as `branch_id` in JSON for clarity, while internally using `version_id` for backward compatibility.

## Example Usage

### 1. Create Database (with Main Branch)

```bash
curl -X POST http://localhost:7061/databases \
  -H "Content-Type: application/json" \
  -d '{
    "id": "furniture-db",
    "name": "Furniture Store",
    "description": "Product catalog database"
  }'
```

This automatically creates a "main" branch as the default.

### 1.1. Query Database Directly (Auto-selects Main Branch)

```bash
# Get schema from main branch automatically
curl http://localhost:7061/databases/furniture-db/schema

# List instances from main branch automatically
curl http://localhost:7061/databases/furniture-db/instances

# Get specific instance from main branch
curl http://localhost:7061/databases/furniture-db/instances/delux-underbed
```

These endpoints automatically use the database's default branch (typically "main") under the hood, providing a convenient way to work with the primary dataset without specifying branch IDs.

### 1.2. Granular Class Management

```bash
# Add a new class to the main branch
curl -X POST http://localhost:7061/databases/furniture-db/schema/classes \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Chair",
    "description": "Chair furniture class",
    "properties": [
      {"id": "prop-chair-name", "name": "name", "data_type": "String", "required": true},
      {"id": "prop-chair-price", "name": "price", "data_type": "Number", "required": true}
    ],
    "relationships": [
      {"id": "rel-chair-legs", "name": "legs", "targets": ["class-leg"], "quantifier": {"Exactly": 4}, "selection": "ExplicitOrFilter"}
    ],
    "derived": []
  }'

# Get an individual class
curl http://localhost:7061/databases/furniture-db/schema/classes/class-chair

# Update a class (partial update)
curl -X PATCH http://localhost:7061/databases/furniture-db/schema/classes/class-chair \
  -H "Content-Type: application/json" \
  -d '{
    "description": "Updated chair description"
  }'

# Delete a class
curl -X DELETE http://localhost:7061/databases/furniture-db/schema/classes/class-chair
```

### 1.3. Granular Instance Management

```bash
# Delete an individual instance from main branch
curl -X DELETE http://localhost:7061/databases/furniture-db/instances/delux-underbed

# Update just specific properties of an instance
curl -X PATCH http://localhost:7061/databases/furniture-db/instances/delux-underbed \
  -H "Content-Type: application/json" \
  -d '{
    "properties": {
      "price": {"Literal": {"value": 250.00, "type": "Number"}}
    }
  }'
```

### 2. Create Feature Branch

```bash
curl -X POST http://localhost:7061/databases/furniture-db/branches \
  -H "Content-Type: application/json" \
  -d '{
    "id": "feature-new-tables",
    "name": "Add Table Support",
    "description": "Branch for adding table furniture support",
    "parent_branch_id": "main-branch-id"
  }'
```

### 3. Create Class-based Schema

```bash
curl -X POST http://localhost:7061/databases/furniture-db/branches/feature-new-tables/schema \
  -H "Content-Type: application/json" \
  -d '{
    "id": "FurnitureSchema",
    "classes": [
      {
        "name": "Table",
        "description": "Table furniture class",
        "properties": [
          {"id": "name", "data_type": "string", "required": true},
          {"id": "basePrice", "data_type": "number", "required": true},
          {"id": "material", "data_type": "string", "required": true}
        ],
        "relationships": [
          {
            "id": "legs",
            "targets": ["class-leg"],
            "quantifier": {"Exactly": 4},
            "selection": "explicit-or-filter"
          }
        ],
        "derived": [
          {
            "id": "totalPrice",
            "data_type": "number",
            "expr": {
              "Add": {
                "left": {"Prop": {"prop": "basePrice"}},
                "right": {"Sum": {"over": "legs", "prop": "price"}}
              }
            }
          }
        ]
      },
      {
        "name": "Leg",
        "description": "Furniture leg class",
        "properties": [
          {"id": "name", "data_type": "string", "required": true},
          {"id": "material", "data_type": "string", "required": true},
          {"id": "price", "data_type": "number", "required": true}
        ],
        "relationships": [],
        "derived": []
      }
    ]
  }'
```

### 4. Create Typed Instances

```bash
# Create leg instances first
curl -X POST http://localhost:7061/databases/furniture-db/branches/feature-new-tables/instances \
  -H "Content-Type: application/json" \
  -d '{
    "id": "oak-leg-1",
    "class": "class-leg",
    "properties": {
      "name": {"Literal": {"value": "Oak Table Leg #1", "type": "string"}},
      "material": {"Literal": {"value": "Oak", "type": "string"}},
      "price": {"Literal": {"value": 45, "type": "number"}}
    }
  }'

# Create more legs (oak-leg-2, oak-leg-3, oak-leg-4)...

# Create table instance with relationships
curl -X POST http://localhost:7061/databases/furniture-db/branches/feature-new-tables/instances \
  -H "Content-Type: application/json" \
  -d '{
    "id": "dining-table-001",
    "class": "class-table",
    "properties": {
      "name": {"Literal": {"value": "Oak Dining Table", "type": "string"}},
      "basePrice": {"Literal": {"value": 800, "type": "number"}},
      "material": {"Literal": {"value": "Oak", "type": "string"}}
    },
    "relationships": {
      "legs": {"Ids": {"ids": ["oak-leg-1", "oak-leg-2", "oak-leg-3", "oak-leg-4"]}}
    }
  }'
```

### 5. Get Instance with Derived Values

```bash
# Get derived totalPrice (basePrice + sum of leg prices)
curl http://localhost:7061/databases/furniture-db/branches/feature-new-tables/instances/dining-table-001/derived
```

Response:

```json
{
  "derived": {
    "totalPrice": 980
  }
}
```

### 6. Query with Expansion

```bash
# Relationships are expanded by default, showing resolved pool information and filter details
curl "http://localhost:7061/databases/furniture-db/branches/feature-new-tables/instances/dining-table-001"

# Control expansion depth to include related instances
curl "http://localhost:7061/databases/furniture-db/branches/feature-new-tables/instances/dining-table-001?depth=1"

# Expand specific relationships only
curl "http://localhost:7061/databases/furniture-db/branches/feature-new-tables/instances/dining-table-001?expand=legs"
```

### 7. Branch Management

```bash
# List all branches
curl http://localhost:7061/databases/furniture-db/branches

# Get branch details with commit info
curl http://localhost:7061/databases/furniture-db/branches/feature-new-tables
```

Response shows git-like branch info:

```json
{
  "id": "feature-new-tables",
  "database_id": "furniture-db",
  "name": "Add Table Support",
  "parent_branch_id": "main-branch-id",
  "commit_hash": "abc123...",
  "commit_message": "Created branch 'Add Table Support'",
  "author": "developer@company.com",
  "status": "active",
  "created_at": "2024-01-15T10:30:00Z"
}
```

### 8. Database Deletion

Database deletion includes comprehensive safety checks to prevent accidental data loss:

```bash
# Try to delete database with commit history (will be blocked)
curl -X DELETE http://localhost:7061/databases/furniture-db
```

Response (409 Conflict):
```json
{
  "error": "Cannot delete database: contains commit history. This operation would cause data loss."
}
```

```bash
# Create a test database for deletion
curl -X POST http://localhost:7061/databases \
  -H "Content-Type: application/json" \
  -d '{
    "id": "test-db",
    "name": "Test Database",
    "description": "Database for testing deletion"
  }'

# Delete empty database (succeeds)
curl -X DELETE http://localhost:7061/databases/test-db
```

Response (200 OK):
```json
{
  "message": "Database deleted successfully",
  "deleted_database_id": "test-db"
}
```

**Safety Features:**
- **Won't delete databases with commit history** (prevents data loss)
- **Won't delete databases with multiple branches** (must delete feature branches first)  
- **Won't delete databases with active working commits** (must commit or abandon changes first)
- **Only allows deletion of truly empty databases** (new databases with only main branch, no commits)

## 🚀 Working Commit System (Git-like Staging)

The OAT-DB includes a sophisticated **working commit system** that enables git-like staging of changes before creating permanent commits. This allows you to group multiple related changes into single, logical commits with clean history.

### Core Concepts

- **Working Commit**: A mutable staging area where you accumulate changes before committing
- **Staging**: Making changes that are stored temporarily in the working commit
- **Committing**: Converting all staged changes into a permanent, immutable commit
- **Abandoning**: Discarding all staged changes without creating a commit
- **Schema Default Pool Resolution**: Working commits automatically resolve relationships using class schema default pools, just like regular branch endpoints

### Enhanced Relationship Resolution

Working commits now provide **comprehensive relationship resolution** that matches the behavior of regular branch endpoints:

- **Explicit Relationships**: Instance-configured relationships are resolved using working commit data
- **Schema Default Pools**: Relationships defined in class schema with `default_pool` settings are automatically resolved even if not explicitly configured on instances
- **Complete Coverage**: All relationships defined in the class schema are shown, providing full visibility into available selections
- **Working Commit Context**: All resolution uses staged working commit data, not just the base branch data

### Why Use Working Commits?

| **Without Working Commits**   | **With Working Commits**      |
| ----------------------------- | ----------------------------- |
| Each API call = 1 commit      | Multiple API calls = 1 commit |
| Verbose commit history        | Clean, logical commit history |
| Hard to group related changes | Easy to group related changes |
| No review before commit       | Review staged changes first   |

### Working Commit API Endpoints

#### Staging Management

- `POST /databases/{db_id}/branches/{branch_id}/working-commit` - Create staging area
- `GET /databases/{db_id}/branches/{branch_id}/working-commit` - View staged changes
- `DELETE /databases/{db_id}/branches/{branch_id}/working-commit` - Abandon staged changes

#### Committing

- `POST /databases/{db_id}/branches/{branch_id}/working-commit/commit` - Commit staged changes

### Complete Working Commit Workflow

#### Example: Adding Description Property to Color Class

Let's walk through adding a "description" property to the Color class and updating all existing color instances.

#### Step 1: Create Staging Area

```bash
# Start staging changes
curl -X POST http://localhost:7061/databases/furniture_catalog/branches/main/working-commit \
  -H "Content-Type: application/json" \
  -d '{
    "author": "developer@company.com"
  }'
```

Response:

```json
{
  "id": "working-commit-uuid",
  "database_id": "furniture_catalog",
  "branch_id": "main",
  "author": "developer@company.com",
  "status": "active",
  "created_at": "2024-01-15T10:30:00Z",
  "schema_data": {...},
  "instances_data": [...]
}
```

#### Step 2: Stage Schema Change

```bash
# Add description property to Color class (staged)
curl -X PATCH http://localhost:7061/databases/furniture_catalog/schema/classes/class-color \
  -H "Content-Type: application/json" \
  -d '{
    "properties": [
      {"id": "prop-color-name", "name": "name", "data_type": "String", "required": true},
      {"id": "prop-color-price", "name": "price", "data_type": "Number", "required": true},
      {"id": "prop-color-description", "name": "description", "data_type": "String", "required": false}
    ]
  }'
```

#### Step 3: Stage Instance Changes

```bash
# Add description to red color (staged)
curl -X PATCH http://localhost:7061/databases/furniture_catalog/instances/color-red \
  -H "Content-Type: application/json" \
  -d '{
    "properties": {
      "description": {
        "Literal": {
          "value": "A vibrant red color perfect for bold designs",
          "type": "String"
        }
      }
    }
  }'

# Add description to blue color (staged)
curl -X PATCH http://localhost:7061/databases/furniture_catalog/instances/color-blue \
  -H "Content-Type: application/json" \
  -d '{
    "properties": {
      "description": {
        "Literal": {
          "value": "A calming blue color ideal for modern aesthetics",
          "type": "String"
        }
      }
    }
  }'

# Add description to gold color (staged)
curl -X PATCH http://localhost:7061/databases/furniture_catalog/instances/color-gold \
  -H "Content-Type: application/json" \
  -d '{
    "properties": {
      "description": {
        "Literal": {
          "value": "An elegant gold color for luxury applications",
          "type": "String"
        }
      }
    }
  }'
```

#### Step 4: Review Staged Changes

```bash
# View what's currently staged with full relationship resolution
curl http://localhost:7061/databases/furniture_catalog/branches/main/working-commit
```

This returns the working commit with all staged changes - the updated Color class schema and all modified color instances. **All relationships are fully resolved**, including:

- **Explicit instance relationships** with their original configuration and resolved instance IDs
- **Schema default pool relationships** that are automatically resolved from class definitions
- **Detailed resolution metadata** showing how each relationship was resolved and from what source

The enhanced response shows both `original` relationship configuration and `resolved` materialized IDs with comprehensive resolution details.

#### Step 5: Commit All Changes Together

```bash
# Create single logical commit with all changes
curl -X POST http://localhost:7061/databases/furniture_catalog/branches/main/working-commit/commit \
  -H "Content-Type: application/json" \
  -d '{
    "message": "Add description property to Color class and update all color instances",
    "author": "developer@company.com"
  }'
```

Response:

```json
{
  "hash": "def456789abcdef...",
  "database_id": "furniture_catalog",
  "parent_hash": "abc123456fedcba...",
  "author": "developer@company.com",
  "message": "Add description property to Color class and update all color instances",
  "created_at": "2024-01-15T10:35:00Z",
  "data_size": 15420,
  "schema_classes_count": 8,
  "instances_count": 26
}
```

#### Alternative: Abandon Changes

If you decide not to commit the changes:

```bash
# Discard all staged changes
curl -X DELETE http://localhost:7061/databases/furniture_catalog/branches/main/working-commit
```

### Commit History Comparison

#### Without Working Commits (Old Approach)

```
abc123 <- def456 <- ghi789 <- jkl012 <- mno345
  ^        ^         ^         ^         ^
initial  add desc   red desc  blue desc gold desc
commit   property   value     value     value
```

**Result**: 4 separate commits for one logical change

#### With Working Commits (New Approach)

```
abc123 <- def456
  ^        ^
initial  add description property + update all instances
commit   (single logical commit)
```

**Result**: 1 clean commit with all related changes

### Advanced Working Commit Features

#### Auto-Creation

If you make changes without an active working commit, the system automatically creates one:

```bash
# This automatically creates a working commit if none exists
curl -X PATCH .../schema/classes/class-color -d '{...changes...}'
```

#### Working Commit Status

Working commits have three states:

- `active` - Currently being worked on (can stage more changes)
- `committing` - In the process of being committed (temporary state)
- `abandoned` - Discarded (will be garbage collected)

#### Conflict Prevention

Only one active working commit per branch is allowed:

```bash
# If you try to create when one already exists
curl -X POST .../working-commit -d '{...}'
# Returns: 409 Conflict - "Branch already has an active working commit"
```

### Working Commits vs Direct Commits

#### Direct Commits (Method 1)

```bash
# Each change creates immediate commit
curl -X PATCH .../schema/classes/class-color -d '{...}'  # Commit A
curl -X PATCH .../instances/color-red -d '{...}'         # Commit B
curl -X PATCH .../instances/color-blue -d '{...}'        # Commit C
```

**Use when**: Making simple, standalone changes

#### Working Commits (Method 2)

```bash
# Stage multiple changes, then commit together
curl -X POST .../working-commit -d '{...}'               # Create staging
curl -X PATCH .../schema/classes/class-color -d '{...}'  # Stage change
curl -X PATCH .../instances/color-red -d '{...}'         # Stage change
curl -X PATCH .../instances/color-blue -d '{...}'        # Stage change
curl -X POST .../working-commit/commit -d '{...}'        # Single commit
```

**Use always**: This is now the only supported way to make modifications

### Integration with Git-like Operations

Working commits integrate seamlessly with branch operations:

- **Merge**: Working commits must be committed or abandoned before merging branches
- **Rebase**: Similar requirements - clean working state needed
- **Branch Switching**: Working commits are branch-specific
- **Validation**: All staged changes are validated before committing

### Best Practices

1. **Logical Grouping**: Group related schema and instance changes together
2. **Clear Messages**: Write descriptive commit messages that explain the complete change
3. **Review Before Commit**: Use `GET working-commit` to review staged changes
4. **Clean Up**: Abandon working commits you decide not to pursue
5. **One Feature Per Working Commit**: Don't mix unrelated changes in one working commit

The working commit system provides the best of both worlds - the simplicity of direct commits when you need them, and the power of git-like staging for complex, multi-step changes.

## Property Type System

All properties now include explicit typing:

```json
{
  "properties": {
    "name": {
      "Literal": {
        "value": "Oak Table",
        "type": "string"
      }
    },
    "price": {
      "Literal": {
        "value": 299.99,
        "type": "number"
      }
    },
    "inStock": {
      "Literal": {
        "value": true,
        "type": "bool"
      }
    },
    "dynamicPrice": {
      "Conditional": {
        "branches": [
          {
            "when": { "Has": { "rel": "size", "ids": ["large"] } },
            "then": 399.99
          }
        ],
        "default": 299.99
      }
    }
  }
}
```

## 📋 Audit Trail System

The OAT-DB includes a comprehensive audit trail system that tracks who created and modified every class and instance in the system, providing full accountability and change history.

### Core Audit Features

- **Object-level Tracking**: Every `ClassDef` and `Instance` tracks its audit information
- **Creation Tracking**: `created_by` (user ID) and `created_at` (UTC timestamp) for all objects
- **Modification Tracking**: `updated_by` (user ID) and `updated_at` (UTC timestamp) for updates
- **User Context Extraction**: Automatic user identification from HTTP headers
- **Legacy Data Compatibility**: Seamless handling of existing data through serde defaults
- **API Integration**: All create/update operations automatically populate audit fields

### Audit Field Structure

Every class and instance includes audit metadata:

```json
{
  "id": "class-chair",
  "name": "Chair", 
  "properties": [...],
  "relationships": [...],
  "created_by": "user-123",
  "created_at": "2024-01-15T10:30:00.000Z",
  "updated_by": "admin-456", 
  "updated_at": "2024-01-16T14:45:30.000Z"
}
```

### User Context Headers

The API extracts user information from request headers:

- **X-User-Id** (required): Unique user identifier 
- **X-User-Email** (optional): User email address
- **X-User-Name** (optional): User display name

```bash
curl -X POST http://localhost:7061/databases/furniture-db/schema/classes \
  -H "Content-Type: application/json" \
  -H "X-User-Id: developer-123" \
  -H "X-User-Email: dev@company.com" \
  -H "X-User-Name: Jane Developer" \
  -d '{ "name": "NewClass", ... }'
```

### Legacy Data Handling

Existing data without audit fields is automatically handled using default values:

- **Legacy User**: `"legacy-user"` for `created_by`/`updated_by` fields
- **Legacy Timestamp**: Unix epoch (1970-01-01) for `created_at`/`updated_at` fields

This ensures backward compatibility while enabling audit tracking for all future operations.

### API Operations with Audit Trail

All class and instance operations now track user activity:

- **Class Creation**: `POST /databases/{db_id}/schema/classes` - Records creator
- **Class Updates**: `PATCH /databases/{db_id}/schema/classes/{class_id}` - Records modifier  
- **Instance Creation**: `POST /databases/{db_id}/instances` - Records creator
- **Instance Updates**: `PATCH /databases/{db_id}/instances/{id}` - Records modifier

### Audit Benefits

- **Accountability**: Know exactly who made each change and when
- **Change History**: Track evolution of classes and instances over time
- **Compliance**: Meet audit requirements for data modification tracking
- **Debugging**: Identify who introduced specific changes or data issues
- **Security**: Monitor and audit all data modification activities

## 🧩 Conditional Properties System

The OAT-DB includes a sophisticated conditional properties system that allows property values to be determined by rules based on relationship presence, enabling dynamic pricing, configuration, and business logic.

### Core Features

- **Rule-based Property Evaluation**: Properties can use conditional logic instead of fixed values
- **Relationship Presence Checking**: Rules can check if specific relationships exist on an instance
- **Simple JSON Syntax**: Clean, readable conditional syntax with `{"all": ["rel1", "rel2"]}` format
- **Fallback Values**: Default values when no rules match
- **Validation Integration**: Conditional properties are validated to ensure referenced relationships exist

### Conditional Property Format

```json
{
  "properties": {
    "price": {
      "rules": [
        {
          "when": { "all": ["a", "b"] },
          "then": 100.0
        },
        {
          "when": { "all": ["a", "c"] },
          "then": 110.0
        }
      ],
      "default": 0
    }
  }
}
```

### Example: Dynamic Painting Pricing

The seed data includes a `Painting` class that demonstrates conditional pricing based on component relationships:

**Painting Schema**:

```json
{
  "name": "Painting",
  "properties": [
    {
      "name": "price",
      "data_type": "Number",
      "required": false
    }
  ],
  "relationships": [
    { "name": "a", "targets": ["Component"] },
    { "name": "b", "targets": ["Component"] },
    { "name": "c", "targets": ["Component"] }
  ]
}
```

**Painting Instances with Conditional Pricing**:

- **painting1**: Has components A + B → Price = $100
- **painting2**: Has components A + C → Price = $110
- **painting3**: Has only component A → Price = $0 (default)

### Conditional Property Evaluation

When accessing a conditional property, the system:

1. **Evaluates each rule in order** - checks `when` condition against instance relationships
2. **Returns first match** - uses `then` value from first rule where condition is true
3. **Falls back to default** - uses default value if no rules match
4. **Validates relationships** - ensures all referenced relationships exist in class schema

### Use Cases

- **Dynamic Pricing**: Prices based on selected options or configurations
- **Configuration Logic**: Different settings based on feature combinations
- **Business Rules**: Complex logic based on relationship presence
- **Conditional Features**: Features enabled/disabled based on other selections

### Testing Conditional Properties

See `verify_features.md` for complete testing instructions. Quick test:

```bash
cargo run  # Start server
# Instances now return expanded relationships by default with detailed resolution information
curl -s http://localhost:7061/databases/furniture_catalog/instances/painting1 | jq '.properties.price'  # Returns: 100
curl -s http://localhost:7061/databases/furniture_catalog/instances/painting-minimal | jq '.properties.price'  # Returns: 25

# View relationship resolution details with filter information
curl -s http://localhost:7061/databases/furniture_catalog/instances/car-001 | jq '.relationships.color.resolution_details'
```

## 🎯 Domain System for Configuration Spaces

The OAT-DB includes a comprehensive domain system for managing configuration spaces and instance selection constraints. Domains define value ranges for instances, enabling super-configuration management and constraint satisfaction.

### Core Domain Concepts

- **Domain**: A range `[lower, upper]` defining possible values for an instance
- **Class Domain Constraints**: Default domain ranges for instances of a class
- **Instance Domains**: Actual domain values for specific instances (can override class defaults)
- **Super Configuration**: Collection of instances with their domain ranges
- **Specific Configuration**: All domains collapsed to constants (lower == upper)

### Domain Structure

```json
{
  "domain": {
    "lower": 0,
    "upper": 1
  }
}
```

### Domain Types and Semantics

#### Binary Domains `[0,1]`

Instance can be included (1) or excluded (0):

```rust
Domain::binary()  // Creates [0,1] domain
```

#### Constant Domains `[n,n]`

Instance has fixed value (always selected with specific quantity):

```rust
Domain::constant(1)  // Creates [1,1] domain (always 1 copy)
Domain::constant(5)  // Creates [5,5] domain (always 5 copies)
```

#### Range Domains `[min,max]`

Instance can have any value within range:

```rust
Domain::new(0, 4)   // Creates [0,4] domain (0 to 4 copies allowed)
Domain::new(1, 10)  // Creates [1,10] domain (1 to 10 copies allowed)
```

### Class Domain Constraints (Schema Level)

Classes define default domains for their instances:

```json
{
  "name": "Color",
  "domain_constraint": {
    "lower": 1,
    "upper": 1
  }
}
```

**This means**: Every Color instance defaults to domain `[1,1]` (always selected).

### Instance Domains (Instance Level)

Instances can override class defaults:

```json
{
  "id": "painting-minimal",
  "class": "class-painting",
  "domain": {
    "lower": 0,
    "upper": 1
  }
}
```

### Configuration Space Examples

The seed data demonstrates various domain strategies:

#### Class Domain Constraints

- **Painting/Component/Option/Car**: `[0,1]` → instances default to binary selection
- **Size/Color**: `[1,1]` → instances default to always selected
- **Fabric**: `[0,10]` → instances default to allowing 0-10 copies
- **Leg**: `[0,4]` → instances default to allowing 0-4 copies

#### Instance Domain Overrides

- **painting-minimal**: `[0,1]` (inherits Painting class default)
- **comp-a**: `[1,5]` (overrides Component class default `[0,1]`)
- **painting1**: `[1,1]` (constant - always included)
- **painting2**: `[0,3]` (allows 0-3 copies)

### Domain Helper Methods

The Domain struct provides useful utility methods:

```rust
let domain = Domain::new(0, 3);

domain.is_constant()     // false (0 != 3)
domain.is_binary()       // false (not [0,1])
domain.contains(2)       // true (2 is in [0,3])

let constant = Domain::constant(5);
constant.is_constant()   // true (5 == 5)
```

### Configuration Workflow

1. **Super Configuration**: Start with instances having domain ranges

   ```
   painting-a: [0,1], color-red: [1,1], option-gps: [0,1]
   ```

2. **Configuration Process**: Make selection decisions

   ```
   painting-a: [1,1], color-red: [1,1], option-gps: [0,0]
   ```

3. **Specific Configuration**: All domains are constants
   - painting-a: included (1 copy)
   - color-red: selected (1 copy)
   - option-gps: excluded (0 copies)

### Domain Validation

Domains provide the foundation for:

- **Configuration Validation**: Ensuring selections respect domain constraints
- **Solution Space Definition**: Defining valid configuration boundaries
- **Constraint Satisfaction**: Managing complex selection rules
- **Optimization**: Finding optimal configurations within domain bounds

### API Integration

Domains appear in both class definitions and instance responses:

```bash
# View class domain constraints
curl http://localhost:7061/databases/furniture_catalog/branches/main/schema | jq '.classes[] | {name, domain_constraint}'

# View instance domains
curl http://localhost:7061/databases/furniture_catalog/instances/painting-minimal | jq '{id, type, domain}'
```

### Use Cases

- **Product Configuration**: Define valid option ranges for products
- **Resource Allocation**: Constrain resource assignment quantities
- **Combinatorial Optimization**: Search within defined solution spaces
- **Configuration Management**: Manage valid configuration states
- **Constraint Programming**: Express domain constraints for solvers

## 🎯 Pool Resolution System

The OAT-DB includes an advanced pool resolution system for combinatorial optimization, allowing sophisticated control over which instances are available for selection in relationships.

### Core Concepts

- **Default Pools**: Schema-level defaults for what instances are available by default
- **Instance Overrides**: Instance-level pool customization with filters
- **Pool Resolution**: Determines all available instances for relationships
- **Solver Selection**: Quantifiers and solvers determine final selections from available instances

### Pool Resolution Modes

#### DefaultPool::All

All instances of target type(s) are available in the pool by default.

```json
{
  "name": "color",
  "targets": ["class-color"],
  "default_pool": { "mode": "all" }
}
```

#### DefaultPool::None

No instances are available by default - must be explicitly specified.

```json
{
  "name": "freeOptions",
  "targets": ["Option"],
  "default_pool": { "mode": "none" }
}
```

#### DefaultPool::Filter

A filtered subset based on conditions.

```json
{
  "name": "budgetColors",
  "targets": ["class-color"],
  "default_pool": {
    "mode": "filter",
    "type": ["class-color"],
    "where": {
      "all": {
        "predicates": [{ "prop_lt": { "prop": "price", "value": 100 } }]
      }
    }
  }
}
```

### Pool-Based Relationship Customization

Instances can override schema defaults with custom pool filters:

```json
{
  "relationships": {
    "color": {
      "pool": {
        "type": ["class-color"],
        "where": {
          "all": {
            "predicates": [{ "prop_lt": { "prop": "price", "value": 100 } }]
          }
        },
        "limit": 2
      }
    }
  }
}
```

**Key Points:**
- **Pool defines availability**: What instances CAN be chosen from
- **No selection property**: The solver determines what IS chosen based on quantifiers
- **No sort property**: Order doesn't matter for combinatorial problems

### Example: Car Color and Options

The seed data includes comprehensive Car/Color/Option examples demonstrating different pool strategies:

**Car Schema**:

```json
{
  "name": "Car",
  "relationships": [
    {
      "name": "color",
      "targets": ["class-color"],
      "quantifier": { "Exactly": 1 },
      "default_pool": { "mode": "all" }
    },
    {
      "name": "freeOptions",
      "targets": ["Option"],
      "quantifier": { "AtLeast": 0 },
      "default_pool": { "mode": "none" }
    }
  ]
}
```

**Car Examples**:

1. **Sedan (car-001)**: Custom color pool (under $100), explicit GPS option
2. **Luxury SUV (car-002)**: Schema default (all colors), custom expensive options pool
3. **Economy Hatchback (car-003)**: Budget color pool with sorting/limiting, no free options

### Pool Resolution Process

#### Single-Step Pool Resolution

Determines all instances available for solver selection:

```rust
let effective_pool = PoolResolver::resolve_effective_pool(
    store,
    relationship_def,
    instance_pool_override, // Optional custom filter
    branch_id,
).await?;
```

#### Result: Available Options

All instances from the pool are returned as `materialized_ids`:

```rust
// Pool resolution provides ALL available options
// Solver uses quantifiers to make final selections
let materialized_ids = effective_pool; // All instances available for solver
```

### Solver Integration

- **Pool Resolution**: Finds all available instances (e.g., all colors under $100)  
- **Materialized IDs**: Contains full set of options for solver to choose from
- **Quantifiers**: Guide solver selection (e.g., `EXACTLY(1)` = pick exactly 1 color)
- **Solver Output**: Final configuration with specific instance selections

### Use Cases

- **E-commerce Configuration**: Available options based on product tier
- **Resource Allocation**: Constrain resource pools based on quotas or policies
- **Combinatorial Optimization**: Complex constraint satisfaction problems
- **Dynamic Catalogs**: Available products change based on customer segment

### Testing Pool Resolution

See `verify_features.md` for complete testing instructions. Quick test:

```bash
cargo run  # Start server
# All instances now return expanded relationships by default with comprehensive pool resolution details
curl -s http://localhost:7061/databases/furniture_catalog/instances/car-001 | jq '.relationships'
# Shows resolved pools with filter descriptions, timing, and resolution methods

# View detailed filter information for pool resolution
curl -s http://localhost:7061/databases/furniture_catalog/instances/car-001 | jq '.relationships.color.resolution_details.filter_description'
# Returns: "Pool filter: InstanceFilter { types: Some([\"Color\"]), where_clause: Some(All { predicates: [PropLt { prop: \"price\", value: Number(100) }] }), sort: None, limit: None }"
```

## Sample Data Structure

The seed data creates this git-like structure:

```
Furniture Catalog Database
├── default_branch_id: "main-branch-uuid"
└── Main Branch (name: "main")
    ├── commit_hash: "initial-commit-uuid"
    ├── commit_message: "Initial commit"
    ├── author: "System"
    ├── status: "active"
    ├── FurnitureCatalogSchema (class-based)
    │   ├── Class: "Underbed" (domain_constraint: [0,1])
    │   │   ├── Properties: name, basePrice, price
    │   │   ├── Relationships: size, fabric, leg
    │   │   └── Derived: totalPrice = basePrice + Sum(leg.price)
    │   ├── Class: "Size" (domain_constraint: [1,1], Properties: name, width, length)
    │   ├── Class: "Fabric" (domain_constraint: [0,10], Properties: name, color, material)
    │   ├── Class: "Leg" (domain_constraint: [0,4], Properties: name, material, price)
    │   ├── Class: "Painting" (domain_constraint: [0,1], Conditional pricing based on components)
    │   │   ├── Properties: name, price (conditional)
    │   │   └── Relationships: a, b, c (to Component instances)
    │   ├── Class: "Component" (domain_constraint: [0,1], Properties: name, type)
    │   ├── Class: "Car" (domain_constraint: [0,1], Pool resolution examples)
    │   │   ├── Properties: model
    │   │   ├── Relationships: color (default pool: All), freeOptions (default pool: None)
    │   │   └── Pool strategies: DefaultPool::All vs DefaultPool::None
    │   ├── Class: "Color" (domain_constraint: [1,1], Properties: name, price)
    │   └── Class: "Option" (domain_constraint: [0,1], Properties: name, price)
    └── Instances (all with typed properties):
        ├── Size: size-small, size-medium
        ├── Fabric: fabric-cotton-white, fabric-linen-beige
        ├── Legs: leg-wooden, leg-wooden-2, leg-wooden-3, leg-wooden-4, leg-metal
        ├── Underbed: delux-underbed (with unique leg references)
        ├── Painting: painting1 (domain: [1,1], components A+B, $100), painting2 (domain: [0,3], A+C, $110), painting3 (A only, $0), painting-minimal (domain: [0,1], no components, $25)
        ├── Components: comp-a (domain: [1,5]), comp-b, comp-c (for conditional pricing examples)
        ├── Cars: car-001 (Sedan), car-002 (Luxury SUV), car-003 (Economy Hatchback)
        ├── Colors: color-red ($50), color-blue ($75), color-gold ($150)
        └── Options: option-gps ($300), option-sunroof ($800)
```

## Development

The project uses:

- **Axum** for HTTP server
- **SQLx** for PostgreSQL integration with compile-time query checking
- **Serde** for JSON serialization
- **Tokio** for async runtime
- **Anyhow/ThisError** for error handling
- **Chrono** for timestamps
- **SHA2** for commit hashing
- **Flate2** for commit data compression

### Architecture Highlights

- **Git-like Branch Model**: Each database has branches with commit history like git repos
- **PostgreSQL Backend**: Production-ready persistence with proper ACID transactions
- **Immutable Commits**: SHA-256 hashed commits with compressed schema + instance data
- **Branch-aware Queries**: All operations respect database isolation boundaries
- **Class-based Schemas**: One schema contains multiple class definitions
- **Typed Properties**: Every property has explicit type information
- **Trait-based Storage**: Abstracted storage layer supporting multiple backends

### Testing

Tests cover:

- Database and branch creation (git-like workflow)
- Class-based schema management
- Typed property instance operations
- Branch-based data isolation
- Hierarchical data integrity

Run `cargo test` to verify implementation.

### Current Status

The current implementation provides a complete production-ready system with PostgreSQL backend:

#### ✅ **Core Architecture**

- **Git-like PostgreSQL schema** with commits, branches, and immutable history
- **Enhanced working commit staging system** with full relationship resolution including schema default pools
- **SHA-256 commit hashing** with compressed binary data storage (gzip)
- **Branch-aware database isolation** preventing cross-database data leakage
- **Comprehensive audit trail system** with user tracking for all class and instance operations
- Class-based schemas with separate entity definitions
- Typed properties with explicit data types (String, Number, Boolean, Object, Array, StringList)
- **Conditional properties system** with rule-based evaluation and relationship presence checking
- **Advanced pool resolution system** for combinatorial optimization with default pool strategies and working commit context
- **Domain system** for configuration space management with class constraints and instance domains
- Database/Branch hierarchy with proper isolation
- **Production PostgreSQL backend** with trait-based abstraction
- Full backward compatibility with in-memory storage option

#### ✅ **API Features**

- REST API endpoints with comprehensive CRUD operations
- Database-level API endpoints that auto-select main branch
- Branch-specific endpoints for isolated operations
- Granular class CRUD operations with individual endpoints
- Individual instance delete and update operations
- **Working commit system** with git-like staging and commit workflow
- Query parameters for filtering and relationship expansion

#### ✅ **Improved Validation Workflow**

The system now supports a **user-controlled validation approach** that separates data modification from validation:

- **🔧 PATCH Operations**: Work without validation constraints, allowing incremental data fixes
- **🔍 Explicit Validation**: Use dedicated `/validate` endpoints to check data when ready
- **📝 Working Commit Validation**: New `/working-commit/validate` endpoint for pre-commit validation
- **✅ Commit Control**: Users decide when data is ready to be committed

**Benefits:**
- Fix invalid data step-by-step without being blocked
- Make partial changes and validate when ready  
- Complete control over validation timing
- No more validation errors preventing legitimate data updates

**Example Workflow:**
```bash
# 1. Make changes without validation blocking
curl -X PATCH /databases/db/instances/item \
  -d '{"properties": {"new_field": {"value": "test", "type": "String"}}}'

# 2. Validate staged changes when ready
curl /databases/db/branches/main/working-commit/validate

# 3. Commit when validation passes
curl -X POST /databases/db/branches/main/working-commit/commit
```

#### ✅ **Type Validation System**

- **Comprehensive instance validation** against class-based schemas
- **Schema compliance checking** with detailed error reporting
- **Data type validation** for all property values
- **Required property enforcement**
- **Value-type consistency verification**
- **Relationship validation** for undefined connections
- **Conditional property validation** with relationship reference checking
- **Pool-based relationship validation** with constraint verification

#### ✅ **Merge Validation System**

- **Pre-merge validation** to prevent data corruption
- **Merge simulation** without affecting actual data
- **Validation conflict detection** integrated with merge process
- **Enhanced merge blocking** for validation errors
- **Detailed reporting** of affected instances and potential issues

#### ✅ **Branch Operations**

- Branch merge operations with comprehensive conflict detection
- **Git-like rebase functionality** for keeping branches up to date
- Branch deletion with proper status management
- Branch commit functionality with hash and author tracking
- Automatic validation integration in merge and rebase processes
- Force merge/rebase capability for override scenarios
- Pre-operation validation to prevent data corruption

#### ✅ **Documentation & Developer Experience**

- **Interactive Swagger UI documentation** at `/docs`
- **Complete OpenAPI 3.0 specification** with all endpoints
- **Live API testing** directly from browser
- **Comprehensive schema definitions** with examples
- **Error response documentation** with detailed schemas

### Branch Operations API

#### Merge Branch

```bash
POST /databases/{db_id}/branches/{branch_id}/merge
```

```json
{
  "target_branch_id": "main-branch-id",
  "author": "developer@company.com",
  "force": false
}
```

Response:

```json
{
  "success": true,
  "conflicts": [],
  "merged_instances": 5,
  "merged_schema_changes": true,
  "message": "Successfully merged branch 'feature-xyz' into 'main'"
}
```

#### Rebase Branch

```bash
POST /databases/{db_id}/branches/{feature_branch_id}/rebase
```

```json
{
  "target_branch_id": "main",
  "author": "developer@company.com",
  "force": false
}
```

Response:

```json
{
  "success": true,
  "conflicts": [],
  "message": "Successfully rebased 'feature-add-materials' onto 'main'",
  "rebased_instances": 10,
  "rebased_schema_changes": true
}
```

#### Rebase with Specific Target

```bash
POST /databases/{db_id}/branches/{feature_branch_id}/rebase/{target_branch_id}
```

```json
{
  "author": "developer@company.com",
  "force": false
}
```

#### Commit Changes

```bash
POST /databases/{db_id}/branches/{branch_id}/commit
```

```json
{
  "message": "Add new table support with validation",
  "author": "developer@company.com"
}
```

#### Delete Branch

```bash
POST /databases/{db_id}/branches/{branch_id}/delete
```

```json
{
  "force": false
}
```

## 🔍 Type Validation System

The OAT-DB includes a comprehensive type validation system that ensures data integrity across all branches and merge operations.

### Core Validation Features

- **Schema Compliance**: All properties validated against class definitions
- **Data Type Checking**: Values validated against declared types (String, Number, Boolean, Object, Array, StringList)
- **Required Property Validation**: Missing required properties caught during validation
- **Type Consistency**: Declared type must match actual JSON value type
- **Relationship Validation**: Basic checks for undefined relationships
- **Detailed Error Reporting**: Rich error and warning information with specific property details

### Validation API Endpoints

#### Instance Validation

- `GET /databases/{db_id}/validate` - Validate all instances in database (main branch)
- `GET /databases/{db_id}/instances/{instance_id}/validate` - Validate single instance in database (main branch)
- `GET /databases/{db_id}/branches/{branch_id}/validate` - Validate all instances in specific branch
- `GET /databases/{db_id}/branches/{branch_id}/instances/{instance_id}/validate` - Validate single instance in specific branch

#### Merge Validation (Pre-merge Safety Checks)

- `GET /databases/{db_id}/branches/{source_branch_id}/validate-merge` - Validate merge into database main branch
- `GET /databases/{db_id}/branches/{source_branch_id}/validate-merge/{target_branch_id}` - Validate merge between specific branches

### Validation Result Format

```json
{
  "valid": true,
  "errors": [],
  "warnings": [
    {
      "instance_id": "delux-underbed",
      "warning_type": "ConditionalPropertySkipped",
      "message": "Conditional property 'price' was not type-checked",
      "property_name": "price"
    }
  ],
  "instance_count": 10,
  "validated_instances": ["size-small", "size-medium", "fabric-cotton-white", ...]
}
```

### Validation Error Types

- **TypeMismatch**: Property type doesn't match schema
- **MissingRequiredProperty**: Required field is absent
- **UndefinedProperty**: Instance has property not in schema
- **ValueTypeInconsistency**: JSON value doesn't match declared type
- **ClassNotFound**: Instance type has no schema definition
- **RelationshipError**: Undefined relationships

### Example: Validate All Instances

```bash
# Check all instances in main branch
curl http://localhost:7061/databases/furniture_catalog/validate

# Check specific instance
curl http://localhost:7061/databases/furniture_catalog/instances/delux-underbed/validate

# Check all instances in feature branch
curl http://localhost:7061/databases/furniture_catalog/branches/feature-xyz/validate
```

## 🔄 Merge Validation System

The merge validation system prevents data integrity issues by validating merges **before** they happen, ensuring schema changes don't break existing instances.

### Pre-Merge Workflow

1. **Developer creates feature branch** and modifies schema classes
2. **Before merging back to main**, calls validation endpoint:
   ```bash
   GET /databases/furniture_catalog/branches/feature-new-properties/validate-merge
   ```
3. **System simulates the merge** and validates all main branch instances against the modified schema
4. **Returns detailed report** showing potential validation errors

### Merge Validation Features

- **Merge Simulation**: Creates virtual merge result without affecting actual data
- **Full Instance Validation**: Validates all instances against merged schema
- **Conflict Detection**: Identifies schema/instance conflicts and validation issues
- **Detailed Reporting**: Shows exactly which instances would fail and why
- **Prevention**: Stops problematic merges before they corrupt data

### Merge Validation Result

```json
{
  "can_merge": false,
  "conflicts": [
    {
      "conflict_type": "ValidationConflict",
      "resource_id": "delux-underbed",
      "description": "Merge would create validation error: Required property 'material' is missing (Instance: delux-underbed)"
    }
  ],
  "validation_result": {
    "valid": false,
    "errors": [
      {
        "instance_id": "delux-underbed",
        "error_type": "MissingRequiredProperty",
        "message": "Required property 'material' is missing",
        "property_name": "material",
        "expected": "String",
        "actual": null
      }
    ],
    "warnings": [],
    "instance_count": 10,
    "validated_instances": ["delux-underbed", ...]
  },
  "simulated_schema_valid": true,
  "affected_instances": ["delux-underbed", "size-small", "fabric-cotton-white"]
}
```

### Integration with Merge Process

The validation system is automatically integrated into the merge process:

- **Automatic Detection**: Normal merge operations now detect validation conflicts
- **Merge Blocking**: Merges fail if validation errors would be introduced (unless `force` is used)
- **Enhanced Conflicts**: Merge conflicts now include validation issues alongside traditional conflicts

### Example: Pre-Merge Validation

```bash
# Check if feature branch can safely merge into main
curl http://localhost:7061/databases/furniture_catalog/branches/feature-add-materials/validate-merge

# Check merge between specific branches
curl http://localhost:7061/databases/furniture_catalog/branches/feature-src/validate-merge/feature-target
```

### When Validation Helps

**Perfect for preventing:**

- Schema changes that break existing instances
- Adding required properties without providing values
- Type changes that invalidate existing data
- Relationship modifications that break connections

**Example Scenario:**

1. Feature branch adds required `material` property to `Underbed` class
2. Main branch has `delux-underbed` instance without `material` property
3. Pre-merge validation catches this conflict before merge
4. Developer can either:
   - Make `material` optional instead of required
   - Add default `material` value to existing instances
   - Update the problematic instances in their branch first

## 🔀 Git-like Rebase System

The OAT-DB includes a comprehensive rebase system that allows you to replay your feature branch changes on top of the latest target branch state, similar to `git rebase`.

### Core Rebase Features

- **Branch Rebasing**: Replay feature branch changes on top of target branch (usually main)
- **Automatic Conflict Detection**: Identifies schema, instance, and validation conflicts before rebasing
- **Smart Merging**: Target branch provides the base, feature branch changes override conflicts
- **Validation Integration**: Ensures rebased result passes validation checks
- **Force Option**: Override conflicts when you're confident about changes

### Rebase vs Merge

| **Operation** | **What It Does**                                | **When to Use**                          |
| ------------- | ----------------------------------------------- | ---------------------------------------- |
| **Merge**     | Combines two branches, creating a merge commit  | When you want to preserve branch history |
| **Rebase**    | Replays feature changes on top of target branch | When you want a linear, clean history    |

### Rebase Workflow

1. **Check if rebase is needed**: Use validate-rebase to see if target branch has new changes
2. **Pre-rebase validation**: Check for conflicts and validation issues
3. **Resolve conflicts**: Fix schema or validation issues if needed
4. **Execute rebase**: Apply feature branch changes on top of target branch
5. **Verify result**: Feature branch now contains target branch base + feature changes

### Rebase API Endpoints

#### Rebase Validation

- `GET /databases/{db_id}/branches/{feature_branch_id}/validate-rebase` - Check rebase compatibility with main
- `GET /databases/{db_id}/branches/{feature_branch_id}/validate-rebase/{target_branch_id}` - Check rebase with specific target

#### Execute Rebase

- `POST /databases/{db_id}/branches/{feature_branch_id}/rebase` - Rebase onto main branch
- `POST /databases/{db_id}/branches/{feature_branch_id}/rebase/{target_branch_id}` - Rebase onto specific branch

### Rebase Validation Result

```json
{
  "can_rebase": true,
  "conflicts": [],
  "validation_result": {
    "valid": true,
    "errors": [],
    "warnings": [],
    "instance_count": 11,
    "validated_instances": ["instance1", "instance2", ...]
  },
  "needs_rebase": true,
  "affected_instances": ["instance1", "instance2", ...]
}
```

### Example: Complete Rebase Workflow

```bash
# 1. Check if rebase is needed and safe
curl http://localhost:7061/databases/furniture_catalog/branches/feature-add-materials/validate-rebase

# 2. If validation shows conflicts, fix them first
curl -X PATCH http://localhost:7061/databases/furniture_catalog/branches/feature-add-materials/schema/classes/class-underbed \
  -H "Content-Type: application/json" \
  -d '{"properties": [{"id": "prop-material", "name": "material", "data_type": "String", "required": false}]}'

# 3. Execute the rebase
curl -X POST http://localhost:7061/databases/furniture_catalog/branches/feature-add-materials/rebase \
  -H "Content-Type: application/json" \
  -d '{
    "target_branch_id": "main",
    "author": "developer@company.com",
    "force": false
  }'
```

Response:

```json
{
  "success": true,
  "conflicts": [],
  "message": "Successfully rebased 'feature-add-materials' onto 'main'",
  "rebased_instances": 10,
  "rebased_schema_changes": true
}
```

### What Happens During Rebase

1. **Target Branch Base**: Feature branch gets all instances and schema from target branch as the new base
2. **Feature Changes Applied**: Feature branch's schema and instance changes are applied on top
3. **Conflict Resolution**: Feature branch changes take precedence over target branch for same resources
4. **Branch Update**: Feature branch metadata updated with new commit hash and parent reference
5. **Validation Check**: Final result validated to ensure data integrity

### When to Use Rebase

**Perfect for:**

- Keeping feature branches up to date with main branch
- Creating linear history without merge commits
- Incorporating latest main branch changes before final merge
- Updating long-running feature branches

**Example Scenario:**

1. You create `feature-add-tables` branch from main
2. While you work, main branch gets new commits (new classes, instances)
3. Before merging back, you rebase to get latest main changes
4. Your feature branch now contains main's latest changes + your feature work
5. Final merge into main will be clean and linear

### Rebase Conflict Types

- **Schema Conflicts**: Both branches modified same classes
- **Instance Conflicts**: Both branches modified same instances
- **Validation Conflicts**: Rebased result would fail validation
- **Structural Conflicts**: Changes incompatible with target branch structure

### Force Rebase

Use `"force": true` when:

- You're confident about overriding conflicts
- Schema conflicts are intentional (feature branch has better schema)
- You've manually verified the result will be correct

⚠️ **Warning**: Force rebase can override validation errors and may break data integrity.

## 📚 Interactive API Documentation

The OAT-DB includes comprehensive interactive API documentation powered by Swagger UI.

### Accessing Documentation

- **Interactive UI**: Visit `http://localhost:7061/docs` for full Swagger UI interface
- **OpenAPI Spec**: Access raw specification at `http://localhost:7061/docs/openapi.json`

### Documentation Features

- **Complete API Coverage**: All endpoints with detailed descriptions
- **Interactive Testing**: Test API calls directly from the browser
- **Schema Definitions**: Full model documentation with examples
- **Error Responses**: Comprehensive error handling documentation
- **Organized by Tags**: Logical grouping (Databases, Validation, Branches, etc.)
- **Request/Response Examples**: Clear examples for all operations

### What's Documented

- All database, branch, schema, and instance endpoints
- Type validation endpoints with detailed error schemas
- Merge validation endpoints with conflict resolution examples
- Model definitions for all request/response structures
- Query parameters and their usage
- HTTP status codes and error conditions

## Granular Operations Benefits

### Why Use Individual Endpoints?

1. **Precision** - Modify only what needs changing without affecting other schema elements
2. **Atomic Operations** - Each class/instance operation is independent and atomic
3. **Better Error Handling** - Specific error messages for individual class/instance operations
4. **Conflict Avoidance** - No need to worry about concurrent modifications to other parts of the schema
5. **Cleaner API Design** - RESTful semantics with proper HTTP methods (POST, PATCH, DELETE)
6. **Model Separation** - Clean input models without server-managed fields (like IDs)

### When to Use Which Approach?

**Use Granular Endpoints When:**

- Adding a single new class to an existing schema
- Updating specific properties of one class
- Removing obsolete classes
- Need precise control over individual operations

**Use Bulk Schema Operations When:**

- Creating entirely new schemas from scratch
- Major schema restructuring affecting multiple classes
- Migrating between schema versions

## 🚀 Advanced Solve System

The OAT-DB includes a sophisticated solve system that transforms abstract relationship selections and conditional properties into concrete, reproducible configuration artifacts through a comprehensive pipeline.

### Core Architecture: Selector vs Resolution Context

The solve system separates **WHAT** to select from **WHERE/WHEN** to select it:

- **Selectors**: Abstract descriptions of what instances to choose (independent of branch/commit)
- **ResolutionContext**: The scope and policies for evaluating selectors at solve time
- **ConfigurationArtifacts**: Immutable, reproducible solve results with complete metadata

### Selector Types

#### Static Selectors

Pre-materialized instance IDs for deterministic selection:

```json
{
  "resolution_mode": "static",
  "materialized_ids": ["color-red", "color-blue"],
  "metadata": {
    "description": "Manually selected premium colors"
  }
}
```

#### Dynamic Selectors

Filter-based selection resolved at solve time:

```json
{
  "resolution_mode": "dynamic",
  "filter": {
    "type": ["class-color"],
    "where": {
      "all": ["premium", "available"]
    },
    "limit": 3
  }
}
```

### Resolution Context

Defines the scope and policies for selector evaluation:

```json
{
  "database_id": "furniture_catalog",
  "branch_id": "main",
  "commit_hash": "abc123def456", // Optional point-in-time
  "policies": {
    "cross_branch_policy": "reject",
    "missing_instance_policy": "skip",
    "empty_selection_policy": "allow",
    "max_selection_size": 1000
  }
}
```

#### Resolution Policies

- **Cross-Branch Policy**: How to handle references across branches (`reject`/`allow`/`allow_with_warnings`)
- **Missing Instance Policy**: Handle missing static IDs (`fail`/`skip`/`placeholder`)
- **Empty Selection Policy**: Handle empty dynamic results (`fail`/`allow`/`fallback`)
- **Max Selection Size**: Prevent runaway selections from dynamic filters

### Configuration Artifacts

Immutable solve results containing everything needed for reproducibility:

```json
{
  "id": "artifact-12345",
  "created_at": "2024-01-15T10:30:00Z",
  "resolution_context": {
    /* Full context snapshot */
  },
  "schema_snapshot": {
    /* Schema at solve time */
  },
  "resolved_domains": {
    "painting-a": { "lower": 1, "upper": 1 }, // Constant selection
    "color-red": { "lower": 1, "upper": 1 }
  },
  "resolved_properties": {
    "painting-a": {
      "price": 110.0, // Evaluated conditional property
      "name": "Premium Painting"
    }
  },
  "selector_snapshots": {
    "painting-a": {
      "color": {
        "selector": {
          /* Original selector definition */
        },
        "resolved_ids": ["color-red"],
        "resolution_notes": [
          {
            "note_type": "info",
            "message": "Static selector resolved successfully"
          }
        ]
      }
    }
  },
  "solve_metadata": {
    "total_time_ms": 250,
    "pipeline_phases": [
      { "name": "snapshot", "duration_ms": 50 },
      { "name": "expand", "duration_ms": 75 },
      { "name": "evaluate", "duration_ms": 80 },
      { "name": "validate", "duration_ms": 30 },
      { "name": "compile", "duration_ms": 15 }
    ],
    "statistics": {
      "total_instances": 5,
      "total_selectors": 3,
      "conditional_properties_evaluated": 2,
      "domains_resolved": 5
    }
  }
}
```

### Five-Phase Solve Pipeline

1. **Snapshot Phase**: Capture immutable state of schema and instances at solve time
2. **Expand Phase**: Resolve all selectors to concrete instance sets using resolution policies
3. **Evaluate Phase**: Process conditional properties and resolve domains to final values
4. **Validate Phase**: Check constraints, quantifiers, and relationship consistency
5. **Compile Phase**: Assemble final artifact with metadata and timing information

### Backwards Compatibility

The solve system automatically converts legacy pool-based selections to modern selectors:

- **Simple IDs** → Static selectors with materialized IDs
- **Filters** → Dynamic selectors with filter definitions
- **Pool-based** → Selectors derived from pool and selection components
- **All/None** → Dynamic selectors with appropriate filters

### API Usage

#### Create a Solve Operation

```bash
curl -X POST http://localhost:7061/solve \
  -H "Content-Type: application/json" \
  -d '{
    "resolution_context": {
      "database_id": "furniture_catalog",
      "branch_id": "main",
      "policies": {
        "cross_branch_policy": "reject",
        "missing_instance_policy": "skip"
      }
    },
    "user_metadata": {
      "name": "Production Configuration V1",
      "tags": ["production", "validated"]
    }
  }'
```

#### List Configuration Artifacts

```bash
curl "http://localhost:7061/artifacts?database_id=furniture_catalog&branch_id=main"
```

#### Get Artifact Details

```bash
curl http://localhost:7061/artifacts/{artifact_id}
```

#### Get Solve Summary

```bash
curl http://localhost:7061/artifacts/{artifact_id}/summary
```

### Key Benefits

- **Reproducible Solves**: Artifacts contain everything needed to reproduce exact results
- **Branch-Aware Resolution**: Proper isolation and cross-branch policy enforcement
- **Comprehensive Metadata**: Full timing, statistics, and resolution notes for debugging
- **Policy-Driven**: Configurable behavior for missing instances, empty selections, etc.
- **Immutable Results**: Artifacts never change, enabling reliable caching and auditing
- **Backwards Compatible**: Seamlessly works with existing pool/selection formats

### Use Cases

- **Configuration Management**: Generate and track validated product configurations
- **Audit Trails**: Immutable record of how configurations were derived
- **A/B Testing**: Compare different resolution contexts and policies
- **Debugging**: Detailed resolution notes and timing for troubleshooting
- **Caching**: Reuse artifacts for identical resolution contexts
- **Compliance**: Prove configurations meet specific constraints and policies

### Next Steps

Future enhancements could include:

- Advanced conflict resolution for complex merges
- Branch history and timeline tracking
- Full relationship validation with quantifiers (currently warnings only)
- Advanced expression evaluation for derived fields
- Full filter resolution implementation with branch context
- Branch-aware relationship expansion
- Database cloning and forking operations
- Granular property and relationship management within classes
- **Enhanced pool filtering** - implement full predicate evaluation in pool resolution
- **Universe constraints** - support for relationship universe restrictions
- **Cascading pool effects** - where selecting one option affects available pools for other relationships
