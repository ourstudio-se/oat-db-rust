use anyhow::{anyhow, Result};
use std::collections::{HashMap, HashSet};

use crate::model::merge::{
    ChangeOp, CommitDiff, FieldChange, MergeConflict,
    MergeResult,
};
use crate::model::{ClassDef, CommitData, Instance, Schema};
use crate::store::traits::Store;

/// Implements three-way merge algorithm for commits
pub struct MergeEngine;

impl MergeEngine {
    /// Find the common ancestor of two commits
    pub async fn find_common_ancestor<S: Store>(
        store: &S,
        left_commit: &str,
        right_commit: &str,
    ) -> Result<Option<String>> {
        // Build ancestor sets for both commits
        let left_ancestors = Self::get_ancestor_chain(store, left_commit).await?;
        let right_ancestors = Self::get_ancestor_chain(store, right_commit).await?;

        // Find the most recent common ancestor
        for ancestor in &left_ancestors {
            if right_ancestors.contains(ancestor) {
                return Ok(Some(ancestor.clone()));
            }
        }

        Ok(None)
    }

    /// Get the chain of ancestors for a commit (including the commit itself)
    async fn get_ancestor_chain<S: Store>(
        store: &S,
        commit_hash: &str,
    ) -> Result<Vec<String>> {
        let mut ancestors = vec![commit_hash.to_string()];
        let mut current = commit_hash.to_string();

        while let Some(commit) = store.get_commit(&current).await? {
            if let Some(parent) = commit.parent_hash {
                ancestors.push(parent.clone());
                current = parent;
            } else {
                break;
            }
        }

        Ok(ancestors)
    }

    /// Perform a three-way merge
    pub async fn three_way_merge<S: Store>(
        store: &S,
        base_commit: &str,
        left_commit: &str,
        right_commit: &str,
    ) -> Result<MergeResult> {
        // Load all three commits
        let base = store.get_commit(base_commit).await?
            .ok_or_else(|| anyhow!("Base commit not found: {}", base_commit))?;
        let left = store.get_commit(left_commit).await?
            .ok_or_else(|| anyhow!("Left commit not found: {}", left_commit))?;
        let right = store.get_commit(right_commit).await?
            .ok_or_else(|| anyhow!("Right commit not found: {}", right_commit))?;

        // Get the commit data
        let base_data = base.get_data()
            .map_err(|e| anyhow!("Failed to get base commit data: {}", e))?;
        let left_data = left.get_data()
            .map_err(|e| anyhow!("Failed to get left commit data: {}", e))?;
        let right_data = right.get_data()
            .map_err(|e| anyhow!("Failed to get right commit data: {}", e))?;

        // Compute diffs
        let left_diff = Self::compute_diff(&base_data, &left_data)?;
        let right_diff = Self::compute_diff(&base_data, &right_data)?;

        // Merge the diffs
        Ok(Self::merge_diffs(left_diff, right_diff))
    }

    /// Compute diff between two commit data states
    pub fn compute_diff(from: &CommitData, to: &CommitData) -> Result<CommitDiff> {
        let mut operations = Vec::new();

        // Diff schemas
        operations.extend(Self::diff_schemas(&from.schema, &to.schema)?);

        // Diff instances
        operations.extend(Self::diff_instances(&from.instances, &to.instances)?);

        Ok(CommitDiff {
            from_commit: String::new(), // Will be set by caller
            to_commit: String::new(),   // Will be set by caller
            operations,
        })
    }

    /// Diff two schemas
    fn diff_schemas(from: &Schema, to: &Schema) -> Result<Vec<ChangeOp>> {
        let mut ops = Vec::new();

        let from_classes: HashMap<_, _> = from.classes.iter()
            .map(|c| (&c.id, c))
            .collect();
        let to_classes: HashMap<_, _> = to.classes.iter()
            .map(|c| (&c.id, c))
            .collect();

        // Find added classes
        for (id, class) in &to_classes {
            if !from_classes.contains_key(id) {
                ops.push(ChangeOp::AddClass {
                    class: (*class).clone(),
                });
            }
        }

        // Find deleted classes
        for (id, _) in &from_classes {
            if !to_classes.contains_key(id) {
                ops.push(ChangeOp::DeleteClass {
                    class_id: (*id).clone(),
                });
            }
        }

        // Find modified classes
        for (id, to_class) in &to_classes {
            if let Some(from_class) = from_classes.get(id) {
                if let Some(patch_op) = Self::diff_class(from_class, to_class)? {
                    ops.push(patch_op);
                }
            }
        }

        Ok(ops)
    }

    /// Diff two classes
    fn diff_class(from: &ClassDef, to: &ClassDef) -> Result<Option<ChangeOp>> {
        let mut field_changes = HashMap::new();

        // Compare name
        if from.name != to.name {
            field_changes.insert(
                "name".to_string(),
                FieldChange {
                    field_path: vec!["name".to_string()],
                    old_value: Some(serde_json::json!(from.name)),
                    new_value: Some(serde_json::json!(to.name)),
                },
            );
        }

        // Compare description
        if from.description != to.description {
            field_changes.insert(
                "description".to_string(),
                FieldChange {
                    field_path: vec!["description".to_string()],
                    old_value: from.description.as_ref().map(|d| serde_json::json!(d)),
                    new_value: to.description.as_ref().map(|d| serde_json::json!(d)),
                },
            );
        }

        // Compare properties (simplified - could be more granular)
        if from.properties != to.properties {
            field_changes.insert(
                "properties".to_string(),
                FieldChange {
                    field_path: vec!["properties".to_string()],
                    old_value: Some(serde_json::to_value(&from.properties)?),
                    new_value: Some(serde_json::to_value(&to.properties)?),
                },
            );
        }

        // Compare relationships
        if from.relationships != to.relationships {
            field_changes.insert(
                "relationships".to_string(),
                FieldChange {
                    field_path: vec!["relationships".to_string()],
                    old_value: Some(serde_json::to_value(&from.relationships)?),
                    new_value: Some(serde_json::to_value(&to.relationships)?),
                },
            );
        }

        // Compare derived properties
        if from.derived != to.derived {
            field_changes.insert(
                "derived".to_string(),
                FieldChange {
                    field_path: vec!["derived".to_string()],
                    old_value: Some(serde_json::to_value(&from.derived)?),
                    new_value: Some(serde_json::to_value(&to.derived)?),
                },
            );
        }

        // Compare domain constraint
        if from.domain_constraint != to.domain_constraint {
            field_changes.insert(
                "domain_constraint".to_string(),
                FieldChange {
                    field_path: vec!["domain_constraint".to_string()],
                    old_value: Some(serde_json::to_value(&from.domain_constraint)?),
                    new_value: Some(serde_json::to_value(&to.domain_constraint)?),
                },
            );
        }

        if field_changes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ChangeOp::PatchClass {
                class_id: from.id.clone(),
                field_changes,
            }))
        }
    }

    /// Diff two instance lists
    fn diff_instances(from: &[Instance], to: &[Instance]) -> Result<Vec<ChangeOp>> {
        let mut ops = Vec::new();

        let from_instances: HashMap<_, _> = from.iter()
            .map(|i| (&i.id, i))
            .collect();
        let to_instances: HashMap<_, _> = to.iter()
            .map(|i| (&i.id, i))
            .collect();

        // Find added instances
        for (id, instance) in &to_instances {
            if !from_instances.contains_key(id) {
                ops.push(ChangeOp::AddInstance {
                    instance: (*instance).clone(),
                });
            }
        }

        // Find deleted instances
        for (id, _) in &from_instances {
            if !to_instances.contains_key(id) {
                ops.push(ChangeOp::DeleteInstance {
                    instance_id: (*id).clone(),
                });
            }
        }

        // Find modified instances
        for (id, to_instance) in &to_instances {
            if let Some(from_instance) = from_instances.get(id) {
                if let Some(patch_op) = Self::diff_instance(from_instance, to_instance)? {
                    ops.push(patch_op);
                }
            }
        }

        Ok(ops)
    }

    /// Diff two instances
    fn diff_instance(from: &Instance, to: &Instance) -> Result<Option<ChangeOp>> {
        let mut field_changes = HashMap::new();

        // Compare class_id
        if from.class_id != to.class_id {
            field_changes.insert(
                "class_id".to_string(),
                FieldChange {
                    field_path: vec!["class_id".to_string()],
                    old_value: Some(serde_json::json!(from.class_id)),
                    new_value: Some(serde_json::json!(to.class_id)),
                },
            );
        }

        // Compare domain
        if from.domain != to.domain {
            field_changes.insert(
                "domain".to_string(),
                FieldChange {
                    field_path: vec!["domain".to_string()],
                    old_value: from.domain.as_ref().map(|d| serde_json::json!(d)),
                    new_value: to.domain.as_ref().map(|d| serde_json::json!(d)),
                },
            );
        }

        // Compare properties (ignoring metadata fields)
        let from_props = Self::filter_properties(&from.properties);
        let to_props = Self::filter_properties(&to.properties);
        
        if from_props != to_props {
            field_changes.insert(
                "properties".to_string(),
                FieldChange {
                    field_path: vec!["properties".to_string()],
                    old_value: Some(serde_json::to_value(&from_props)?),
                    new_value: Some(serde_json::to_value(&to_props)?),
                },
            );
        }

        // Compare relationships (ignoring materialized_ids and resolution_details)
        let from_rels = Self::filter_relationships(&from.relationships);
        let to_rels = Self::filter_relationships(&to.relationships);

        if from_rels != to_rels {
            field_changes.insert(
                "relationships".to_string(),
                FieldChange {
                    field_path: vec!["relationships".to_string()],
                    old_value: Some(serde_json::to_value(&from_rels)?),
                    new_value: Some(serde_json::to_value(&to_rels)?),
                },
            );
        }

        if field_changes.is_empty() {
            Ok(None)
        } else {
            Ok(Some(ChangeOp::PatchInstance {
                instance_id: from.id.clone(),
                field_changes,
            }))
        }
    }

    /// Filter out ignored fields from properties
    fn filter_properties(
        props: &HashMap<String, crate::model::PropertyValue>,
    ) -> HashMap<String, crate::model::PropertyValue> {
        props.clone() // For now, we don't have metadata in PropertyValue
    }

    /// Filter out materialized_ids and resolution_details from relationships
    fn filter_relationships(
        rels: &HashMap<String, crate::model::RelationshipSelection>,
    ) -> HashMap<String, serde_json::Value> {
        let mut filtered = HashMap::new();
        
        for (key, rel) in rels {
            // Convert to JSON and remove materialized fields
            if let Ok(mut value) = serde_json::to_value(rel) {
                if let Some(obj) = value.as_object_mut() {
                    obj.remove("materialized_ids");
                    obj.remove("resolution_details");
                }
                filtered.insert(key.clone(), value);
            }
        }
        
        filtered
    }

    /// Merge two diffs to produce a final result
    pub fn merge_diffs(left_diff: CommitDiff, right_diff: CommitDiff) -> MergeResult {
        let mut conflicts = Vec::new();
        let mut merged_operations = Vec::new();
        let mut processed_right = HashSet::new();

        // Process all left operations
        for left_op in left_diff.operations {
            let (left_type, left_id) = left_op.resource_info();
            let mut found_conflict = false;

            // Check against all right operations
            for (idx, right_op) in right_diff.operations.iter().enumerate() {
                if let Some(conflict_type) = left_op.conflicts_with(right_op) {
                    let (_, right_id) = right_op.resource_info();
                    
                    // Create conflict description
                    let conflict = MergeConflict {
                        conflict_type: conflict_type.clone(),
                        resource_type: left_type.clone(),
                        resource_id: left_id.to_string(),
                        field_path: None, // TODO: Extract from patch operations
                        base_value: None, // TODO: Would need base data
                        left_value: Some(serde_json::to_value(&left_op).unwrap()),
                        right_value: Some(serde_json::to_value(right_op).unwrap()),
                        description: format!(
                            "{:?} conflict on {:?} '{}'",
                            conflict_type, left_type, left_id
                        ),
                    };
                    
                    conflicts.push(conflict);
                    found_conflict = true;
                    processed_right.insert(idx);
                    break;
                }
            }

            if !found_conflict {
                // No conflict, include left operation
                merged_operations.push(left_op);
            }
        }

        // Add all non-conflicting right operations
        for (idx, right_op) in right_diff.operations.into_iter().enumerate() {
            if !processed_right.contains(&idx) {
                merged_operations.push(right_op);
            }
        }

        MergeResult {
            success: conflicts.is_empty(),
            conflicts,
            merged_operations,
            needs_validation: true,
        }
    }

    /// Apply a merge result to create a new commit data
    pub fn apply_merge_result(
        base_data: &CommitData,
        merge_result: &MergeResult,
    ) -> Result<CommitData> {
        let mut result_data = base_data.clone();

        for op in &merge_result.merged_operations {
            match op {
                ChangeOp::AddClass { class } => {
                    result_data.schema.classes.push(class.clone());
                }
                ChangeOp::DeleteClass { class_id } => {
                    result_data.schema.classes.retain(|c| &c.id != class_id);
                }
                ChangeOp::PatchClass { class_id, field_changes } => {
                    if let Some(class) = result_data.schema.classes.iter_mut().find(|c| &c.id == class_id) {
                        Self::apply_class_patches(class, field_changes)?;
                    }
                }
                ChangeOp::AddInstance { instance } => {
                    result_data.instances.push(instance.clone());
                }
                ChangeOp::DeleteInstance { instance_id } => {
                    result_data.instances.retain(|i| &i.id != instance_id);
                }
                ChangeOp::PatchInstance { instance_id, field_changes } => {
                    if let Some(instance) = result_data.instances.iter_mut().find(|i| &i.id == instance_id) {
                        Self::apply_instance_patches(instance, field_changes)?;
                    }
                }
            }
        }

        // Normalize schema after merge
        result_data.schema.normalize();

        Ok(result_data)
    }

    /// Apply field changes to a class
    fn apply_class_patches(
        class: &mut ClassDef,
        field_changes: &HashMap<String, FieldChange>,
    ) -> Result<()> {
        for (field, change) in field_changes {
            match field.as_str() {
                "name" => {
                    if let Some(new_val) = &change.new_value {
                        class.name = serde_json::from_value(new_val.clone())?;
                    }
                }
                "description" => {
                    if let Some(new_val) = &change.new_value {
                        class.description = serde_json::from_value(new_val.clone())?;
                    }
                }
                "properties" => {
                    if let Some(new_val) = &change.new_value {
                        class.properties = serde_json::from_value(new_val.clone())?;
                    }
                }
                "relationships" => {
                    if let Some(new_val) = &change.new_value {
                        class.relationships = serde_json::from_value(new_val.clone())?;
                    }
                }
                "derived" => {
                    if let Some(new_val) = &change.new_value {
                        class.derived = serde_json::from_value(new_val.clone())?;
                    }
                }
                "domain_constraint" => {
                    if let Some(new_val) = &change.new_value {
                        class.domain_constraint = serde_json::from_value(new_val.clone())?;
                    }
                }
                _ => {
                    // Ignore unknown fields
                }
            }
        }
        Ok(())
    }

    /// Apply field changes to an instance
    fn apply_instance_patches(
        instance: &mut Instance,
        field_changes: &HashMap<String, FieldChange>,
    ) -> Result<()> {
        for (field, change) in field_changes {
            match field.as_str() {
                "class_id" => {
                    if let Some(new_val) = &change.new_value {
                        instance.class_id = serde_json::from_value(new_val.clone())?;
                    }
                }
                "domain" => {
                    if let Some(new_val) = &change.new_value {
                        instance.domain = serde_json::from_value(new_val.clone())?;
                    }
                }
                "properties" => {
                    if let Some(new_val) = &change.new_value {
                        instance.properties = serde_json::from_value(new_val.clone())?;
                    }
                }
                "relationships" => {
                    if let Some(new_val) = &change.new_value {
                        instance.relationships = serde_json::from_value(new_val.clone())?;
                    }
                }
                _ => {
                    // Ignore metadata fields
                }
            }
        }
        Ok(())
    }
}