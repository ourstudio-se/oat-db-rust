use serde::{Deserialize, Serialize};

use crate::model::{ClassDef, Id, Instance, PropertyValue, RelationshipSelection, Schema};

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
    pub based_on_hash: String,
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
    
    /// Merge state if this is a merge working commit
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge_state: Option<crate::model::merge::MergeState>,
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
    /// In a merge operation with conflicts to resolve
    Merging,
    /// In a rebase operation
    Rebasing,
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
        let hash = Self::calculate_hash(
            &database_id,
            parent_hash.as_deref(),
            &serialized,
            author.as_deref(),
            message.as_deref(),
        );

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
        let mut commit_data: CommitData = serde_json::from_str(&json_str)?;
        
        // Normalize the schema to ensure all PropertyDef instances have the value field
        // This handles migration from older versions that don't have the value field
        commit_data.schema.normalize();
        
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
            based_on_hash: based_on_commit.hash.clone(),
            author,
            created_at: now.clone(),
            updated_at: now,
            schema_data: commit_data.schema,
            instances_data: commit_data.instances,
            status: WorkingCommitStatus::Active,
            merge_state: None,
        })
    }

    /// Convert this working commit into an immutable commit
    pub fn to_commit(&self, message: String) -> Commit {
        let commit_data = CommitData {
            schema: self.schema_data.clone(),
            instances: self.instances_data.clone(),
        };

        // For initial commits, parent_hash should be None
        let parent_hash = if self.based_on_hash.is_empty() {
            None
        } else {
            Some(self.based_on_hash.clone())
        };

        Commit::new(
            self.database_id.clone(),
            parent_hash,
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
    pub based_on_hash: String,
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
    /// Granular field-level changes (only included when requested)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granular_changes: Option<GranularChanges>,
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

/// Type of change operation for granular tracking
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Modified,
    Removed,
}

/// Granular change to a single property
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyChange {
    /// ID of the property that changed
    pub property_id: String,
    /// Previous value (None if property was added)
    pub old_value: Option<PropertyValue>,
    /// New value (None if property was removed)
    pub new_value: Option<PropertyValue>,
    /// Type of change
    pub change_type: ChangeType,
}

/// Granular change to a single relationship
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RelationshipChange {
    /// ID of the relationship that changed
    pub relationship_id: String,
    /// Previous selection (None if relationship was added)
    pub old_selection: Option<RelationshipSelection>,
    /// New selection (None if relationship was removed)
    pub new_selection: Option<RelationshipSelection>,
    /// Type of change
    pub change_type: ChangeType,
}

/// Type of instance-level change
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InstanceChangeType {
    New,
    Modified,
    Deleted,
}

/// Granular changes to a single instance showing field-level deltas
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GranularInstanceChange {
    /// ID of the instance that changed
    pub instance_id: String,
    /// Class ID of the instance
    pub class_id: String,
    /// Changes to individual properties
    pub property_changes: Vec<PropertyChange>,
    /// Changes to individual relationships
    pub relationship_changes: Vec<RelationshipChange>,
    /// Type of instance change
    pub change_type: InstanceChangeType,
}

/// Similar structure for class changes (optional - for consistency)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GranularClassChange {
    /// ID of the class that changed
    pub class_id: String,
    /// Field-level changes (can be expanded later)
    pub field_changes: Vec<String>, // Simplified for now - could be more detailed
    /// Type of class change
    pub change_type: ChangeType,
}

/// Granular changes collection
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GranularChanges {
    /// Granular instance changes with field-level details
    pub instance_changes: Vec<GranularInstanceChange>,
    /// Granular class changes (simplified for now)
    pub class_changes: Vec<GranularClassChange>,
}

impl WorkingCommit {
    /// Generate a diff-style view showing only changes compared to the base commit
    pub async fn to_changes<S>(
        &self,
        store: &S,
    ) -> Result<WorkingCommitChanges, Box<dyn std::error::Error>>
    where
        S: crate::store::traits::Store,
    {
        self.to_changes_with_options(store, false).await
    }

    /// Generate a diff-style view showing only changes compared to the base commit
    /// with optional granular field-level change tracking
    pub async fn to_changes_with_options<S>(
        &self,
        store: &S,
        include_granular: bool,
    ) -> Result<WorkingCommitChanges, Box<dyn std::error::Error>>
    where
        S: crate::store::traits::Store,
    {
        let base_data = match store.get_commit(&self.based_on_hash).await? {
            Some(base_commit) => {
                let commit_data = base_commit.get_data()?;
                Some((commit_data.schema, commit_data.instances))
            }
            None => None,
        };

        let (base_schema, base_instances) = match base_data {
            Some((schema, instances)) => (schema, instances),
            None => {
                // No base commit - everything is new
                let granular_changes = if include_granular {
                    Some(Self::diff_instances_granular(&[], &self.instances_data))
                } else {
                    None
                };

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
                    granular_changes,
                });
            }
        };

        // Compare schemas
        let schema_changes = Self::diff_schemas(&base_schema, &self.schema_data);

        // Compare instances
        let instance_changes = Self::diff_instances(&base_instances, &self.instances_data);

        // Optionally generate granular changes
        let granular_changes = if include_granular {
            Some(Self::diff_instances_granular(
                &base_instances,
                &self.instances_data,
            ))
        } else {
            None
        };

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
            granular_changes,
        })
    }

    /// Compare two schemas and return the differences
    fn diff_schemas(base: &Schema, current: &Schema) -> SchemaChanges {
        use std::collections::HashMap;

        // Create maps for easier comparison
        let base_classes: HashMap<String, &ClassDef> =
            base.classes.iter().map(|c| (c.id.clone(), c)).collect();
        let current_classes: HashMap<String, &ClassDef> =
            current.classes.iter().map(|c| (c.id.clone(), c)).collect();

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
        let base_instances: HashMap<String, &Instance> =
            base.iter().map(|i| (i.id.clone(), i)).collect();
        let current_instances: HashMap<String, &Instance> =
            current.iter().map(|i| (i.id.clone(), i)).collect();

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

    /// Compare two instance lists and return granular field-level differences
    fn diff_instances_granular(base: &[Instance], current: &[Instance]) -> GranularChanges {
        use std::collections::HashMap;

        // Create maps for easier comparison
        let base_instances: HashMap<String, &Instance> =
            base.iter().map(|i| (i.id.clone(), i)).collect();
        let current_instances: HashMap<String, &Instance> =
            current.iter().map(|i| (i.id.clone(), i)).collect();

        let mut instance_changes = Vec::new();

        // Find added instances (all fields are "new")
        for (id, current_instance) in &current_instances {
            if !base_instances.contains_key(id) {
                // Instance is new - all properties and relationships are new
                let mut property_changes = Vec::new();
                let mut relationship_changes = Vec::new();

                // All properties are "added"
                for (prop_id, prop_value) in &current_instance.properties {
                    property_changes.push(PropertyChange {
                        property_id: prop_id.clone(),
                        old_value: None,
                        new_value: Some(prop_value.clone()),
                        change_type: ChangeType::Added,
                    });
                }

                // All relationships are "added"
                for (rel_id, rel_selection) in &current_instance.relationships {
                    relationship_changes.push(RelationshipChange {
                        relationship_id: rel_id.clone(),
                        old_selection: None,
                        new_selection: Some(rel_selection.clone()),
                        change_type: ChangeType::Added,
                    });
                }

                instance_changes.push(GranularInstanceChange {
                    instance_id: id.clone(),
                    class_id: current_instance.class_id.clone(),
                    property_changes,
                    relationship_changes,
                    change_type: InstanceChangeType::New,
                });
            }
        }

        // Find deleted instances (all fields are "removed")
        for (id, base_instance) in &base_instances {
            if !current_instances.contains_key(id) {
                // Instance is deleted - all properties and relationships are removed
                let mut property_changes = Vec::new();
                let mut relationship_changes = Vec::new();

                // All properties are "removed"
                for (prop_id, prop_value) in &base_instance.properties {
                    property_changes.push(PropertyChange {
                        property_id: prop_id.clone(),
                        old_value: Some(prop_value.clone()),
                        new_value: None,
                        change_type: ChangeType::Removed,
                    });
                }

                // All relationships are "removed"
                for (rel_id, rel_selection) in &base_instance.relationships {
                    relationship_changes.push(RelationshipChange {
                        relationship_id: rel_id.clone(),
                        old_selection: Some(rel_selection.clone()),
                        new_selection: None,
                        change_type: ChangeType::Removed,
                    });
                }

                instance_changes.push(GranularInstanceChange {
                    instance_id: id.clone(),
                    class_id: base_instance.class_id.clone(),
                    property_changes,
                    relationship_changes,
                    change_type: InstanceChangeType::Deleted,
                });
            }
        }

        // Find modified instances - compare field by field
        for (id, current_instance) in &current_instances {
            if let Some(base_instance) = base_instances.get(id) {
                // Instance exists in both - compare field by field
                if *base_instance != *current_instance {
                    let mut property_changes = Vec::new();
                    let mut relationship_changes = Vec::new();

                    // Compare properties
                    let all_prop_ids: std::collections::HashSet<String> = base_instance
                        .properties
                        .keys()
                        .chain(current_instance.properties.keys())
                        .cloned()
                        .collect();

                    for prop_id in all_prop_ids {
                        let base_prop = base_instance.properties.get(&prop_id);
                        let current_prop = current_instance.properties.get(&prop_id);

                        match (base_prop, current_prop) {
                            (None, Some(new_val)) => {
                                // Property was added
                                property_changes.push(PropertyChange {
                                    property_id: prop_id,
                                    old_value: None,
                                    new_value: Some(new_val.clone()),
                                    change_type: ChangeType::Added,
                                });
                            }
                            (Some(old_val), None) => {
                                // Property was removed
                                property_changes.push(PropertyChange {
                                    property_id: prop_id,
                                    old_value: Some(old_val.clone()),
                                    new_value: None,
                                    change_type: ChangeType::Removed,
                                });
                            }
                            (Some(old_val), Some(new_val)) => {
                                // Property exists in both - check if modified
                                if old_val != new_val {
                                    property_changes.push(PropertyChange {
                                        property_id: prop_id,
                                        old_value: Some(old_val.clone()),
                                        new_value: Some(new_val.clone()),
                                        change_type: ChangeType::Modified,
                                    });
                                }
                            }
                            (None, None) => {
                                // Should not happen due to how we collect all_prop_ids
                            }
                        }
                    }

                    // Compare relationships
                    let all_rel_ids: std::collections::HashSet<String> = base_instance
                        .relationships
                        .keys()
                        .chain(current_instance.relationships.keys())
                        .cloned()
                        .collect();

                    for rel_id in all_rel_ids {
                        let base_rel = base_instance.relationships.get(&rel_id);
                        let current_rel = current_instance.relationships.get(&rel_id);

                        match (base_rel, current_rel) {
                            (None, Some(new_sel)) => {
                                // Relationship was added
                                relationship_changes.push(RelationshipChange {
                                    relationship_id: rel_id,
                                    old_selection: None,
                                    new_selection: Some(new_sel.clone()),
                                    change_type: ChangeType::Added,
                                });
                            }
                            (Some(old_sel), None) => {
                                // Relationship was removed
                                relationship_changes.push(RelationshipChange {
                                    relationship_id: rel_id,
                                    old_selection: Some(old_sel.clone()),
                                    new_selection: None,
                                    change_type: ChangeType::Removed,
                                });
                            }
                            (Some(old_sel), Some(new_sel)) => {
                                // Relationship exists in both - check if modified
                                if old_sel != new_sel {
                                    relationship_changes.push(RelationshipChange {
                                        relationship_id: rel_id,
                                        old_selection: Some(old_sel.clone()),
                                        new_selection: Some(new_sel.clone()),
                                        change_type: ChangeType::Modified,
                                    });
                                }
                            }
                            (None, None) => {
                                // Should not happen due to how we collect all_rel_ids
                            }
                        }
                    }

                    // Only add to granular changes if there are actual field changes
                    if !property_changes.is_empty() || !relationship_changes.is_empty() {
                        instance_changes.push(GranularInstanceChange {
                            instance_id: id.clone(),
                            class_id: current_instance.class_id.clone(),
                            property_changes,
                            relationship_changes,
                            change_type: InstanceChangeType::Modified,
                        });
                    }
                }
            }
        }

        GranularChanges {
            instance_changes,
            class_changes: Vec::new(), // Simplified for now
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DataType, PropertyValue, TypedValue};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_granular_change_tracking() {
        // Create a base commit with one instance
        let mut base_properties = HashMap::new();
        base_properties.insert(
            "prop-price".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!(500),
                data_type: DataType::Number,
            }),
        );
        base_properties.insert(
            "prop-name".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!("Original Name"),
                data_type: DataType::String,
            }),
        );

        let base_instance = Instance {
            id: "bike-awesome-bike".to_string(),
            class_id: "bike".to_string(),
            domain: None,
            properties: base_properties,
            relationships: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: "test-user".to_string(),
            updated_by: "test-user".to_string(),
        };

        // Create a modified instance with only price changed
        let mut modified_properties = HashMap::new();
        modified_properties.insert(
            "prop-price".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!(400), // Changed from 500 to 400
                data_type: DataType::Number,
            }),
        );
        modified_properties.insert(
            "prop-name".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!("Original Name"), // Unchanged
                data_type: DataType::String,
            }),
        );

        let modified_instance = Instance {
            id: "bike-awesome-bike".to_string(),
            class_id: "bike".to_string(),
            domain: None,
            properties: modified_properties,
            relationships: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: "test-user".to_string(),
            updated_by: "test-user".to_string(),
        };

        // Test granular change detection
        let granular_changes =
            WorkingCommit::diff_instances_granular(&[base_instance], &[modified_instance]);

        // Verify we have one modified instance
        assert_eq!(granular_changes.instance_changes.len(), 1);

        let instance_change = &granular_changes.instance_changes[0];
        assert_eq!(instance_change.instance_id, "bike-awesome-bike");
        assert_eq!(instance_change.change_type, InstanceChangeType::Modified);

        // Verify we have one property change (price)
        assert_eq!(instance_change.property_changes.len(), 1);

        let prop_change = &instance_change.property_changes[0];
        assert_eq!(prop_change.property_id, "prop-price");
        assert_eq!(prop_change.change_type, ChangeType::Modified);

        // Verify old and new values exist
        assert!(prop_change.old_value.is_some());
        assert!(prop_change.new_value.is_some());

        // Verify no relationship changes
        assert_eq!(instance_change.relationship_changes.len(), 0);
    }

    #[tokio::test]
    async fn test_granular_property_addition_removal() {
        // Create a base instance with two properties
        let mut base_properties = HashMap::new();
        base_properties.insert(
            "prop-price".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!(500),
                data_type: DataType::Number,
            }),
        );
        base_properties.insert(
            "prop-name".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!("Bike Name"),
                data_type: DataType::String,
            }),
        );

        let base_instance = Instance {
            id: "bike-test".to_string(),
            class_id: "bike".to_string(),
            domain: None,
            properties: base_properties,
            relationships: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: "test-user".to_string(),
            updated_by: "test-user".to_string(),
        };

        // Create a modified instance: remove name, add description
        let mut modified_properties = HashMap::new();
        modified_properties.insert(
            "prop-price".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!(500), // Unchanged
                data_type: DataType::Number,
            }),
        );
        // prop-name removed
        modified_properties.insert(
            "prop-description".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::json!("New description"), // Added
                data_type: DataType::String,
            }),
        );

        let modified_instance = Instance {
            id: "bike-test".to_string(),
            class_id: "bike".to_string(),
            domain: None,
            properties: modified_properties,
            relationships: HashMap::new(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            created_by: "test-user".to_string(),
            updated_by: "test-user".to_string(),
        };

        // Test granular change detection
        let granular_changes =
            WorkingCommit::diff_instances_granular(&[base_instance], &[modified_instance]);

        // Verify we have one modified instance
        assert_eq!(granular_changes.instance_changes.len(), 1);

        let instance_change = &granular_changes.instance_changes[0];
        assert_eq!(instance_change.instance_id, "bike-test");
        assert_eq!(instance_change.change_type, InstanceChangeType::Modified);

        // Verify we have two property changes (one removed, one added)
        assert_eq!(instance_change.property_changes.len(), 2);

        // Find the changes by property id
        let mut prop_changes_by_id: HashMap<String, &PropertyChange> = HashMap::new();
        for change in &instance_change.property_changes {
            prop_changes_by_id.insert(change.property_id.clone(), change);
        }

        // Verify removed property
        let removed_change = prop_changes_by_id.get("prop-name").unwrap();
        assert_eq!(removed_change.change_type, ChangeType::Removed);
        assert!(removed_change.old_value.is_some());
        assert!(removed_change.new_value.is_none());

        // Verify added property
        let added_change = prop_changes_by_id.get("prop-description").unwrap();
        assert_eq!(added_change.change_type, ChangeType::Added);
        assert!(added_change.old_value.is_none());
        assert!(added_change.new_value.is_some());
    }
}
