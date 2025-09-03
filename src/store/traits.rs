use crate::model::{Branch, ClassDef, Commit, CommitData, Database, Id, Instance, InstanceFilter, NewCommit, NewWorkingCommit, Schema, WorkingCommit};
use anyhow::Result;

#[async_trait::async_trait]
pub trait DatabaseStore: Send + Sync {
    async fn get_database(&self, id: &Id) -> Result<Option<Database>>;
    async fn list_databases(&self) -> Result<Vec<Database>>;
    async fn upsert_database(&self, database: Database) -> Result<()>;
    async fn delete_database(&self, id: &Id) -> Result<bool>;
}

#[async_trait::async_trait]
pub trait BranchStore: Send + Sync {
    async fn get_branch(&self, database_id: &Id, name: &str) -> Result<Option<Branch>>;
    async fn list_branches_for_database(&self, database_id: &Id) -> Result<Vec<Branch>>;
    async fn upsert_branch(&self, branch: Branch) -> Result<()>;
    async fn delete_branch(&self, database_id: &Id, name: &str) -> Result<bool>;
    async fn get_default_branch(&self, database_id: &Id) -> Result<Option<Branch>>;
}

// Keep backward compatibility alias
pub trait VersionStore: BranchStore {}
impl<T: BranchStore> VersionStore for T {}

// Provide version methods as aliases to branch methods for compatibility
#[async_trait::async_trait]
pub trait VersionCompat: BranchStore + SchemaStore + InstanceStore {
    // Legacy methods (deprecated - will need database lookup to convert single ID to database_id + name)
    async fn get_version(&self, _id: &Id) -> Result<Option<Branch>> {
        unimplemented!("get_version is deprecated, use get_branch with database_id and name")
    }
    async fn list_versions_for_database(&self, database_id: &Id) -> Result<Vec<Branch>> {
        self.list_branches_for_database(database_id).await
    }
    async fn upsert_version(&self, version: Branch) -> Result<()> {
        self.upsert_branch(version).await
    }
    async fn delete_version(&self, _id: &Id) -> Result<bool> {
        unimplemented!("delete_version is deprecated, use delete_branch with database_id and name")
    }

    // Schema compatibility methods (deprecated)
    async fn get_schema_for_version(&self, _version_id: &Id) -> Result<Option<Schema>> {
        unimplemented!("get_schema_for_version is deprecated, use get_schema with database_id and branch_name")
    }

    // Instance compatibility methods (deprecated)
    async fn list_instances_for_version(
        &self,
        _version_id: &Id,
        _filter: Option<InstanceFilter>,
    ) -> Result<Vec<Instance>> {
        unimplemented!("list_instances_for_version is deprecated, use list_instances_for_branch with database_id and branch_name")
    }
    async fn find_by_type_in_version(
        &self,
        _version_id: &Id,
        _class_id: &Id,
    ) -> Result<Vec<Instance>> {
        unimplemented!("find_by_type_in_version is deprecated, use find_by_type_in_branch with database_id and branch_name")
    }
}
impl<T: BranchStore + SchemaStore + InstanceStore> VersionCompat for T {}

/// Trait for commit-based schema operations (reads from commit data)
#[async_trait::async_trait]
pub trait SchemaStore: Send + Sync {
    /// Get schema from the current commit of a branch
    async fn get_schema(&self, database_id: &Id, branch_name: &str) -> Result<Option<Schema>>;
    /// Get a class from the current commit of a branch
    async fn get_class(&self, database_id: &Id, branch_name: &str, class_id: &Id) -> Result<Option<ClassDef>>;
}

/// Trait for commit-based instance operations (reads from commit data)
#[async_trait::async_trait]
pub trait InstanceStore: Send + Sync {
    /// Get instance from the current commit of a branch
    async fn get_instance(&self, database_id: &Id, branch_name: &str, id: &Id) -> Result<Option<Instance>>;
    /// List instances from the current commit of a branch
    async fn list_instances_for_branch(
        &self,
        database_id: &Id,
        branch_name: &str,
        filter: Option<InstanceFilter>,
    ) -> Result<Vec<Instance>>;
    /// Find instances by type from the current commit of a branch
    async fn find_by_type_in_branch(
        &self,
        database_id: &Id,
        branch_name: &str,
        class_id: &Id,
    ) -> Result<Vec<Instance>>;
}

#[async_trait::async_trait]
pub trait CommitStore: Send + Sync {
    /// Get a commit by its hash
    async fn get_commit(&self, hash: &str) -> Result<Option<Commit>>;
    /// List commits for a database (with optional parent filtering)
    async fn list_commits_for_database(&self, database_id: &Id, parent_hash: Option<&str>) -> Result<Vec<Commit>>;
    /// Create a new commit from a working commit
    async fn create_commit(&self, commit: NewCommit) -> Result<Commit>;
    /// Get commit data (decompressed schema + instances)
    async fn get_commit_data(&self, hash: &str) -> Result<Option<CommitData>>;
    /// Check if a commit exists
    async fn commit_exists(&self, hash: &str) -> Result<bool>;
}

#[async_trait::async_trait]
pub trait WorkingCommitStore: Send + Sync {
    /// Get a working commit by ID
    async fn get_working_commit(&self, id: &Id) -> Result<Option<WorkingCommit>>;
    /// List working commits for a database/branch
    async fn list_working_commits_for_branch(&self, database_id: &Id, branch_name: &str) -> Result<Vec<WorkingCommit>>;
    /// Create a new working commit
    async fn create_working_commit(&self, database_id: &Id, branch_name: &str, working_commit: NewWorkingCommit) -> Result<WorkingCommit>;
    /// Update a working commit (schema and/or instances)
    async fn update_working_commit(&self, working_commit: WorkingCommit) -> Result<()>;
    /// Delete/abandon a working commit
    async fn delete_working_commit(&self, id: &Id) -> Result<bool>;
    /// Get the active working commit for a branch (if any)
    async fn get_active_working_commit_for_branch(&self, database_id: &Id, branch_name: &str) -> Result<Option<WorkingCommit>>;
}

pub trait Store: DatabaseStore + BranchStore + SchemaStore + InstanceStore + CommitStore + WorkingCommitStore + Send + Sync {}
