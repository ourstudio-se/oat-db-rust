use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::model::{ClassDef, Id, Instance};

/// Represents a change operation in the database
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op_type")]
pub enum ChangeOp {
    // Schema operations
    AddClass {
        class: ClassDef,
    },
    DeleteClass {
        class_id: Id,
    },
    PatchClass {
        class_id: Id,
        field_changes: HashMap<String, FieldChange>,
    },
    
    // Instance operations
    AddInstance {
        instance: Instance,
    },
    DeleteInstance {
        instance_id: Id,
    },
    PatchInstance {
        instance_id: Id,
        field_changes: HashMap<String, FieldChange>,
    },
}

/// Represents a field-level change
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldChange {
    pub field_path: Vec<String>,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
}

/// Represents a diff between two commits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDiff {
    pub from_commit: String,
    pub to_commit: String,
    pub operations: Vec<ChangeOp>,
}

/// Represents a merge conflict
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeConflict {
    pub conflict_type: ConflictType,
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub field_path: Option<Vec<String>>,
    pub base_value: Option<serde_json::Value>,
    pub left_value: Option<serde_json::Value>,
    pub right_value: Option<serde_json::Value>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictType {
    /// Both sides added the same resource with different content
    AddAdd,
    /// One side deleted, other side modified
    DeleteModify,
    /// Both sides modified the same field differently
    ModifyModify,
    /// Schema constraint violation after merge
    SchemaConstraint,
    /// Validation error would occur after merge
    ValidationError,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Class,
    Instance,
    Property,
    Relationship,
}

/// Result of a three-way merge operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub success: bool,
    pub conflicts: Vec<MergeConflict>,
    pub merged_operations: Vec<ChangeOp>,
    pub needs_validation: bool,
}

/// Represents a merge in progress
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeState {
    /// The common ancestor commit
    pub base_commit: String,
    /// The left (current) branch commit
    pub left_commit: String,
    /// The right (incoming) branch commit
    pub right_commit: String,
    /// Detected conflicts
    pub conflicts: Vec<MergeConflict>,
    /// Resolved conflicts (conflict index -> chosen resolution)
    pub resolutions: HashMap<usize, ConflictResolution>,
    /// Whether this is a rebase operation
    pub is_rebase: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Take the left (current) branch value
    UseLeft,
    /// Take the right (incoming) branch value
    UseRight,
    /// Use a custom merged value
    UseCustom(serde_json::Value),
    /// Skip this change entirely
    Skip,
}

/// Fields to ignore during diff/merge operations
pub const IGNORED_FIELDS: &[&str] = &[
    "created_at",
    "updated_at",
    "created_by",
    "updated_by",
    "materialized_ids",
    "resolution_details",
];

impl ChangeOp {
    /// Get the resource type and ID affected by this operation
    pub fn resource_info(&self) -> (ResourceType, &str) {
        match self {
            ChangeOp::AddClass { class } => (ResourceType::Class, &class.id),
            ChangeOp::DeleteClass { class_id } => (ResourceType::Class, class_id),
            ChangeOp::PatchClass { class_id, .. } => (ResourceType::Class, class_id),
            ChangeOp::AddInstance { instance } => (ResourceType::Instance, &instance.id),
            ChangeOp::DeleteInstance { instance_id } => (ResourceType::Instance, instance_id),
            ChangeOp::PatchInstance { instance_id, .. } => (ResourceType::Instance, instance_id),
        }
    }

    /// Check if this operation conflicts with another
    pub fn conflicts_with(&self, other: &ChangeOp) -> Option<ConflictType> {
        let (self_type, self_id) = self.resource_info();
        let (other_type, other_id) = other.resource_info();

        // Only operations on the same resource can conflict
        if self_type != other_type || self_id != other_id {
            return None;
        }

        match (self, other) {
            // Both adding the same resource
            (ChangeOp::AddClass { .. }, ChangeOp::AddClass { .. }) |
            (ChangeOp::AddInstance { .. }, ChangeOp::AddInstance { .. }) => {
                Some(ConflictType::AddAdd)
            }

            // Delete vs Modify
            (ChangeOp::DeleteClass { .. }, ChangeOp::PatchClass { .. }) |
            (ChangeOp::PatchClass { .. }, ChangeOp::DeleteClass { .. }) |
            (ChangeOp::DeleteInstance { .. }, ChangeOp::PatchInstance { .. }) |
            (ChangeOp::PatchInstance { .. }, ChangeOp::DeleteInstance { .. }) => {
                Some(ConflictType::DeleteModify)
            }

            // Both modifying - need to check if same fields
            (ChangeOp::PatchClass { field_changes: fc1, .. }, 
             ChangeOp::PatchClass { field_changes: fc2, .. }) |
            (ChangeOp::PatchInstance { field_changes: fc1, .. }, 
             ChangeOp::PatchInstance { field_changes: fc2, .. }) => {
                // Check if any fields overlap
                for (field1, change1) in fc1 {
                    if let Some(change2) = fc2.get(field1) {
                        // Same field modified - check if values differ
                        if change1.new_value != change2.new_value {
                            return Some(ConflictType::ModifyModify);
                        }
                    }
                }
                None
            }

            _ => None,
        }
    }
}

/// Helper to check if a field should be ignored during merge
pub fn should_ignore_field(field_path: &[String]) -> bool {
    field_path.first()
        .map(|f| IGNORED_FIELDS.contains(&f.as_str()))
        .unwrap_or(false)
}