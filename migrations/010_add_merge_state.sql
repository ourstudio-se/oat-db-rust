-- Add merge_state column to working_commits table
ALTER TABLE working_commits 
ADD COLUMN merge_state JSONB DEFAULT NULL;

-- Add index for merge status queries
CREATE INDEX idx_working_commits_status_merge 
ON working_commits(status) 
WHERE status IN ('merging', 'rebasing');

-- Update the status check constraint to include new states
ALTER TABLE working_commits 
DROP CONSTRAINT IF EXISTS working_commits_status_check;

ALTER TABLE working_commits 
ADD CONSTRAINT working_commits_status_check 
CHECK (status IN ('active', 'committing', 'abandoned', 'merging', 'rebasing'));