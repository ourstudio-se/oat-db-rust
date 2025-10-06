#!/bin/bash
set -e

echo "Starting PostgreSQL..."
service postgresql start

# Wait for PostgreSQL to be ready
until pg_isready -h localhost -p 5432 -U postgres; do
  echo "Waiting for PostgreSQL to be ready..."
  sleep 2
done

echo "PostgreSQL is ready!"

# Run migrations manually since they're disabled in the app
echo "Running database migrations..."
cd /app

# List migrations directory to debug
echo "Checking migrations directory:"
ls -la migrations/ || echo "Migrations directory not found!"

# Run migrations
if [ -d "migrations" ]; then
    for migration in migrations/*.sql; do
        if [ -f "$migration" ]; then
            echo "Running migration: $migration"
            PGPASSWORD=embedded psql -h localhost -U oatadmin -d oatdb -f "$migration" || {
                echo "Warning: Migration $migration had errors (continuing anyway)"
            }
        fi
    done
    echo "Migrations completed (with possible errors)!"
else
    echo "ERROR: No migrations directory found!"
    exit 1
fi

# Apply critical schema fixes
echo "Applying schema fixes to ensure correct structure..."
PGPASSWORD=embedded psql -h localhost -U oatadmin -d oatdb << 'EOF'
-- Ensure databases table has required columns
ALTER TABLE databases ADD COLUMN IF NOT EXISTS default_branch_name TEXT DEFAULT 'main';
ALTER TABLE databases ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
ALTER TABLE databases ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Remove old column if exists
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns 
               WHERE table_name = 'databases' AND column_name = 'default_branch_id') THEN
        ALTER TABLE databases DROP COLUMN default_branch_id CASCADE;
    END IF;
END $$;

-- CRITICAL FIX: Drop the circular foreign key constraint that prevents database creation
ALTER TABLE databases DROP CONSTRAINT IF EXISTS fk_databases_default_branch CASCADE;

-- Ensure branches table has correct structure
ALTER TABLE branches ADD COLUMN IF NOT EXISTS database_id UUID;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS name TEXT;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS current_commit_hash TEXT;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE branches ADD COLUMN IF NOT EXISTS parent_branch_name TEXT;
ALTER TABLE branches ADD COLUMN IF NOT EXISTS created_at TIMESTAMPTZ NOT NULL DEFAULT NOW();
ALTER TABLE branches ADD COLUMN IF NOT EXISTS updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW();

-- Drop any problematic constraints on branches
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_parent_branch_database_id_fkey CASCADE;
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_parent_branch_database_id_parent_branch_name_fkey CASCADE;

-- Fix branches table to allow NULL current_commit_hash (for new branches)
ALTER TABLE branches ALTER COLUMN current_commit_hash DROP NOT NULL;

-- Ensure branches has a proper primary key
DO $$
BEGIN
    -- Drop old primary key if it exists
    IF EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'branches_pkey' AND conrelid = 'branches'::regclass) THEN
        ALTER TABLE branches DROP CONSTRAINT branches_pkey CASCADE;
    END IF;
    -- Add composite primary key
    ALTER TABLE branches ADD PRIMARY KEY (database_id, name);
EXCEPTION
    WHEN others THEN
        -- If we can't add primary key, it might already exist or there are duplicates
        NULL;
END $$;

-- Show all constraints on both tables
\echo 'Constraints on databases table:'
SELECT conname, pg_get_constraintdef(oid) 
FROM pg_constraint 
WHERE conrelid = 'databases'::regclass;

\echo 'Constraints on branches table:'
SELECT conname, pg_get_constraintdef(oid) 
FROM pg_constraint 
WHERE conrelid = 'branches'::regclass;

-- Verify table structures
\echo 'Databases table structure:'
\d databases
\echo 'Branches table structure:'
\d branches

-- Show any existing data to debug
\echo 'Existing branches:'
SELECT * FROM branches LIMIT 5;
EOF

# Show final state
echo "Final database state:"
PGPASSWORD=embedded psql -h localhost -U oatadmin -d oatdb -c "SELECT column_name, data_type FROM information_schema.columns WHERE table_name = 'databases' ORDER BY ordinal_position;"

# Start the application
echo "Starting OAT-DB server..."
exec /usr/local/bin/oat-db-rust