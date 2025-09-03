use serde::{Deserialize, Serialize};

use crate::model::{Id, Schema, Instance, ClassDef};

/// A commit represents an immutable snapshot of a database state
/// Contains compressed binary data with schema + instances
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commit {
    /// SHA-256 hash of the commit content
    pub hash: String,
    /// Database this commit belongs to
    pub database_id: Id,
    /// Parent commit hash (None for initial commit)
    pub parent_hash: Option<String>,
    /// Commit author
    pub author: Option<String>,
    /// Commit message
    pub message: Option<String>,
    /// When the commit was created
    pub created_at: String, // ISO 8601 string
    
    /// Compressed binary data containing schema + instances
    /// This is the actual git-like blob storage
    pub data: Vec<u8>,
    /// Uncompressed size for monitoring
    pub data_size: i64,
    
    /// Metadata for quick access without decompressing
    pub schema_classes_count: i32,
    pub instances_count: i32,
}

/// A working commit represents mutable changes being made to a branch
/// This is the "staging area" before changes are committed
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkingCommit {
    /// Unique identifier for this working commit
    pub id: Id,
    /// Database this working commit belongs to
    pub database_id: Id,
    /// Branch this working commit is for (None for detached head)
    pub branch_name: Option<String>,
    /// Base commit this work is built on
    pub based_on_hash: Option<String>,
    /// Author making the changes
    pub author: Option<String>,
    /// When the working commit was created
    pub created_at: String, // ISO 8601 string
    /// When the working commit was last updated
    pub updated_at: String, // ISO 8601 string
    
    /// Current mutable schema (JSON format)
    pub schema_data: Schema,
    /// Current mutable instances (JSON format)
    pub instances_data: Vec<Instance>,
    
    /// Status of the working commit
    pub status: WorkingCommitStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum WorkingCommitStatus {
    /// Actively being worked on
    Active,
    /// In the process of being committed
    Committing,
    /// Abandoned (will be garbage collected)
    Abandoned,
}

impl Default for WorkingCommitStatus {
    fn default() -> Self {
        WorkingCommitStatus::Active
    }
}

/// Data structure for the content stored in a commit's binary data
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitData {
    /// Schema definition with all classes
    pub schema: Schema,
    /// All instances in the database at this commit
    pub instances: Vec<Instance>,
}

/// Commit creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCommit {
    /// Database to commit to
    pub database_id: Id,
    /// Working commit to convert into immutable commit
    pub working_commit_id: Id,
    /// Commit message
    pub message: String,
    /// Author of the commit
    pub author: Option<String>,
}

/// Working commit creation request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewWorkingCommit {
    /// Author making the changes
    pub author: Option<String>,
}

impl Commit {
    /// Create a new commit with the given data
    pub fn new(
        database_id: Id,
        parent_hash: Option<String>,
        commit_data: CommitData,
        author: Option<String>,
        message: Option<String>,
    ) -> Self {
        let serialized = serde_json::to_string(&commit_data).unwrap();
        let compressed_data = Self::compress_data(serialized.as_bytes());
        let hash = Self::calculate_hash(&database_id, parent_hash.as_deref(), &serialized, author.as_deref(), message.as_deref());
        
        Self {
            hash,
            database_id,
            parent_hash,
            author,
            message,
            created_at: chrono::Utc::now().to_rfc3339(),
            data: compressed_data,
            data_size: serialized.len() as i64,
            schema_classes_count: commit_data.schema.classes.len() as i32,
            instances_count: commit_data.instances.len() as i32,
        }
    }
    
    /// Calculate SHA-256 hash for the commit
    fn calculate_hash(
        database_id: &str,
        parent_hash: Option<&str>,
        data: &str,
        author: Option<&str>,
        message: Option<&str>,
    ) -> String {
        use sha2::{Digest, Sha256};
        
        let mut hasher = Sha256::new();
        hasher.update(format!("database:{}\n", database_id));
        if let Some(parent) = parent_hash {
            hasher.update(format!("parent:{}\n", parent));
        }
        if let Some(author) = author {
            hasher.update(format!("author:{}\n", author));
        }
        if let Some(message) = message {
            hasher.update(format!("message:{}\n", message));
        }
        hasher.update(format!("data:{}\n", data));
        
        hex::encode(hasher.finalize())
    }
    
    /// Compress data using gzip
    fn compress_data(data: &[u8]) -> Vec<u8> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use std::io::Write;
        
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data).unwrap();
        encoder.finish().unwrap()
    }
    
    /// Decompress data from gzip
    fn decompress_data(data: &[u8]) -> Result<Vec<u8>, std::io::Error> {
        use flate2::read::GzDecoder;
        use std::io::Read;
        
        // Check if data is gzip-compressed by looking for gzip magic bytes (1f 8b)
        if data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b {
            // Data is gzip-compressed, decompress it
            let mut decoder = GzDecoder::new(data);
            let mut decompressed = Vec::new();
            decoder.read_to_end(&mut decompressed)?;
            Ok(decompressed)
        } else {
            // Data is not compressed, return as-is
            Ok(data.to_vec())
        }
    }
    
    /// Decompress and deserialize the commit data
    pub fn get_data(&self) -> Result<CommitData, Box<dyn std::error::Error>> {
        let decompressed = Self::decompress_data(&self.data)?;
        let json_str = String::from_utf8(decompressed)?;
        let commit_data: CommitData = serde_json::from_str(&json_str)?;
        Ok(commit_data)
    }
    
    /// Create an empty initial commit
    pub fn create_initial(database_id: Id, author: Option<String>) -> Self {
        let empty_schema = Schema {
            id: format!("schema-{}", database_id),
            // branch_id field removed in commit-based architecture
            description: None,
            classes: Vec::new(),
        };
        
        let commit_data = CommitData {
            schema: empty_schema,
            instances: Vec::new(),
        };
        
        Self::new(
            database_id,
            None, // No parent for initial commit
            commit_data,
            author,
            Some("Initial empty commit".to_string()),
        )
    }
}

impl WorkingCommit {
    /// Create a new working commit based on an existing commit
    pub fn new(
        database_id: Id,
        branch_name: Option<String>,
        based_on_commit: &Commit,
        author: Option<String>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let commit_data = based_on_commit.get_data()?;
        let now = chrono::Utc::now().to_rfc3339();
        
        Ok(Self {
            id: crate::model::generate_id(),
            database_id,
            branch_name,
            based_on_hash: Some(based_on_commit.hash.clone()),
            author,
            created_at: now.clone(),
            updated_at: now,
            schema_data: commit_data.schema,
            instances_data: commit_data.instances,
            status: WorkingCommitStatus::Active,
        })
    }
    
    /// Create an empty working commit (for new databases)
    pub fn create_empty(
        database_id: Id,
        branch_name: Option<String>,
        author: Option<String>,
    ) -> Self {
        let empty_schema = Schema {
            id: format!("schema-{}", database_id),
            // branch_id field removed in commit-based architecture
            description: None,
            classes: Vec::new(),
        };
        
        let now = chrono::Utc::now().to_rfc3339();
        
        Self {
            id: crate::model::generate_id(),
            database_id,
            branch_name,
            based_on_hash: None,
            author,
            created_at: now.clone(),
            updated_at: now,
            schema_data: empty_schema,
            instances_data: Vec::new(),
            status: WorkingCommitStatus::Active,
        }
    }
    
    /// Convert this working commit into an immutable commit
    pub fn to_commit(&self, message: String) -> Commit {
        let commit_data = CommitData {
            schema: self.schema_data.clone(),
            instances: self.instances_data.clone(),
        };
        
        Commit::new(
            self.database_id.clone(),
            self.based_on_hash.clone(),
            commit_data,
            self.author.clone(),
            Some(message),
        )
    }
    
    /// Update the updated_at timestamp
    pub fn touch(&mut self) {
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }
}

/// Working commit changes - shows only what has been added, modified, or deleted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkingCommitChanges {
    /// Unique identifier for this working commit
    pub id: Id,
    /// Database this working commit belongs to
    pub database_id: Id,
    /// Branch this working commit is for
    pub branch_name: Option<String>,
    /// Base commit this work is built on
    pub based_on_hash: Option<String>,
    /// Author making the changes
    pub author: Option<String>,
    /// When the working commit was created
    pub created_at: String,
    /// When the working commit was last updated
    pub updated_at: String,
    /// Status of the working commit
    pub status: WorkingCommitStatus,
    
    /// Changes to schema classes
    pub schema_changes: SchemaChanges,
    /// Changes to instances
    pub instance_changes: InstanceChanges,
}

/// Changes to schema classes
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SchemaChanges {
    /// Newly added classes
    pub added: Vec<ClassDef>,
    /// Modified classes (shows full class definition)
    pub modified: Vec<ClassDef>,
    /// Deleted class IDs
    pub deleted: Vec<Id>,
}

/// Changes to instances
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InstanceChanges {
    /// Newly added instances
    pub added: Vec<Instance>,
    /// Modified instances (shows full instance)
    pub modified: Vec<Instance>,
    /// Deleted instance IDs
    pub deleted: Vec<Id>,
}

impl WorkingCommit {
    /// Generate a diff-style view showing only changes compared to the base commit
    pub async fn to_changes<S>(&self, store: &S) -> Result<WorkingCommitChanges, Box<dyn std::error::Error>>
    where 
        S: crate::store::traits::Store,
    {
        let base_data = if let Some(base_hash) = &self.based_on_hash {
            // Get the base commit data
            match store.get_commit(base_hash).await? {
                Some(base_commit) => {
                    let commit_data = base_commit.get_data()?;
                    Some((commit_data.schema, commit_data.instances))
                }
                None => None,
            }
        } else {
            // No base commit (initial working commit)
            None
        };

        let (base_schema, base_instances) = match base_data {
            Some((schema, instances)) => (schema, instances),
            None => {
                // No base commit - everything is new
                return Ok(WorkingCommitChanges {
                    id: self.id.clone(),
                    database_id: self.database_id.clone(),
                    branch_name: self.branch_name.clone(),
                    based_on_hash: self.based_on_hash.clone(),
                    author: self.author.clone(),
                    created_at: self.created_at.clone(),
                    updated_at: self.updated_at.clone(),
                    status: self.status.clone(),
                    schema_changes: SchemaChanges {
                        added: self.schema_data.classes.clone(),
                        modified: Vec::new(),
                        deleted: Vec::new(),
                    },
                    instance_changes: InstanceChanges {
                        added: self.instances_data.clone(),
                        modified: Vec::new(),
                        deleted: Vec::new(),
                    },
                });
            }
        };

        // Compare schemas
        let schema_changes = Self::diff_schemas(&base_schema, &self.schema_data);
        
        // Compare instances
        let instance_changes = Self::diff_instances(&base_instances, &self.instances_data);

        Ok(WorkingCommitChanges {
            id: self.id.clone(),
            database_id: self.database_id.clone(),
            branch_name: self.branch_name.clone(),
            based_on_hash: self.based_on_hash.clone(),
            author: self.author.clone(),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
            status: self.status.clone(),
            schema_changes,
            instance_changes,
        })
    }

    /// Compare two schemas and return the differences
    fn diff_schemas(base: &Schema, current: &Schema) -> SchemaChanges {
        use std::collections::HashMap;
        
        // Create maps for easier comparison
        let base_classes: HashMap<String, &ClassDef> = base.classes.iter()
            .map(|c| (c.id.clone(), c))
            .collect();
        let current_classes: HashMap<String, &ClassDef> = current.classes.iter()
            .map(|c| (c.id.clone(), c))
            .collect();

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        // Find added and modified classes
        for (id, current_class) in &current_classes {
            match base_classes.get(id) {
                Some(base_class) => {
                    // Class exists in both - check if modified
                    if *base_class != *current_class {
                        modified.push((*current_class).clone());
                    }
                }
                None => {
                    // Class is new
                    added.push((*current_class).clone());
                }
            }
        }

        // Find deleted classes
        for id in base_classes.keys() {
            if !current_classes.contains_key(id) {
                deleted.push(id.clone());
            }
        }

        SchemaChanges {
            added,
            modified,
            deleted,
        }
    }

    /// Compare two instance lists and return the differences
    fn diff_instances(base: &[Instance], current: &[Instance]) -> InstanceChanges {
        use std::collections::HashMap;
        
        // Create maps for easier comparison
        let base_instances: HashMap<String, &Instance> = base.iter()
            .map(|i| (i.id.clone(), i))
            .collect();
        let current_instances: HashMap<String, &Instance> = current.iter()
            .map(|i| (i.id.clone(), i))
            .collect();

        let mut added = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        // Find added and modified instances
        for (id, current_instance) in &current_instances {
            match base_instances.get(id) {
                Some(base_instance) => {
                    // Instance exists in both - check if modified
                    if *base_instance != *current_instance {
                        modified.push((*current_instance).clone());
                    }
                }
                None => {
                    // Instance is new
                    added.push((*current_instance).clone());
                }
            }
        }

        // Find deleted instances
        for id in base_instances.keys() {
            if !current_instances.contains_key(id) {
                deleted.push(id.clone());
            }
        }

        InstanceChanges {
            added,
            modified,
            deleted,
        }
    }
}