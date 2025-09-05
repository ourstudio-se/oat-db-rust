-- Migration to add commit tagging system
-- This allows commits to be tagged with versions, releases, and other metadata

-- Create commit_tags table for flexible commit labeling
CREATE TABLE commit_tags (
    id SERIAL PRIMARY KEY,
    commit_hash VARCHAR(64) NOT NULL REFERENCES commits(hash) ON DELETE CASCADE,
    tag_type VARCHAR(50) NOT NULL,           -- 'version', 'release', 'milestone', 'custom'
    tag_name VARCHAR(255) NOT NULL,          -- 'v1.0.0', 'prod-release', 'feature-complete', etc.
    tag_description TEXT,                    -- Optional description
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_by VARCHAR(255),                 -- Who created the tag
    metadata JSONB DEFAULT '{}',             -- Additional flexible metadata (can store version info)
    
    -- Ensure unique tag names per commit
    CONSTRAINT unique_tag_name_per_commit UNIQUE (tag_name, commit_hash)
);

-- Indexes for better performance
CREATE INDEX idx_commit_tags_commit_hash ON commit_tags(commit_hash);
CREATE INDEX idx_commit_tags_tag_type ON commit_tags(tag_type);
CREATE INDEX idx_commit_tags_tag_name ON commit_tags(tag_name);
CREATE INDEX idx_commit_tags_created_at ON commit_tags(created_at);

-- GIN index for flexible metadata search (useful for version info stored in metadata)
CREATE INDEX idx_commit_tags_metadata_gin ON commit_tags USING gin(metadata);

-- View to get all tags for a commit with easy access
CREATE VIEW commit_tags_view AS
SELECT 
    c.hash as commit_hash,
    c.database_id,
    c.message as commit_message,
    c.author as commit_author,
    c.created_at as commit_created_at,
    ct.id as tag_id,
    ct.tag_type,
    ct.tag_name,
    ct.tag_description,
    ct.created_by as tag_created_by,
    ct.created_at as tag_created_at,
    ct.metadata as tag_metadata
FROM commits c
LEFT JOIN commit_tags ct ON c.hash = ct.commit_hash
ORDER BY c.created_at DESC, ct.created_at DESC;