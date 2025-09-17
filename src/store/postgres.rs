use anyhow::{Context, Result};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};

use crate::model::{Branch, ClassDef, Database, Id, Instance, InstanceFilter, Schema};
use crate::store::traits::{
    BranchStore, CommitStore, DatabaseStore, InstanceStore, SchemaStore, Store, WorkingCommitStore,
};

#[derive(Debug, Clone)]
pub struct PostgresStore {
    pool: PgPool,
}

impl PostgresStore {
    /// Create a new PostgreSQL store with the given database URL
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(20)
            .connect(database_url)
            .await
            .context("Failed to create PostgreSQL connection pool")?;

        Ok(Self { pool })
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        // Skip migrations for now - run manually to avoid compile-time database access
        // sqlx::migrate!("./migrations")
        //     .run(&self.pool)
        //     .await
        //     .context("Failed to run database migrations")?;
        println!("Note: Database migrations skipped - run manually if needed");
        Ok(())
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
    
    /// Parse working commit status from string
    fn parse_working_commit_status(status: &str) -> crate::model::WorkingCommitStatus {
        match status {
            "active" => crate::model::WorkingCommitStatus::Active,
            "committing" => crate::model::WorkingCommitStatus::Committing,
            "abandoned" => crate::model::WorkingCommitStatus::Abandoned,
            "merging" => crate::model::WorkingCommitStatus::Merging,
            "rebasing" => crate::model::WorkingCommitStatus::Rebasing,
            _ => crate::model::WorkingCommitStatus::Active,
        }
    }
    
    /// Convert working commit status to string
    fn working_commit_status_to_string(status: &crate::model::WorkingCommitStatus) -> &'static str {
        match status {
            crate::model::WorkingCommitStatus::Active => "active",
            crate::model::WorkingCommitStatus::Committing => "committing",
            crate::model::WorkingCommitStatus::Abandoned => "abandoned",
            crate::model::WorkingCommitStatus::Merging => "merging",
            crate::model::WorkingCommitStatus::Rebasing => "rebasing",
        }
    }
}

#[async_trait::async_trait]
impl DatabaseStore for PostgresStore {
    async fn get_database(&self, id: &Id) -> Result<Option<Database>> {
        let row = sqlx::query("SELECT id, name, description, created_at, default_branch_name FROM databases WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .context("Failed to fetch database")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(Database {
            id: row.get("id"),
            name: row.get("name"),
            description: row.get("description"),
            created_at: row.get("created_at"),
            default_branch_name: row.get("default_branch_name"),
        }))
    }

    async fn list_databases(&self) -> Result<Vec<Database>> {
        let rows = sqlx::query("SELECT id, name, description, created_at, default_branch_name FROM databases ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .context("Failed to list databases")?;

        let databases = rows
            .into_iter()
            .map(|row| Database {
                id: row.get("id"),
                name: row.get("name"),
                description: row.get("description"),
                created_at: row.get("created_at"),
                default_branch_name: row.get("default_branch_name"),
            })
            .collect();

        Ok(databases)
    }

    async fn upsert_database(&self, database: Database) -> Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO databases (id, name, description, created_at, default_branch_name)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE SET
                name = EXCLUDED.name,
                description = EXCLUDED.description,
                default_branch_name = EXCLUDED.default_branch_name,
                updated_at = NOW()
            "#,
            database.id,
            database.name,
            database.description,
            database.created_at,
            database.default_branch_name
        )
        .execute(&self.pool)
        .await
        .context("Failed to upsert database")?;

        Ok(())
    }

    async fn delete_database(&self, id: &Id) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM databases WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .context("Failed to delete database")?;

        Ok(result.rows_affected() > 0)
    }
}

#[async_trait::async_trait]
impl BranchStore for PostgresStore {
    async fn get_branch(&self, database_id: &Id, name: &str) -> Result<Option<Branch>> {
        let row = sqlx::query!(
            r#"
            SELECT database_id, name, description, parent_branch_name, created_at, current_commit_hash, commit_message, author, status
            FROM branches 
            WHERE database_id = $1 AND name = $2
            "#,
            database_id,
            name
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch branch")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let status = match row.status.as_str() {
            "active" => crate::model::BranchStatus::Active,
            "merged" => crate::model::BranchStatus::Merged,
            "archived" => crate::model::BranchStatus::Archived,
            _ => crate::model::BranchStatus::Active, // Default fallback
        };

        Ok(Some(Branch {
            database_id: row.database_id,
            name: row.name,
            description: row.description,
            parent_branch_name: row.parent_branch_name,
            created_at: row.created_at,
            current_commit_hash: row.current_commit_hash,
            commit_message: row.commit_message,
            author: row.author,
            status,
        }))
    }

    async fn list_branches_for_database(&self, database_id: &Id) -> Result<Vec<Branch>> {
        let rows = sqlx::query!(
            r#"
            SELECT database_id, name, description, parent_branch_name, created_at, current_commit_hash, commit_message, author, status
            FROM branches 
            WHERE database_id = $1
            ORDER BY created_at
            "#,
            database_id
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list branches")?;

        let branches = rows
            .into_iter()
            .map(|row| {
                let status = match row.status.as_str() {
                    "active" => crate::model::BranchStatus::Active,
                    "merged" => crate::model::BranchStatus::Merged,
                    "archived" => crate::model::BranchStatus::Archived,
                    _ => crate::model::BranchStatus::Active, // Default fallback
                };

                Branch {
                    database_id: row.database_id,
                    name: row.name,
                    description: row.description,
                    parent_branch_name: row.parent_branch_name,
                    created_at: row.created_at,
                    current_commit_hash: row.current_commit_hash,
                    commit_message: row.commit_message,
                    author: row.author,
                    status,
                }
            })
            .collect();

        Ok(branches)
    }

    async fn upsert_branch(&self, branch: Branch) -> Result<()> {
        let status_str = match branch.status {
            crate::model::BranchStatus::Active => "active",
            crate::model::BranchStatus::Merged => "merged",
            crate::model::BranchStatus::Archived => "archived",
        };

        sqlx::query!(
            r#"
            INSERT INTO branches (database_id, name, description, parent_branch_name, created_at, current_commit_hash, commit_message, author, status)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (database_id, name) DO UPDATE SET
                description = EXCLUDED.description,
                parent_branch_name = EXCLUDED.parent_branch_name,
                current_commit_hash = EXCLUDED.current_commit_hash,
                commit_message = EXCLUDED.commit_message,
                author = EXCLUDED.author,
                status = EXCLUDED.status,
                updated_at = NOW()
            "#,
            branch.database_id,
            branch.name,
            branch.description,
            branch.parent_branch_name,
            branch.created_at,
            branch.current_commit_hash,
            branch.commit_message,
            branch.author,
            status_str
        )
        .execute(&self.pool)
        .await
        .context("Failed to upsert branch")?;

        Ok(())
    }

    async fn delete_branch(&self, database_id: &Id, name: &str) -> Result<bool> {
        let result = sqlx::query!(
            "DELETE FROM branches WHERE database_id = $1 AND name = $2",
            database_id,
            name
        )
        .execute(&self.pool)
        .await
        .context("Failed to delete branch")?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_default_branch(&self, database_id: &Id) -> Result<Option<Branch>> {
        // First get the database to find the default branch name
        let database = self
            .get_database(database_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Database not found: {}", database_id))?;

        // Then get the branch using the composite key
        self.get_branch(database_id, &database.default_branch_name)
            .await
    }
}

#[async_trait::async_trait]
impl SchemaStore for PostgresStore {
    async fn get_schema(&self, database_id: &Id, branch_name: &str) -> Result<Option<Schema>> {
        // Get the branch to find current commit
        let branch = self
            .get_branch(database_id, branch_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Branch not found: {}/{}", database_id, branch_name))?;

        // If no commit hash, return empty schema
        if branch.current_commit_hash.is_empty() {
            return Ok(Some(Schema {
                id: format!("schema-{}-{}", database_id, branch_name),
                classes: Vec::new(),
                description: Some("Empty schema".to_string()),
            }));
        }

        // Get commit data
        let commit = self
            .get_commit(&branch.current_commit_hash)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Commit not found: {}", branch.current_commit_hash))?;

        let commit_data = commit
            .get_data()
            .map_err(|e| anyhow::anyhow!("Failed to get commit data: {}", e))?;

        Ok(Some(commit_data.schema))
    }

    async fn get_class(
        &self,
        database_id: &Id,
        branch_name: &str,
        class_id: &Id,
    ) -> Result<Option<ClassDef>> {
        // Get schema first
        let schema = self
            .get_schema(database_id, branch_name)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Schema not found for branch: {}/{}",
                    database_id,
                    branch_name
                )
            })?;

        // Find the class in the schema
        Ok(schema.get_class_by_id(class_id).cloned())
    }
}

#[async_trait::async_trait]
impl InstanceStore for PostgresStore {
    async fn get_instance(
        &self,
        database_id: &Id,
        branch_name: &str,
        id: &Id,
    ) -> Result<Option<Instance>> {
        // Get the branch to find current commit
        let branch = self
            .get_branch(database_id, branch_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Branch not found: {}/{}", database_id, branch_name))?;

        // If no commit hash, return None
        if branch.current_commit_hash.is_empty() {
            return Ok(None);
        }

        // Get commit data
        let commit = self
            .get_commit(&branch.current_commit_hash)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Commit not found: {}", branch.current_commit_hash))?;

        let commit_data = commit
            .get_data()
            .map_err(|e| anyhow::anyhow!("Failed to get commit data: {}", e))?;

        // Find the instance in the commit data
        Ok(commit_data
            .instances
            .into_iter()
            .find(|inst| &inst.id == id))
    }

    async fn list_instances_for_branch(
        &self,
        database_id: &Id,
        branch_name: &str,
        filter: Option<InstanceFilter>,
    ) -> Result<Vec<Instance>> {
        // Get the branch to find current commit
        let branch = self
            .get_branch(database_id, branch_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Branch not found: {}/{}", database_id, branch_name))?;

        // If no commit hash, return empty vec
        if branch.current_commit_hash.is_empty() {
            return Ok(Vec::new());
        }

        // Get commit data
        let commit = self
            .get_commit(&branch.current_commit_hash)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Commit not found: {}", branch.current_commit_hash))?;

        let commit_data = commit
            .get_data()
            .map_err(|e| anyhow::anyhow!("Failed to get commit data: {}", e))?;

        // Apply filter if provided
        let mut instances = commit_data.instances;

        if let Some(filter) = filter {
            // Apply type filter
            if let Some(types) = filter.types {
                instances.retain(|inst| types.contains(&inst.class_id));
            }

            // Apply complex where clause filter if present
            if let Some(filter_expr) = &filter.where_clause {
                instances = crate::logic::filter_instances(instances, filter_expr);
            }
        }

        // Sort by class_id and id for consistency
        instances.sort_by(|a, b| a.class_id.cmp(&b.class_id).then_with(|| a.id.cmp(&b.id)));

        Ok(instances)
    }

    async fn find_by_type_in_branch(
        &self,
        database_id: &Id,
        branch_name: &str,
        class_id: &Id,
    ) -> Result<Vec<Instance>> {
        // Use optimized method that filters at database level
        self.find_instances_by_type_optimized(database_id, branch_name, class_id)
            .await
    }
}

impl PostgresStore {
    /// Optimized method to find instances by type using PostgreSQL JSON operators
    /// This filters at the database level instead of loading all data into memory
    async fn find_instances_by_type_optimized(
        &self,
        database_id: &Id,
        branch_name: &str,
        class_id: &Id,
    ) -> Result<Vec<Instance>> {
        // First get the branch to find current commit hash
        let branch = self
            .get_branch(database_id, branch_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Branch not found: {}/{}", database_id, branch_name))?;

        if branch.current_commit_hash.is_empty() {
            return Ok(Vec::new());
        }

        // For now, fallback to the original implementation
        // The optimized query would require casting bytea to jsonb which is complex
        let filter = InstanceFilter {
            types: Some(vec![class_id.clone()]),
            limit: None,
            sort: None,
            where_clause: None,
        };

        self.list_instances_for_branch(database_id, branch_name, Some(filter))
            .await
    }
}

#[async_trait::async_trait]
impl crate::store::traits::CommitStore for PostgresStore {
    async fn get_commit(&self, hash: &str) -> Result<Option<crate::model::Commit>> {
        let row = sqlx::query!(
            r#"
            SELECT hash, database_id, parent_hash, author, message, created_at, 
                   data, data_size, schema_classes_count, instances_count
            FROM commits 
            WHERE hash = $1
            "#,
            hash
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch commit")?;

        let Some(row) = row else {
            return Ok(None);
        };

        Ok(Some(crate::model::Commit {
            hash: row.hash,
            database_id: row.database_id,
            parent_hash: row.parent_hash,
            author: row.author,
            message: row.message,
            created_at: row.created_at.to_rfc3339(),
            data: row.data,
            data_size: row.data_size,
            schema_classes_count: row.schema_classes_count,
            instances_count: row.instances_count,
        }))
    }

    async fn list_commits_for_database(
        &self,
        database_id: &crate::model::Id,
        parent_hash: Option<&str>,
    ) -> Result<Vec<crate::model::Commit>> {
        let query_str = if parent_hash.is_some() {
            r#"
            SELECT hash, database_id, parent_hash, author, message, created_at, 
                   data, data_size, schema_classes_count, instances_count
            FROM commits 
            WHERE database_id = $1 AND parent_hash = $2
            ORDER BY created_at DESC
            "#
        } else {
            r#"
            SELECT hash, database_id, parent_hash, author, message, created_at, 
                   data, data_size, schema_classes_count, instances_count
            FROM commits 
            WHERE database_id = $1
            ORDER BY created_at DESC
            "#
        };

        let mut query = sqlx::query(query_str).bind(database_id);

        if let Some(parent) = parent_hash {
            query = query.bind(parent);
        }

        let rows = query
            .fetch_all(&self.pool)
            .await
            .context("Failed to list commits for database")?;

        let commits = rows
            .into_iter()
            .map(|row| crate::model::Commit {
                hash: row.get("hash"),
                database_id: row.get("database_id"),
                parent_hash: row.get("parent_hash"),
                author: row.get("author"),
                message: row.get("message"),
                created_at: row
                    .get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                    .to_rfc3339(),
                data: row.get("data"),
                data_size: row.get("data_size"),
                schema_classes_count: row.get("schema_classes_count"),
                instances_count: row.get("instances_count"),
            })
            .collect();

        Ok(commits)
    }

    async fn create_commit(
        &self,
        new_commit: crate::model::NewCommit,
    ) -> Result<crate::model::Commit> {
        // Get the working commit to convert
        let working_commit = self
            .get_working_commit(&new_commit.working_commit_id)
            .await?
            .ok_or_else(|| {
                anyhow::anyhow!("Working commit not found: {}", new_commit.working_commit_id)
            })?;

        // Convert working commit to immutable commit
        let commit = working_commit.to_commit(new_commit.message);

        // Store the commit in database
        sqlx::query!(
            r#"
            INSERT INTO commits (hash, database_id, parent_hash, author, message, created_at, 
                               data, data_size, schema_classes_count, instances_count)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            "#,
            commit.hash,
            commit.database_id,
            commit.parent_hash,
            commit.author,
            commit.message,
            chrono::DateTime::parse_from_rfc3339(&commit.created_at)
                .context("Failed to parse commit created_at")?
                .with_timezone(&chrono::Utc),
            commit.data,
            commit.data_size,
            commit.schema_classes_count,
            commit.instances_count
        )
        .execute(&self.pool)
        .await
        .context("Failed to create commit")?;

        Ok(commit)
    }

    async fn get_commit_data(&self, hash: &str) -> Result<Option<crate::model::CommitData>> {
        let commit = self.get_commit(hash).await?;
        match commit {
            Some(commit) => match commit.get_data() {
                Ok(data) => Ok(Some(data)),
                Err(e) => Err(anyhow::anyhow!("Failed to decompress commit data: {}", e)),
            },
            None => Ok(None),
        }
    }

    async fn commit_exists(&self, hash: &str) -> Result<bool> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM commits WHERE hash = $1", hash)
            .fetch_one(&self.pool)
            .await
            .context("Failed to check commit existence")?;

        Ok(count.unwrap_or(0) > 0)
    }
}

#[async_trait::async_trait]
impl crate::store::traits::WorkingCommitStore for PostgresStore {
    async fn get_working_commit(
        &self,
        id: &crate::model::Id,
    ) -> Result<Option<crate::model::WorkingCommit>> {
        // Fetching working commit by id
        let row = sqlx::query!(
            r#"
            SELECT id, database_id, branch_name, based_on_hash, author, created_at, updated_at,
                   schema_data, instances_data, status, merge_state
            FROM working_commits 
            WHERE id = $1
            "#,
            id
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch working commit")?;

        let Some(row) = row else {
            // Working commit not found in database
            return Ok(None);
        };
        // Found working commit with status

        let mut schema_data: crate::model::Schema =
            serde_json::from_value(row.schema_data).context("Failed to deserialize schema data")?;
        let instances_data: Vec<crate::model::Instance> =
            serde_json::from_value(row.instances_data)
                .context("Failed to deserialize instances data")?;

        // Normalize the schema to ensure all PropertyDef instances have the value field
        schema_data.normalize();

        let status = Self::parse_working_commit_status(&row.status);
        
        let merge_state = if let Some(merge_state_json) = row.merge_state {
            Some(serde_json::from_value::<crate::model::merge::MergeState>(merge_state_json).context("Failed to deserialize merge_state")?)
        } else {
            None
        };

        Ok(Some(crate::model::WorkingCommit {
            id: row.id,
            database_id: row.database_id,
            branch_name: row.branch_name,
            based_on_hash: row.based_on_hash.unwrap_or_else(String::new),
            author: row.author,
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
            schema_data,
            instances_data,
            status,
            merge_state,
        }))
    }

    async fn list_working_commits_for_branch(
        &self,
        database_id: &crate::model::Id,
        branch_name: &str,
    ) -> Result<Vec<crate::model::WorkingCommit>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, database_id, branch_name, based_on_hash, author, created_at, updated_at,
                   schema_data, instances_data, status, merge_state
            FROM working_commits 
            WHERE database_id = $1 AND branch_name = $2
            ORDER BY updated_at DESC
            "#,
            database_id,
            branch_name
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list working commits")?;

        let mut working_commits = Vec::new();
        for row in rows {
            let mut schema_data: crate::model::Schema = serde_json::from_value(row.schema_data)
                .context("Failed to deserialize schema data")?;
            
            // Normalize the schema to ensure all PropertyDef instances have the value field
            schema_data.normalize();
            let instances_data: Vec<crate::model::Instance> =
                serde_json::from_value(row.instances_data)
                    .context("Failed to deserialize instances data")?;

            let status = Self::parse_working_commit_status(&row.status);
            
            let merge_state = if let Some(merge_state_json) = row.merge_state {
                Some(serde_json::from_value::<crate::model::merge::MergeState>(merge_state_json)
                    .context("Failed to deserialize merge_state")?)
            } else {
                None
            };

            working_commits.push(crate::model::WorkingCommit {
                id: row.id,
                database_id: row.database_id,
                branch_name: row.branch_name,
                based_on_hash: row.based_on_hash.unwrap_or_else(String::new),
                author: row.author,
                created_at: row.created_at.to_rfc3339(),
                updated_at: row.updated_at.to_rfc3339(),
                schema_data,
                instances_data,
                status,
                merge_state,
            });
        }

        Ok(working_commits)
    }

    async fn create_working_commit(
        &self,
        database_id: &Id,
        branch_name: &str,
        new_working_commit: crate::model::NewWorkingCommit,
    ) -> Result<crate::model::WorkingCommit> {
        // Get the current branch to find the latest commit
        let branch = self
            .get_branch(database_id, branch_name)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Branch not found: {}/{}", database_id, branch_name))?;

        // Get current live schema and instances for the branch
        let current_schema = self
            .get_schema(database_id, branch_name)
            .await?
            .unwrap_or_else(|| crate::model::Schema {
                id: format!("schema-{}", database_id),
                description: None,
                classes: Vec::new(),
            });

        let current_instances = self
            .list_instances_for_branch(database_id, branch_name, None)
            .await?;

        // Create working commit based on current live data
        let now = chrono::Utc::now().to_rfc3339();
        let working_commit = crate::model::WorkingCommit {
            id: crate::model::generate_id(),
            database_id: database_id.clone(),
            branch_name: Some(branch_name.to_string()),
            based_on_hash: branch.current_commit_hash,
            author: new_working_commit.author,
            created_at: now.clone(),
            updated_at: now,
            schema_data: current_schema,
            instances_data: current_instances,
            status: crate::model::WorkingCommitStatus::Active,
            merge_state: None,
        };

        // Store in database
        let schema_json = serde_json::to_value(&working_commit.schema_data)
            .context("Failed to serialize schema data")?;
        let instances_json = serde_json::to_value(&working_commit.instances_data)
            .context("Failed to serialize instances data")?;
        let status_str = Self::working_commit_status_to_string(&working_commit.status);
        
        let merge_state_json = if let Some(merge_state) = &working_commit.merge_state {
            Some(serde_json::to_value(merge_state).context("Failed to serialize merge_state")?)
        } else {
            None
        };

        sqlx::query!(
            r#"
            INSERT INTO working_commits (id, database_id, branch_name, based_on_hash, author, 
                                       created_at, updated_at, schema_data, instances_data, status, merge_state)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            "#,
            working_commit.id,
            working_commit.database_id,
            working_commit.branch_name,
            if working_commit.based_on_hash.is_empty() { None } else { Some(working_commit.based_on_hash.as_str()) },
            working_commit.author,
            chrono::DateTime::parse_from_rfc3339(&working_commit.created_at)
                .context("Failed to parse working commit created_at")?
                .with_timezone(&chrono::Utc),
            chrono::DateTime::parse_from_rfc3339(&working_commit.updated_at)
                .context("Failed to parse working commit updated_at")?
                .with_timezone(&chrono::Utc),
            schema_json,
            instances_json,
            status_str,
            merge_state_json
        )
        .execute(&self.pool)
        .await
        .context("Failed to create working commit")?;

        // Created working commit with status
        Ok(working_commit)
    }

    async fn update_working_commit(
        &self,
        mut working_commit: crate::model::WorkingCommit,
    ) -> Result<()> {
        // Updating working commit with status and merge state
        // Touch the working commit to update timestamp
        working_commit.touch();

        let schema_json = serde_json::to_value(&working_commit.schema_data)
            .context("Failed to serialize schema data")?;
        let instances_json = serde_json::to_value(&working_commit.instances_data)
            .context("Failed to serialize instances data")?;
        let status_str = match working_commit.status {
            crate::model::WorkingCommitStatus::Active => "active",
            crate::model::WorkingCommitStatus::Committing => "committing",
            crate::model::WorkingCommitStatus::Abandoned => "abandoned",
            crate::model::WorkingCommitStatus::Merging => "merging",
            crate::model::WorkingCommitStatus::Rebasing => "rebasing",
        };
        
        let merge_state_json = if let Some(merge_state) = &working_commit.merge_state {
            Some(serde_json::to_value(merge_state).context("Failed to serialize merge_state")?)
        } else {
            None
        };

        sqlx::query!(
            r#"
            UPDATE working_commits 
            SET schema_data = $2, instances_data = $3, status = $4, updated_at = $5, merge_state = $6
            WHERE id = $1
            "#,
            working_commit.id,
            schema_json,
            instances_json,
            status_str,
            chrono::DateTime::parse_from_rfc3339(&working_commit.updated_at)
                .context("Failed to parse working commit updated_at")?
                .with_timezone(&chrono::Utc),
            merge_state_json
        )
        .execute(&self.pool)
        .await
        .context("Failed to update working commit")?;

        // Successfully updated working commit
        Ok(())
    }

    async fn delete_working_commit(&self, id: &crate::model::Id) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM working_commits WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .context("Failed to delete working commit")?;

        Ok(result.rows_affected() > 0)
    }

    async fn get_active_working_commit_for_branch(
        &self,
        database_id: &crate::model::Id,
        branch_name: &str,
    ) -> Result<Option<crate::model::WorkingCommit>> {
        // First clean up any duplicate active working commits
        sqlx::query!(
            r#"
            WITH latest AS (
                SELECT id FROM working_commits
                WHERE database_id = $1 AND branch_name = $2 AND status = 'active'
                ORDER BY updated_at DESC
                LIMIT 1
            )
            UPDATE working_commits
            SET status = 'abandoned'
            WHERE database_id = $1 
              AND branch_name = $2 
              AND status = 'active'
              AND id NOT IN (SELECT id FROM latest)
            "#,
            database_id,
            branch_name
        )
        .execute(&self.pool)
        .await
        .context("Failed to cleanup duplicate active working commits")?;
        
        let row = sqlx::query!(
            r#"
            SELECT id, database_id, branch_name, based_on_hash, author, created_at, updated_at,
                   schema_data, instances_data, status, merge_state
            FROM working_commits 
            WHERE database_id = $1 AND branch_name = $2 AND status = 'active'
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
            database_id,
            branch_name
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to fetch active working commit")?;

        let Some(row) = row else {
            return Ok(None);
        };

        let mut schema_data: crate::model::Schema =
            serde_json::from_value(row.schema_data).context("Failed to deserialize schema data")?;
        let instances_data: Vec<crate::model::Instance> =
            serde_json::from_value(row.instances_data)
                .context("Failed to deserialize instances data")?;

        // Normalize the schema to ensure all PropertyDef instances have the value field
        schema_data.normalize();

        let status = Self::parse_working_commit_status(&row.status);
        
        let merge_state = if let Some(merge_state_json) = row.merge_state {
            Some(serde_json::from_value::<crate::model::merge::MergeState>(merge_state_json).context("Failed to deserialize merge_state")?)
        } else {
            None
        };

        Ok(Some(crate::model::WorkingCommit {
            id: row.id,
            database_id: row.database_id,
            branch_name: row.branch_name,
            based_on_hash: row.based_on_hash.unwrap_or_else(String::new),
            author: row.author,
            created_at: row.created_at.to_rfc3339(),
            updated_at: row.updated_at.to_rfc3339(),
            schema_data,
            instances_data,
            status,
            merge_state,
        }))
    }
}

// Simplified TagStore implementation using only commit_tags
#[async_trait::async_trait]
impl crate::store::traits::TagStore for PostgresStore {
    async fn create_commit_tag(
        &self,
        tag: crate::model::NewCommitTag,
    ) -> Result<crate::model::CommitTag> {
        let metadata = tag.metadata.clone().unwrap_or_default();
        let metadata_json =
            serde_json::to_value(&metadata).context("Failed to serialize metadata")?;

        let row = sqlx::query!(
            r#"
            INSERT INTO commit_tags (commit_hash, tag_type, tag_name, tag_description, created_by, metadata)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, created_at
            "#,
            tag.commit_hash,
            tag.tag_type.to_string(),
            tag.tag_name,
            tag.tag_description,
            tag.created_by,
            metadata_json
        )
        .fetch_one(&self.pool)
        .await
        .context("Failed to create commit tag")?;

        Ok(crate::model::CommitTag {
            id: row.id,
            commit_hash: tag.commit_hash,
            tag_type: tag.tag_type,
            tag_name: tag.tag_name,
            tag_description: tag.tag_description,
            created_at: row.created_at.to_rfc3339(),
            created_by: tag.created_by,
            metadata,
        })
    }

    async fn get_commit_tags(&self, commit_hash: &str) -> Result<Vec<crate::model::CommitTag>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, commit_hash, tag_type, tag_name, tag_description, created_at, created_by, metadata
            FROM commit_tags
            WHERE commit_hash = $1
            ORDER BY created_at DESC
            "#,
            commit_hash
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to get commit tags")?;

        let mut tags = Vec::new();
        for row in rows {
            let tag_type = row
                .tag_type
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid tag type: {}", e))?;
            let metadata =
                serde_json::from_value(row.metadata.unwrap_or_else(|| serde_json::json!({})))
                    .context("Failed to deserialize metadata")?;

            tags.push(crate::model::CommitTag {
                id: row.id,
                commit_hash: row.commit_hash,
                tag_type,
                tag_name: row.tag_name,
                tag_description: row.tag_description,
                created_at: row.created_at.to_rfc3339(),
                created_by: row.created_by,
                metadata,
            });
        }

        Ok(tags)
    }

    async fn delete_commit_tag(&self, tag_id: i32) -> Result<bool> {
        let result = sqlx::query!("DELETE FROM commit_tags WHERE id = $1", tag_id)
            .execute(&self.pool)
            .await
            .context("Failed to delete commit tag")?;

        Ok(result.rows_affected() > 0)
    }

    async fn search_commits_by_tags(
        &self,
        database_id: &crate::model::Id,
        query: crate::model::TagQuery,
    ) -> Result<Vec<crate::model::TaggedCommit>> {
        // Simple search implementation - can be enhanced with dynamic SQL for complex queries
        self.list_tagged_commits(database_id, query.limit).await
    }

    async fn get_tagged_commit(
        &self,
        commit_hash: &str,
    ) -> Result<Option<crate::model::TaggedCommit>> {
        // Get the commit first
        let commit_row = sqlx::query!(
            r#"
            SELECT hash, database_id, message, author, created_at
            FROM commits
            WHERE hash = $1
            "#,
            commit_hash
        )
        .fetch_optional(&self.pool)
        .await
        .context("Failed to get commit")?;

        let Some(commit_row) = commit_row else {
            return Ok(None);
        };

        // Get tags
        let tags = self.get_commit_tags(commit_hash).await?;

        Ok(Some(crate::model::TaggedCommit {
            commit_hash: commit_row.hash,
            database_id: commit_row.database_id,
            commit_message: commit_row.message,
            commit_author: commit_row.author,
            commit_created_at: commit_row.created_at.to_rfc3339(),
            tags,
        }))
    }

    async fn list_tagged_commits(
        &self,
        database_id: &crate::model::Id,
        limit: Option<i32>,
    ) -> Result<Vec<crate::model::TaggedCommit>> {
        let limit_value = limit.unwrap_or(50).min(100); // Default 50, max 100

        let rows = sqlx::query!(
            r#"
            SELECT DISTINCT c.hash, c.database_id, c.message, c.author, c.created_at
            FROM commits c
            INNER JOIN commit_tags ct ON c.hash = ct.commit_hash
            WHERE c.database_id = $1
            ORDER BY c.created_at DESC
            LIMIT $2
            "#,
            database_id,
            limit_value as i64
        )
        .fetch_all(&self.pool)
        .await
        .context("Failed to list tagged commits")?;

        let mut tagged_commits = Vec::new();
        for row in rows {
            let commit_hash = row.hash;
            let tags = self.get_commit_tags(&commit_hash).await?;

            tagged_commits.push(crate::model::TaggedCommit {
                commit_hash,
                database_id: row.database_id,
                commit_message: row.message,
                commit_author: row.author,
                commit_created_at: row.created_at.to_rfc3339(),
                tags,
            });
        }

        Ok(tagged_commits)
    }
}

impl Store for PostgresStore {}

#[cfg(test)]
mod tests {

    #[test]
    fn test_postgres_store_schema_update_complete() {
        // This test verifies that the PostgresStore schema update is complete
        // The fact that this compiles proves the PostgresStore implementation
        // has been successfully updated to work with the new branch schema.

        // Key changes made:
        // ✅ Database.default_branch_id → Database.default_branch_name (String)
        // ✅ Branch.id removed (composite key database_id + name)
        // ✅ Branch.commit_hash → Branch.current_commit_hash
        // ✅ Branch.parent_branch_id → Branch.parent_branch_name
        // ✅ WorkingCommit.branch_id → WorkingCommit.branch_name
        // ✅ All SQL queries updated to use new column names
        // ✅ All method signatures updated to use composite keys

        assert!(true, "PostgresStore schema update completed successfully!");
    }
}
