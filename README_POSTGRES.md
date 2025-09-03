# OAT-DB with PostgreSQL Integration

This document explains how to set up and use OAT-DB with PostgreSQL storage.

## Database Options

OAT-DB now supports two storage backends:

1. **In-Memory Store** (default) - For development and testing
2. **PostgreSQL Store** - For production use with persistent storage

## Quick Start with Docker Compose

The easiest way to get started is using Docker Compose, which runs both PostgreSQL and the OAT-DB service:

```bash
# Start the full stack (PostgreSQL + OAT-DB + pgAdmin)
docker-compose up -d

# View logs
docker-compose logs -f

# Stop everything
docker-compose down
```

This will start:
- PostgreSQL database on port 5432
- OAT-DB REST API on port 3001
- pgAdmin (database management UI) on port 5050

### Accessing Services

- **OAT-DB API**: http://localhost:3001
- **API Documentation**: http://localhost:3001/docs
- **pgAdmin**: http://localhost:5050 (admin@oatdb.com / admin)

## Manual Setup

### 1. PostgreSQL Setup

Install and start PostgreSQL:

```bash
# On macOS (with Homebrew)
brew install postgresql
brew services start postgresql

# On Ubuntu/Debian
sudo apt install postgresql postgresql-contrib
sudo systemctl start postgresql

# Create database
createdb -U postgres oatdb
```

### 2. Environment Configuration

Copy the example environment file and configure it:

```bash
cp .env.example .env
```

Edit `.env` to configure your setup:

```env
# Use PostgreSQL
OAT_DATABASE_TYPE=postgres
DATABASE_URL=postgres://postgres:password@localhost:5432/oatdb

# Server configuration
OAT_SERVER_HOST=127.0.0.1
OAT_SERVER_PORT=3001

# Optional: Load seed data on startup
LOAD_SEED_DATA=true
```

### 3. Run the Application

```bash
# Build and run
cargo run

# Or with specific log level
RUST_LOG=info cargo run
```

The application will automatically:
1. Connect to PostgreSQL
2. Run database migrations
3. Start the REST API server

## Configuration Options

### Environment Variables

| Variable | Description | Default | Example |
|----------|-------------|---------|---------|
| `OAT_DATABASE_TYPE` | Database type | `memory` | `postgres` |
| `DATABASE_URL` | PostgreSQL connection string | - | `postgres://user:pass@host:port/db` |
| `OAT_SERVER_HOST` | Server bind address | `127.0.0.1` | `0.0.0.0` |
| `OAT_SERVER_PORT` | Server port | `3001` | `8080` |
| `OAT_DATABASE_MAX_CONNECTIONS` | Connection pool size | `20` | `50` |
| `LOAD_SEED_DATA` | Load example data on startup | `false` | `true` |
| `RUST_LOG` | Log level | `info` | `debug` |

### Configuration File

You can also use a `config.toml` file:

```toml
[server]
host = "127.0.0.1"
port = 3001

[database]
type = "postgres"
connection_string = "postgres://postgres:password@localhost:5432/oatdb"
max_connections = 20
```

## Database Schema

The PostgreSQL integration uses the following tables:

- **databases** - Database definitions
- **branches** - Git-like branches for each database
- **schemas** - Schema definitions (one per branch)
- **class_definitions** - Class/type definitions within schemas
- **instances** - Instance data within branches

All tables support JSONB for flexible property storage and include proper indexing for performance.

## Development Workflow

### Using In-Memory Store (Development)
```bash
# Use default in-memory store
cargo run

# Or explicitly
OAT_DATABASE_TYPE=memory cargo run
```

### Using PostgreSQL (Production-like)
```bash
# Start PostgreSQL with Docker
docker run --name oatdb-postgres \
  -e POSTGRES_DB=oatdb \
  -e POSTGRES_USER=postgres \
  -e POSTGRES_PASSWORD=password \
  -p 5432:5432 -d postgres:15

# Run OAT-DB with PostgreSQL
OAT_DATABASE_TYPE=postgres \
DATABASE_URL=postgres://postgres:password@localhost:5432/oatdb \
cargo run
```

### Migrations

Database migrations are automatically applied on startup. Migration files are in the `migrations/` directory.

To create a new migration:
```bash
# Create new migration file
touch migrations/002_your_migration.sql
```

## API Usage

The REST API is identical regardless of storage backend:

```bash
# List databases
curl http://localhost:3001/databases

# Create a database
curl -X POST http://localhost:3001/databases \
  -H "Content-Type: application/json" \
  -d '{"name": "my-db", "description": "Test database"}'

# View API documentation
open http://localhost:3001/docs
```

## Monitoring and Maintenance

### Database Management

Use pgAdmin (included in Docker Compose):
1. Open http://localhost:5050
2. Login with admin@oatdb.com / admin
3. Add server: postgres / 5432 / oatdb / postgres / password

### Backup and Restore

```bash
# Backup
pg_dump -U postgres -h localhost oatdb > backup.sql

# Restore
psql -U postgres -h localhost oatdb < backup.sql
```

### Performance Monitoring

```bash
# Check connection pool status (add logging to your app)
RUST_LOG=sqlx=debug cargo run

# Monitor PostgreSQL
docker-compose exec postgres psql -U postgres -d oatdb -c "SELECT * FROM pg_stat_activity;"
```

## Troubleshooting

### Common Issues

1. **Connection Refused**
   - Ensure PostgreSQL is running
   - Check connection string format
   - Verify firewall settings

2. **Migration Errors**
   - Check PostgreSQL version compatibility
   - Ensure database user has CREATE privileges
   - Review migration SQL syntax

3. **Performance Issues**
   - Increase connection pool size
   - Add database indexes for your query patterns
   - Monitor slow queries

### Logs

```bash
# Application logs
RUST_LOG=debug cargo run

# Docker Compose logs
docker-compose logs -f oat-db-rust
docker-compose logs -f postgres
```

## Production Deployment

For production deployment:

1. Use a managed PostgreSQL service (AWS RDS, Google Cloud SQL, etc.)
2. Set appropriate connection pool sizes
3. Enable SSL connections
4. Configure backup strategies
5. Set up monitoring and alerting
6. Use container orchestration (Kubernetes, Docker Swarm)

Example production environment:
```bash
OAT_DATABASE_TYPE=postgres
DATABASE_URL=postgres://user:password@prod-db.example.com:5432/oatdb?sslmode=require
OAT_SERVER_HOST=0.0.0.0
OAT_SERVER_PORT=3001
OAT_DATABASE_MAX_CONNECTIONS=50
RUST_LOG=info
```