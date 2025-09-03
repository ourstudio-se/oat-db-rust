-- Fix branches table to allow each database to have its own "main" branch
-- Changes primary key from single id to composite (database_id, name)
-- This migration handles existing data carefully

-- Step 1: First, let's clean up inconsistent data
-- Fix missing default branches by creating them
INSERT INTO branches (id, database_id, name, description, created_at, current_commit_hash, status)
SELECT 
    d.default_branch_id,
    d.id,
    'main',
    'Main branch',
    NOW()::text,
    '',
    'active'
FROM databases d
LEFT JOIN branches b ON b.id = d.default_branch_id
WHERE b.id IS NULL;

-- Step 2: Normalize branch names - extract actual name from compound IDs
-- Update branch names to remove database prefix (e.g., "furniture_catalog-main" -> "main")
UPDATE branches 
SET name = CASE 
    WHEN name = 'main' THEN 'main'
    WHEN name = 'Add Material Properties' THEN 'feature-materials'  
    WHEN id LIKE '%-main' THEN 'main'
    WHEN id LIKE 'feature-%' THEN REPLACE(id, '-', '_')
    ELSE name 
END;

-- Step 3: Drop existing foreign key constraints that reference branches.id
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_parent_branch_id_fkey;
ALTER TABLE working_commits DROP CONSTRAINT IF EXISTS fk_working_commits_branch;

-- Step 4: Create temporary backup of working_commits references
CREATE TEMP TABLE working_commits_backup AS
SELECT 
    wc.*,
    b.database_id as branch_db_id,
    b.name as branch_name
FROM working_commits wc
LEFT JOIN branches b ON b.id = wc.branch_id;

-- Step 5: Modify working_commits to use composite foreign key
ALTER TABLE working_commits ADD COLUMN branch_database_id VARCHAR(255);
ALTER TABLE working_commits ADD COLUMN branch_name VARCHAR(255);

-- Populate new columns
UPDATE working_commits 
SET branch_database_id = (SELECT branch_db_id FROM working_commits_backup WHERE working_commits_backup.id = working_commits.id),
    branch_name = (SELECT branch_name FROM working_commits_backup WHERE working_commits_backup.id = working_commits.id);

-- Step 6: Modify branches table structure
-- First ensure no duplicate (database_id, name) combinations exist
DELETE FROM branches 
WHERE id IN (
    SELECT id FROM (
        SELECT id, ROW_NUMBER() OVER (PARTITION BY database_id, name ORDER BY created_at DESC) as rn
        FROM branches
    ) ranked WHERE rn > 1
);

-- Drop old primary key and constraints
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_pkey;
ALTER TABLE branches DROP CONSTRAINT IF EXISTS branches_database_id_name_key;

-- Add new composite primary key  
ALTER TABLE branches ADD PRIMARY KEY (database_id, name);

-- Step 7: Update parent branch references to use names within same database
ALTER TABLE branches ADD COLUMN parent_branch_name VARCHAR(255);

UPDATE branches 
SET parent_branch_name = (
    SELECT parent.name 
    FROM branches parent 
    WHERE parent.id = branches.parent_branch_id 
    AND parent.database_id = branches.database_id
);

-- Drop old columns
ALTER TABLE branches DROP COLUMN IF EXISTS parent_branch_id;
ALTER TABLE branches DROP COLUMN IF EXISTS id;

-- Step 8: Update databases table to reference branches properly
ALTER TABLE databases ADD COLUMN default_branch_name VARCHAR(255) DEFAULT 'main';

-- Set default branch name to 'main' for all databases
UPDATE databases SET default_branch_name = 'main';

-- Drop old default_branch_id column
ALTER TABLE databases DROP COLUMN IF EXISTS default_branch_id;

-- Step 9: Drop old branch_id from working_commits
ALTER TABLE working_commits DROP COLUMN IF EXISTS branch_id;

-- Step 10: Add new foreign key constraints
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

-- Step 11: Update indexes
DROP INDEX IF EXISTS idx_branches_database_id;
CREATE INDEX IF NOT EXISTS idx_branches_status ON branches(status);
CREATE INDEX IF NOT EXISTS idx_branches_parent ON branches(database_id, parent_branch_name);

-- Clean up temp table
DROP TABLE IF EXISTS working_commits_backup;