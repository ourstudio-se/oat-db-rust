-- Complete fix for branches table with proper git-like structure
-- This handles the view dependency issues properly

-- Step 1: Drop the dependent view first
DROP VIEW IF EXISTS database_current_state;

-- Step 2: Check current branches state and fix data issues
-- Let's see what we have now after the partial migration
SELECT 'Current branches:' as info;
SELECT database_id, name, id FROM branches;

-- Step 3: Clean up any duplicate branches that might have been created
-- Remove duplicates keeping the most recent one
DELETE FROM branches 
WHERE (database_id, name) IN (
    SELECT database_id, name
    FROM branches 
    GROUP BY database_id, name 
    HAVING COUNT(*) > 1
) 
AND id NOT IN (
    SELECT MAX(id)
    FROM branches 
    GROUP BY database_id, name
);

-- Step 4: Create missing main branches for databases that don't have them
INSERT INTO branches (id, database_id, name, description, created_at, current_commit_hash, status)
SELECT 
    d.id || '-main',  -- temporary id
    d.id,
    'main',
    'Main branch',
    NOW()::text,
    '',
    'active'
FROM databases d
WHERE NOT EXISTS (
    SELECT 1 FROM branches b 
    WHERE b.database_id = d.id AND b.name = 'main'
);

-- Step 5: Now safely drop constraints and rebuild structure
-- Drop all foreign key constraints first
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_parent_branch_id_fkey;
ALTER TABLE working_commits DROP CONSTRAINT IF EXISTS fk_working_commits_branch;

-- Step 6: Create new columns for working_commits if they don't exist
ALTER TABLE working_commits ADD COLUMN IF NOT EXISTS branch_database_id VARCHAR(255);
ALTER TABLE working_commits ADD COLUMN IF NOT EXISTS branch_name VARCHAR(255);

-- Populate them from existing branch_id references
UPDATE working_commits 
SET 
    branch_database_id = b.database_id,
    branch_name = b.name
FROM branches b 
WHERE b.id = working_commits.branch_id
AND (working_commits.branch_database_id IS NULL OR working_commits.branch_name IS NULL);

-- Step 7: Create new column for databases if it doesn't exist
ALTER TABLE databases ADD COLUMN IF NOT EXISTS default_branch_name VARCHAR(255);
UPDATE databases SET default_branch_name = 'main' WHERE default_branch_name IS NULL;

-- Step 8: Now modify branches table structure safely
-- First try to drop the primary key (might already be gone)
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_pkey;

-- Drop the unique constraint if it exists
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_database_id_name_key;

-- Add the composite primary key (this will fail if there are still duplicates)
-- First check for duplicates and remove them if any
WITH ranked_branches AS (
    SELECT *, ROW_NUMBER() OVER (PARTITION BY database_id, name ORDER BY created_at DESC) as rn
    FROM branches
)
DELETE FROM branches 
WHERE (database_id, name, COALESCE(created_at, '')) IN (
    SELECT database_id, name, COALESCE(created_at, '')
    FROM ranked_branches 
    WHERE rn > 1
);

-- Now add the composite primary key
ALTER TABLE branches ADD PRIMARY KEY (database_id, name);

-- Step 9: Handle parent branch references
-- Add parent_branch_name if it doesn't exist
ALTER TABLE branches ADD COLUMN IF NOT EXISTS parent_branch_name VARCHAR(255);

-- Populate from existing parent_branch_id
UPDATE branches 
SET parent_branch_name = parent.name
FROM branches parent 
WHERE parent.id = branches.parent_branch_id 
AND parent.database_id = branches.database_id
AND branches.parent_branch_name IS NULL;

-- Step 10: Drop old columns (CASCADE to handle dependencies)
ALTER TABLE branches DROP COLUMN IF EXISTS parent_branch_id CASCADE;
ALTER TABLE branches DROP COLUMN IF EXISTS id CASCADE;
ALTER TABLE working_commits DROP COLUMN IF EXISTS branch_id CASCADE;
ALTER TABLE databases DROP COLUMN IF EXISTS default_branch_id CASCADE;

-- Step 11: Add new foreign key constraints
ALTER TABLE working_commits 
ADD CONSTRAINT fk_working_commits_branch 
FOREIGN KEY (branch_database_id, branch_name) 
REFERENCES branches(database_id, name) 
ON DELETE SET NULL;

ALTER TABLE branches 
ADD CONSTRAINT fk_branches_parent 
FOREIGN KEY (database_id, parent_branch_name) 
REFERENCES branches(database_id, name);

ALTER TABLE databases 
ADD CONSTRAINT fk_databases_default_branch 
FOREIGN KEY (id, default_branch_name) 
REFERENCES branches(database_id, name);

-- Step 12: Recreate the view with new structure
CREATE VIEW database_current_state AS
SELECT 
    d.id AS database_id,
    d.name AS database_name,
    d.default_branch_name AS main_branch_name,
    b.name AS branch_name,
    b.current_commit_hash,
    c.created_at AS commit_created_at,
    c.schema_classes_count,
    c.instances_count,
    wc.id AS working_commit_id,
    wc.status AS working_status
FROM databases d
LEFT JOIN branches b ON (d.id = b.database_id AND d.default_branch_name = b.name)
LEFT JOIN commits c ON b.current_commit_hash = c.hash
LEFT JOIN working_commits wc ON (wc.branch_database_id = b.database_id AND wc.branch_name = b.name AND wc.status = 'active');

-- Step 13: Update indexes
DROP INDEX IF EXISTS idx_branches_database_id;
CREATE INDEX IF NOT EXISTS idx_branches_status ON branches(status);
CREATE INDEX IF NOT EXISTS idx_branches_parent ON branches(database_id, parent_branch_name);
CREATE INDEX IF NOT EXISTS idx_working_commits_branch ON working_commits(branch_database_id, branch_name);

-- Step 14: Show final result
SELECT 'Final branches structure:' as info;
SELECT database_id, name, current_commit_hash FROM branches ORDER BY database_id, name;