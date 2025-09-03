-- Migration to remove old non-commit-based tables
-- This completes the transition to pure commit-based architecture

-- Drop old triggers first
DROP TRIGGER IF EXISTS update_instances_updated_at ON instances;
DROP TRIGGER IF EXISTS update_class_definitions_updated_at ON class_definitions;
DROP TRIGGER IF EXISTS update_schemas_updated_at ON schemas;

-- Drop old indexes
DROP INDEX IF EXISTS idx_instances_relationships_gin;
DROP INDEX IF EXISTS idx_instances_properties_gin;
DROP INDEX IF EXISTS idx_class_definitions_relationships_gin;
DROP INDEX IF EXISTS idx_class_definitions_properties_gin;
DROP INDEX IF EXISTS idx_instances_branch_type;
DROP INDEX IF EXISTS idx_instances_type;
DROP INDEX IF EXISTS idx_instances_branch_id;
DROP INDEX IF EXISTS idx_class_definitions_name;
DROP INDEX IF EXISTS idx_class_definitions_schema_id;
DROP INDEX IF EXISTS idx_schemas_branch_id;

-- Drop old tables in correct order (due to foreign key constraints)
DROP TABLE IF EXISTS instances;
DROP TABLE IF EXISTS class_definitions;  
DROP TABLE IF EXISTS schemas;

-- Note: We keep databases and branches tables (commits and working_commits will be created in 004)
-- This migration only removes the non-commit-based tables