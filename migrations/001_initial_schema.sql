-- Initial database schema for oat-db-rust
-- This schema supports the git-like combinatorial database system

-- Function: update_updated_at_column
-- Trigger function to automatically update updated_at timestamps

CREATE OR REPLACE FUNCTION public.update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Function: enforce_single_working_commit_per_status
-- Trigger function to ensure only one active/committing working commit per branch

CREATE OR REPLACE FUNCTION public.enforce_single_working_commit_per_status()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.status IN ('active', 'committing') THEN
        IF EXISTS (
            SELECT 1 FROM working_commits
            WHERE database_id = NEW.database_id
            AND branch_name = NEW.branch_name
            AND status = NEW.status
            AND id != NEW.id
        ) THEN
            RAISE EXCEPTION 'Only one working commit per branch with status % allowed', NEW.status;
        END IF;
    END IF;
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Table: public.databases

-- DROP TABLE IF EXISTS public.databases;

CREATE TABLE IF NOT EXISTS public.databases
(
    id character varying(255) COLLATE pg_catalog."default" NOT NULL,
    name character varying(255) COLLATE pg_catalog."default" NOT NULL,
    description text COLLATE pg_catalog."default",
    created_at character varying(255) COLLATE pg_catalog."default" NOT NULL,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    default_branch_name character varying(255) COLLATE pg_catalog."default" DEFAULT 'main'::character varying,
    CONSTRAINT databases_pkey PRIMARY KEY (id)
)

TABLESPACE pg_default;

ALTER TABLE IF EXISTS public.databases
    OWNER to rikardolsson;

-- Trigger: update_databases_updated_at

-- DROP TRIGGER IF EXISTS update_databases_updated_at ON public.databases;

CREATE OR REPLACE TRIGGER update_databases_updated_at
    BEFORE UPDATE 
    ON public.databases
    FOR EACH ROW
    EXECUTE FUNCTION public.update_updated_at_column();

-- Table: public.commits

-- DROP TABLE IF EXISTS public.commits;

CREATE TABLE IF NOT EXISTS public.commits
(
    hash character varying(64) COLLATE pg_catalog."default" NOT NULL,
    database_id character varying(255) COLLATE pg_catalog."default" NOT NULL,
    parent_hash character varying(64) COLLATE pg_catalog."default",
    author character varying(255) COLLATE pg_catalog."default",
    message text COLLATE pg_catalog."default",
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    data bytea NOT NULL,
    data_size bigint NOT NULL,
    schema_classes_count integer NOT NULL DEFAULT 0,
    instances_count integer NOT NULL DEFAULT 0,
    CONSTRAINT commits_pkey PRIMARY KEY (hash),
    CONSTRAINT fk_commits_database FOREIGN KEY (database_id)
        REFERENCES public.databases (id) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE CASCADE,
    CONSTRAINT fk_commits_parent FOREIGN KEY (parent_hash)
        REFERENCES public.commits (hash) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE NO ACTION
)

TABLESPACE pg_default;

ALTER TABLE IF EXISTS public.commits
    OWNER to rikardolsson;
-- Index: idx_commits_created_at

-- DROP INDEX IF EXISTS public.idx_commits_created_at;

CREATE INDEX IF NOT EXISTS idx_commits_created_at
    ON public.commits USING btree
    (created_at ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_commits_database_id

-- DROP INDEX IF EXISTS public.idx_commits_database_id;

CREATE INDEX IF NOT EXISTS idx_commits_database_id
    ON public.commits USING btree
    (database_id COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_commits_parent_hash

-- DROP INDEX IF EXISTS public.idx_commits_parent_hash;

CREATE INDEX IF NOT EXISTS idx_commits_parent_hash
    ON public.commits USING btree
    (parent_hash COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;

-- Table: public.commit_tags

-- DROP TABLE IF EXISTS public.commit_tags;

CREATE TABLE IF NOT EXISTS public.commit_tags
(
    id integer NOT NULL DEFAULT nextval('commit_tags_id_seq'::regclass),
    commit_hash character varying(64) COLLATE pg_catalog."default" NOT NULL,
    tag_type character varying(50) COLLATE pg_catalog."default" NOT NULL,
    tag_name character varying(255) COLLATE pg_catalog."default" NOT NULL,
    tag_description text COLLATE pg_catalog."default",
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    created_by character varying(255) COLLATE pg_catalog."default",
    metadata jsonb DEFAULT '{}'::jsonb,
    CONSTRAINT commit_tags_pkey PRIMARY KEY (id),
    CONSTRAINT unique_tag_name_per_database UNIQUE (tag_name, commit_hash),
    CONSTRAINT commit_tags_commit_hash_fkey FOREIGN KEY (commit_hash)
        REFERENCES public.commits (hash) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE CASCADE
)

TABLESPACE pg_default;

ALTER TABLE IF EXISTS public.commit_tags
    OWNER to rikardolsson;
-- Index: idx_commit_tags_commit_hash

-- DROP INDEX IF EXISTS public.idx_commit_tags_commit_hash;

CREATE INDEX IF NOT EXISTS idx_commit_tags_commit_hash
    ON public.commit_tags USING btree
    (commit_hash COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_commit_tags_created_at

-- DROP INDEX IF EXISTS public.idx_commit_tags_created_at;

CREATE INDEX IF NOT EXISTS idx_commit_tags_created_at
    ON public.commit_tags USING btree
    (created_at ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_commit_tags_metadata_gin

-- DROP INDEX IF EXISTS public.idx_commit_tags_metadata_gin;

CREATE INDEX IF NOT EXISTS idx_commit_tags_metadata_gin
    ON public.commit_tags USING gin
    (metadata)
    TABLESPACE pg_default;
-- Index: idx_commit_tags_tag_name

-- DROP INDEX IF EXISTS public.idx_commit_tags_tag_name;

CREATE INDEX IF NOT EXISTS idx_commit_tags_tag_name
    ON public.commit_tags USING btree
    (tag_name COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_commit_tags_tag_type

-- DROP INDEX IF EXISTS public.idx_commit_tags_tag_type;

CREATE INDEX IF NOT EXISTS idx_commit_tags_tag_type
    ON public.commit_tags USING btree
    (tag_type COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;

-- Table: public.branches

-- DROP TABLE IF EXISTS public.branches;

CREATE TABLE IF NOT EXISTS public.branches
(
    database_id character varying(255) COLLATE pg_catalog."default" NOT NULL,
    name character varying(255) COLLATE pg_catalog."default" NOT NULL,
    description text COLLATE pg_catalog."default",
    created_at character varying(255) COLLATE pg_catalog."default" NOT NULL,
    current_commit_hash character varying(255) COLLATE pg_catalog."default",
    commit_message text COLLATE pg_catalog."default",
    author character varying(255) COLLATE pg_catalog."default",
    status character varying(50) COLLATE pg_catalog."default" NOT NULL DEFAULT 'active'::character varying,
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    parent_branch_name character varying(255) COLLATE pg_catalog."default",
    CONSTRAINT branches_pkey PRIMARY KEY (database_id, name),
    CONSTRAINT branches_database_id_fkey FOREIGN KEY (database_id)
        REFERENCES public.databases (id) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE CASCADE,
    CONSTRAINT fk_branches_parent FOREIGN KEY (database_id, parent_branch_name)
        REFERENCES public.branches (database_id, name) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE NO ACTION
)

TABLESPACE pg_default;

ALTER TABLE IF EXISTS public.branches
    OWNER to rikardolsson;
-- Index: idx_branches_parent

-- DROP INDEX IF EXISTS public.idx_branches_parent;

CREATE INDEX IF NOT EXISTS idx_branches_parent
    ON public.branches USING btree
    (database_id COLLATE pg_catalog."default" ASC NULLS LAST, parent_branch_name COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_branches_status

-- DROP INDEX IF EXISTS public.idx_branches_status;

CREATE INDEX IF NOT EXISTS idx_branches_status
    ON public.branches USING btree
    (status COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;

-- Trigger: update_branches_updated_at

-- DROP TRIGGER IF EXISTS update_branches_updated_at ON public.branches;

CREATE OR REPLACE TRIGGER update_branches_updated_at
    BEFORE UPDATE 
    ON public.branches
    FOR EACH ROW
    EXECUTE FUNCTION public.update_updated_at_column();

-- Table: public.working_commits

-- DROP TABLE IF EXISTS public.working_commits;

CREATE TABLE IF NOT EXISTS public.working_commits
(
    id character varying(255) COLLATE pg_catalog."default" NOT NULL,
    database_id character varying(255) COLLATE pg_catalog."default" NOT NULL,
    based_on_hash character varying(64) COLLATE pg_catalog."default",
    author character varying(255) COLLATE pg_catalog."default",
    created_at timestamp with time zone NOT NULL DEFAULT now(),
    updated_at timestamp with time zone NOT NULL DEFAULT now(),
    schema_data jsonb NOT NULL DEFAULT '{"classes": []}'::jsonb,
    instances_data jsonb NOT NULL DEFAULT '[]'::jsonb,
    status character varying(50) COLLATE pg_catalog."default" NOT NULL DEFAULT 'active'::character varying,
    branch_database_id character varying(255) COLLATE pg_catalog."default",
    branch_name character varying(255) COLLATE pg_catalog."default",
    merge_state jsonb,
    CONSTRAINT working_commits_pkey PRIMARY KEY (id),
    CONSTRAINT fk_working_commits_based_on FOREIGN KEY (based_on_hash)
        REFERENCES public.commits (hash) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE NO ACTION,
    CONSTRAINT fk_working_commits_branch FOREIGN KEY (branch_database_id, branch_name)
        REFERENCES public.branches (database_id, name) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE SET NULL,
    CONSTRAINT fk_working_commits_database FOREIGN KEY (database_id)
        REFERENCES public.databases (id) MATCH SIMPLE
        ON UPDATE NO ACTION
        ON DELETE CASCADE,
    CONSTRAINT working_commits_status_check CHECK (status::text = ANY (ARRAY['active'::character varying, 'committing'::character varying, 'abandoned'::character varying, 'merging'::character varying, 'rebasing'::character varying]::text[]))
)

TABLESPACE pg_default;

ALTER TABLE IF EXISTS public.working_commits
    OWNER to rikardolsson;
-- Index: idx_working_commits_based_on_hash

-- DROP INDEX IF EXISTS public.idx_working_commits_based_on_hash;

CREATE INDEX IF NOT EXISTS idx_working_commits_based_on_hash
    ON public.working_commits USING btree
    (based_on_hash COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_working_commits_branch

-- DROP INDEX IF EXISTS public.idx_working_commits_branch;

CREATE INDEX IF NOT EXISTS idx_working_commits_branch
    ON public.working_commits USING btree
    (branch_database_id COLLATE pg_catalog."default" ASC NULLS LAST, branch_name COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_working_commits_branch_status

-- DROP INDEX IF EXISTS public.idx_working_commits_branch_status;

CREATE INDEX IF NOT EXISTS idx_working_commits_branch_status
    ON public.working_commits USING btree
    (database_id COLLATE pg_catalog."default" ASC NULLS LAST, branch_name COLLATE pg_catalog."default" ASC NULLS LAST, status COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_working_commits_database_id

-- DROP INDEX IF EXISTS public.idx_working_commits_database_id;

CREATE INDEX IF NOT EXISTS idx_working_commits_database_id
    ON public.working_commits USING btree
    (database_id COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_working_commits_instances_data_gin

-- DROP INDEX IF EXISTS public.idx_working_commits_instances_data_gin;

CREATE INDEX IF NOT EXISTS idx_working_commits_instances_data_gin
    ON public.working_commits USING gin
    (instances_data)
    TABLESPACE pg_default;
-- Index: idx_working_commits_instances_gin

-- DROP INDEX IF EXISTS public.idx_working_commits_instances_gin;

CREATE INDEX IF NOT EXISTS idx_working_commits_instances_gin
    ON public.working_commits USING gin
    (instances_data)
    TABLESPACE pg_default;
-- Index: idx_working_commits_schema_data_gin

-- DROP INDEX IF EXISTS public.idx_working_commits_schema_data_gin;

CREATE INDEX IF NOT EXISTS idx_working_commits_schema_data_gin
    ON public.working_commits USING gin
    (schema_data)
    TABLESPACE pg_default;
-- Index: idx_working_commits_schema_gin

-- DROP INDEX IF EXISTS public.idx_working_commits_schema_gin;

CREATE INDEX IF NOT EXISTS idx_working_commits_schema_gin
    ON public.working_commits USING gin
    (schema_data)
    TABLESPACE pg_default;
-- Index: idx_working_commits_status

-- DROP INDEX IF EXISTS public.idx_working_commits_status;

CREATE INDEX IF NOT EXISTS idx_working_commits_status
    ON public.working_commits USING btree
    (status COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_working_commits_status_merge

-- DROP INDEX IF EXISTS public.idx_working_commits_status_merge;

CREATE INDEX IF NOT EXISTS idx_working_commits_status_merge
    ON public.working_commits USING btree
    (status COLLATE pg_catalog."default" ASC NULLS LAST)
    TABLESPACE pg_default
    WHERE status::text = ANY (ARRAY['merging'::character varying, 'rebasing'::character varying]::text[]);
-- Index: idx_working_commits_updated

-- DROP INDEX IF EXISTS public.idx_working_commits_updated;

CREATE INDEX IF NOT EXISTS idx_working_commits_updated
    ON public.working_commits USING btree
    (database_id COLLATE pg_catalog."default" ASC NULLS LAST, branch_name COLLATE pg_catalog."default" ASC NULLS LAST, updated_at ASC NULLS LAST)
    TABLESPACE pg_default;
-- Index: idx_working_commits_updated_at

-- DROP INDEX IF EXISTS public.idx_working_commits_updated_at;

CREATE INDEX IF NOT EXISTS idx_working_commits_updated_at
    ON public.working_commits USING btree
    (updated_at ASC NULLS LAST)
    TABLESPACE pg_default;

-- Trigger: enforce_single_working_commit

-- DROP TRIGGER IF EXISTS enforce_single_working_commit ON public.working_commits;

CREATE OR REPLACE TRIGGER enforce_single_working_commit
    BEFORE INSERT
    ON public.working_commits
    FOR EACH ROW
    EXECUTE FUNCTION public.enforce_single_working_commit_per_status();

-- Trigger: update_working_commits_updated_at

-- DROP TRIGGER IF EXISTS update_working_commits_updated_at ON public.working_commits;

CREATE OR REPLACE TRIGGER update_working_commits_updated_at
    BEFORE UPDATE 
    ON public.working_commits
    FOR EACH ROW
    EXECUTE FUNCTION public.update_updated_at_column();