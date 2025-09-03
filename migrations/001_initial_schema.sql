-- Initial database schema for oat-db-rust
-- This schema supports the git-like combinatorial database system

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Databases table
CREATE TABLE databases (
    id VARCHAR(255) PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    created_at VARCHAR(255) NOT NULL, -- ISO 8601 string
    default_branch_id VARCHAR(255),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Branches table (git-like branching system)
CREATE TABLE branches (
    id VARCHAR(255) PRIMARY KEY,
    database_id VARCHAR(255) NOT NULL REFERENCES databases(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    parent_branch_id VARCHAR(255) REFERENCES branches(id),
    created_at VARCHAR(255) NOT NULL, -- ISO 8601 string
    commit_hash VARCHAR(255) NOT NULL,
    commit_message TEXT,
    author VARCHAR(255),
    status VARCHAR(50) NOT NULL DEFAULT 'active',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(database_id, name)
);

-- Schemas table (one per branch)
CREATE TABLE schemas (
    id VARCHAR(255) PRIMARY KEY,
    branch_id VARCHAR(255) NOT NULL REFERENCES branches(id) ON DELETE CASCADE UNIQUE,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Class definitions table
CREATE TABLE class_definitions (
    id VARCHAR(255) PRIMARY KEY,
    schema_id VARCHAR(255) NOT NULL REFERENCES schemas(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    properties JSONB NOT NULL DEFAULT '[]',
    relationships JSONB NOT NULL DEFAULT '[]',
    derived JSONB NOT NULL DEFAULT '[]',
    domain_constraint JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(schema_id, name)
);

-- Instances table
CREATE TABLE instances (
    id VARCHAR(255) PRIMARY KEY,
    branch_id VARCHAR(255) NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    instance_type VARCHAR(255) NOT NULL,
    properties JSONB NOT NULL DEFAULT '{}',
    relationships JSONB NOT NULL DEFAULT '{}',
    domain JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for better performance
CREATE INDEX idx_branches_database_id ON branches(database_id);
CREATE INDEX idx_branches_status ON branches(status);
CREATE INDEX idx_schemas_branch_id ON schemas(branch_id);
CREATE INDEX idx_class_definitions_schema_id ON class_definitions(schema_id);
CREATE INDEX idx_class_definitions_name ON class_definitions(name);
CREATE INDEX idx_instances_branch_id ON instances(branch_id);
CREATE INDEX idx_instances_type ON instances(instance_type);
CREATE INDEX idx_instances_branch_type ON instances(branch_id, instance_type);

-- GIN indexes for JSONB columns for efficient JSON queries
CREATE INDEX idx_class_definitions_properties_gin ON class_definitions USING gin(properties);
CREATE INDEX idx_class_definitions_relationships_gin ON class_definitions USING gin(relationships);
CREATE INDEX idx_instances_properties_gin ON instances USING gin(properties);
CREATE INDEX idx_instances_relationships_gin ON instances USING gin(relationships);

-- Function to automatically update updated_at column
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers to automatically update updated_at
CREATE TRIGGER update_databases_updated_at BEFORE UPDATE ON databases
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_branches_updated_at BEFORE UPDATE ON branches
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_schemas_updated_at BEFORE UPDATE ON schemas
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_class_definitions_updated_at BEFORE UPDATE ON class_definitions
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();

CREATE TRIGGER update_instances_updated_at BEFORE UPDATE ON instances
    FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();