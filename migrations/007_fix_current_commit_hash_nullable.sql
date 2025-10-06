-- Fix current_commit_hash to allow NULL values for branches without commits
-- This is needed for new branches that haven't had their first commit yet

-- Make current_commit_hash nullable
ALTER TABLE branches ALTER COLUMN current_commit_hash DROP NOT NULL;

-- Update empty strings to NULL
UPDATE branches SET current_commit_hash = NULL WHERE current_commit_hash = '';

-- Update the foreign key constraint to ensure it's correct
ALTER TABLE branches DROP CONSTRAINT IF EXISTS fk_branches_current_commit;
ALTER TABLE branches ADD CONSTRAINT fk_branches_current_commit 
    FOREIGN KEY (current_commit_hash) REFERENCES commits(hash) ON DELETE SET NULL;

-- Add a check constraint to ensure it's either NULL or a valid hash format
ALTER TABLE branches ADD CONSTRAINT check_current_commit_hash_format
    CHECK (current_commit_hash IS NULL OR length(current_commit_hash) > 0);