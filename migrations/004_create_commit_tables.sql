-- Migration to create commit-based tables and complete the git-like architecture
-- This creates the missing commits and working_commits tables

-- Create commits table for immutable commit storage
CREATE TABLE commits (
    hash VARCHAR(64) PRIMARY KEY,              -- SHA-256 hash
    database_id VARCHAR(255) NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    parent_hash VARCHAR(64) REFERENCES commits(hash), -- Git-like parent commit
    author VARCHAR(255),                       -- Commit author
    message TEXT,                             -- Commit message  
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), -- When committed
    data BYTEA NOT NULL,                      -- Compressed schema + instances (gzipped JSON)
    data_size BIGINT NOT NULL,                -- Uncompressed size in bytes
    schema_classes_count INTEGER NOT NULL,     -- Number of classes in schema
    instances_count INTEGER NOT NULL          -- Number of instances
);

-- Create working_commits table for mutable working state
CREATE TABLE working_commits (
    id VARCHAR(255) PRIMARY KEY,              -- Unique working commit ID
    database_id VARCHAR(255) NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    branch_id VARCHAR(255) REFERENCES branches(id) ON DELETE SET NULL, -- Target branch (nullable)
    based_on_hash VARCHAR(64) REFERENCES commits(hash), -- Base commit
    author VARCHAR(255),                      -- Who is making changes
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), -- When created
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(), -- Last modification
    schema_data JSONB NOT NULL,               -- Current schema as JSON
    instances_data JSONB NOT NULL,            -- Current instances as JSON
    status VARCHAR(50) NOT NULL DEFAULT 'active'  -- active/committing/abandoned
);

-- Update branches table to use correct field name expected by Rust code
ALTER TABLE branches RENAME COLUMN commit_hash TO current_commit_hash;

-- Add foreign key constraint from branches to commits
ALTER TABLE branches ADD CONSTRAINT fk_branches_current_commit 
    FOREIGN KEY (current_commit_hash) REFERENCES commits(hash) ON DELETE SET NULL;

-- Indexes for better performance
CREATE INDEX idx_commits_database_id ON commits(database_id);
CREATE INDEX idx_commits_parent_hash ON commits(parent_hash);
CREATE INDEX idx_commits_created_at ON commits(created_at);
CREATE INDEX idx_working_commits_database_id ON working_commits(database_id);
CREATE INDEX idx_working_commits_branch_id ON working_commits(branch_id);
CREATE INDEX idx_working_commits_based_on_hash ON working_commits(based_on_hash);
CREATE INDEX idx_working_commits_status ON working_commits(status);
CREATE INDEX idx_working_commits_updated_at ON working_commits(updated_at);

-- GIN indexes for JSONB columns in working_commits
CREATE INDEX idx_working_commits_schema_data_gin ON working_commits USING gin(schema_data);
CREATE INDEX idx_working_commits_instances_data_gin ON working_commits USING gin(instances_data);

-- Trigger to automatically update updated_at in working_commits
CREATE TRIGGER update_working_commits_updated_at BEFORE UPDATE ON working_commits
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();