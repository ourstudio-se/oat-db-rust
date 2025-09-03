use crate::logic::validate_simple::{SimpleValidator, ValidationResult};
use crate::model::{Branch, Id, Instance};
use crate::store::traits::Store;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

pub struct BranchOperations;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeConflict {
    pub conflict_type: ConflictType,
    pub resource_id: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConflictType {
    SchemaModified,     // Both branches modified the schema
    InstanceModified,   // Same instance modified in both branches
    InstanceDeleted,    // Instance deleted in one branch, modified in other
    ClassAdded,         // Same class name added in both branches with different definitions
    ValidationConflict, // Merging would create validation errors
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MergeResult {
    pub success: bool,
    pub conflicts: Vec<MergeConflict>,
    pub merged_instances: usize,
    pub merged_schema_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeValidationResult {
    pub can_merge: bool,
    pub conflicts: Vec<MergeConflict>,
    pub validation_result: Option<ValidationResult>,
    pub simulated_schema_valid: bool,
    pub affected_instances: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RebaseResult {
    pub success: bool,
    pub conflicts: Vec<MergeConflict>,
    pub message: String,
    pub rebased_instances: usize,
    pub rebased_schema_changes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseValidationResult {
    pub can_rebase: bool,
    pub conflicts: Vec<MergeConflict>,
    pub validation_result: Option<ValidationResult>,
    pub needs_rebase: bool,
    pub affected_instances: Vec<String>,
}

impl BranchOperations {
    /// Merge a feature branch into a target branch (usually main)
    pub async fn merge_branch<S: Store>(
        store: &S,
        source_database_id: &Id,
        source_branch_name: &str,
        target_database_id: &Id,
        target_branch_name: &str,
        author: Option<String>,
        force: bool,
    ) -> Result<MergeResult> {
        // Get both branches
        let source_branch = store
            .get_branch(source_database_id, source_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Source branch '{}' not found", source_branch_name))?;
        let target_branch = store
            .get_branch(target_database_id, target_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Target branch '{}' not found", target_branch_name))?;

        // Validate merge preconditions
        if !source_branch.can_be_merged() {
            return Err(anyhow!(
                "Source branch '{}' cannot be merged (status: {:?})",
                source_branch.name,
                source_branch.status
            ));
        }

        if source_branch.database_id != target_branch.database_id {
            return Err(anyhow!("Cannot merge branches from different databases"));
        }

        // Detect conflicts
        let conflicts = Self::detect_conflicts(store, &source_branch, &target_branch).await?;

        if !conflicts.is_empty() && !force {
            return Ok(MergeResult {
                success: false,
                conflicts,
                merged_instances: 0,
                merged_schema_changes: false,
            });
        }

        // Perform the merge
        let merge_result =
            Self::perform_merge(store, &source_branch, &target_branch, author).await?;

        // Mark source branch as merged
        let mut source_branch_updated = source_branch;
        source_branch_updated.mark_as_merged(Some(format!("Merged into '{}'", target_branch.name)));
        store.upsert_branch(source_branch_updated).await?;

        Ok(merge_result)
    }

    /// Delete a branch (only if it's merged or archived)
    pub async fn delete_branch<S: Store>(store: &S, database_id: &Id, branch_name: &str, force: bool) -> Result<bool> {
        let branch = store
            .get_branch(database_id, branch_name)
            .await?
            .ok_or_else(|| anyhow!("Branch '{}' not found", branch_name))?;

        if !branch.can_be_deleted() && !force {
            return Err(anyhow!(
                "Cannot delete active branch '{}'. Branch must be merged or archived first.",
                branch.name
            ));
        }

        // Delete associated schema and instances
        Self::cleanup_branch_data(store, database_id, branch_name).await?;

        // Delete the branch itself
        store.delete_branch(database_id, branch_name).await
    }

    async fn detect_conflicts<S: Store>(
        store: &S,
        source_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<Vec<MergeConflict>> {
        let mut conflicts = Vec::new();

        // Check schema conflicts
        if let Some(schema_conflicts) =
            Self::detect_schema_conflicts(store, source_branch, target_branch).await?
        {
            conflicts.extend(schema_conflicts);
        }

        // Check instance conflicts
        if let Some(instance_conflicts) =
            Self::detect_instance_conflicts(store, source_branch, target_branch).await?
        {
            conflicts.extend(instance_conflicts);
        }

        // Check validation conflicts (new!)
        if let Some(validation_conflicts) =
            Self::detect_validation_conflicts(store, source_branch, target_branch).await?
        {
            conflicts.extend(validation_conflicts);
        }

        Ok(conflicts)
    }

    async fn detect_schema_conflicts<S: Store>(
        store: &S,
        source_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<Option<Vec<MergeConflict>>> {
        let source_schema = store.get_schema(&source_branch.database_id, &source_branch.name).await?;
        let target_schema = store.get_schema(&target_branch.database_id, &target_branch.name).await?;

        match (source_schema, target_schema) {
            (Some(source), Some(target)) => {
                let mut conflicts = Vec::new();

                // Check if both schemas were modified (simplified check)
                if source.id != target.id {
                    conflicts.push(MergeConflict {
                        conflict_type: ConflictType::SchemaModified,
                        resource_id: "schema".to_string(),
                        description: "Schema modified in both branches".to_string(),
                    });
                }

                // Check for class conflicts
                let _source_classes: HashSet<&String> =
                    source.classes.iter().map(|c| &c.name).collect();
                let _target_classes: HashSet<&String> =
                    target.classes.iter().map(|c| &c.name).collect();

                for source_class in &source.classes {
                    if let Some(target_class) =
                        target.classes.iter().find(|c| c.name == source_class.name)
                    {
                        // Same class exists in both - check if they're different
                        if source_class != target_class {
                            conflicts.push(MergeConflict {
                                conflict_type: ConflictType::ClassAdded,
                                resource_id: source_class.name.clone(),
                                description: format!(
                                    "Class '{}' modified in both branches",
                                    source_class.name
                                ),
                            });
                        }
                    }
                }

                Ok(if conflicts.is_empty() {
                    None
                } else {
                    Some(conflicts)
                })
            }
            _ => Ok(None), // No conflicts if one or both schemas don't exist
        }
    }

    async fn detect_instance_conflicts<S: Store>(
        store: &S,
        source_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<Option<Vec<MergeConflict>>> {
        let source_instances = store
            .list_instances_for_branch(&source_branch.database_id, &source_branch.name, None)
            .await?;
        let target_instances = store
            .list_instances_for_branch(&target_branch.database_id, &target_branch.name, None)
            .await?;

        let target_instance_map: HashMap<&String, &Instance> =
            target_instances.iter().map(|i| (&i.id, i)).collect();

        let mut conflicts = Vec::new();

        for source_instance in &source_instances {
            if let Some(target_instance) = target_instance_map.get(&source_instance.id) {
                // Same instance exists in both branches - check if they differ
                if source_instance != *target_instance {
                    conflicts.push(MergeConflict {
                        conflict_type: ConflictType::InstanceModified,
                        resource_id: source_instance.id.clone(),
                        description: format!(
                            "Instance '{}' modified in both branches",
                            source_instance.id
                        ),
                    });
                }
            }
        }

        Ok(if conflicts.is_empty() {
            None
        } else {
            Some(conflicts)
        })
    }

    async fn perform_merge<S: Store>(
        store: &S,
        source_branch: &Branch,
        target_branch: &Branch,
        _author: Option<String>,
    ) -> Result<MergeResult> {
        // TODO: Update for new commit-based architecture - branch operations currently disabled
        return Err(anyhow::anyhow!("Merge operations disabled pending commit-based architecture update"));
        
        let mut merged_instances = 0;
        let mut merged_schema_changes = false;

        // Merge schema (source takes precedence)
        if let Some(source_schema) = store.get_schema(&source_branch.database_id, &source_branch.name).await? {
            let mut target_schema = source_schema.clone();
            // target_schema.branch_id = target_branch.name.clone(); // branch_id field removed
            // store.upsert_schema(target_schema).await?; // upsert_schema method removed
            merged_schema_changes = true;
        }

        // Merge instances (source takes precedence for conflicts)
        let source_instances = store
            .list_instances_for_branch(&source_branch.database_id, &source_branch.name, None)
            .await?;
        for mut instance in source_instances {
            // instance.branch_id = target_branch.name.clone(); // branch_id field removed
            // store.upsert_instance(instance).await?; // upsert_instance method removed
            merged_instances += 1;
        }

        Ok(MergeResult {
            success: true,
            conflicts: Vec::new(),
            merged_instances,
            merged_schema_changes,
        })
    }

    async fn cleanup_branch_data<S: Store>(store: &S, database_id: &Id, branch_name: &str) -> Result<()> {
        // TODO: Update for new commit-based architecture - branch cleanup currently disabled
        return Err(anyhow::anyhow!("Branch cleanup operations disabled pending commit-based architecture update"));
        
        // Delete schema for this branch
        // store.delete_schema_for_branch(branch_id).await?; // delete_schema_for_branch method removed

        // Delete all instances for this branch
        let instances = store.list_instances_for_branch(database_id, branch_name, None).await?;
        for instance in instances {
            // store.delete_instance(&instance.id).await?; // delete_instance method removed
        }

        Ok(())
    }

    /// Detect validation conflicts that would occur after merge
    async fn detect_validation_conflicts<S: Store>(
        store: &S,
        source_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<Option<Vec<MergeConflict>>> {
        // Simulate the merge and validate the result
        let merge_validation =
            Self::validate_merge_compatibility(store, source_branch, target_branch).await?;

        if !merge_validation.can_merge && merge_validation.validation_result.is_some() {
            let validation_result = merge_validation.validation_result.unwrap();
            if !validation_result.valid {
                let mut conflicts = Vec::new();

                // Convert validation errors to merge conflicts
                for error in validation_result.errors {
                    conflicts.push(MergeConflict {
                        conflict_type: ConflictType::ValidationConflict,
                        resource_id: error.instance_id.clone(),
                        description: format!(
                            "Merge would create validation error: {} (Instance: {})",
                            error.message, error.instance_id
                        ),
                    });
                }

                return Ok(Some(conflicts));
            }
        }

        Ok(None)
    }

    /// Validate merge compatibility by simulating the merge result
    pub async fn validate_merge_compatibility<S: Store>(
        store: &S,
        source_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<MergeValidationResult> {
        // Get schemas from both branches
        let source_schema = store.get_schema(&source_branch.database_id, &source_branch.name).await?;
        let target_schema = store.get_schema(&target_branch.database_id, &target_branch.name).await?;

        // Get instances from both branches
        let source_instances = store
            .list_instances_for_branch(&source_branch.database_id, &source_branch.name, None)
            .await?;
        let target_instances = store
            .list_instances_for_branch(&target_branch.database_id, &target_branch.name, None)
            .await?;

        let mut result = MergeValidationResult {
            can_merge: true,
            conflicts: Vec::new(),
            validation_result: None,
            simulated_schema_valid: true,
            affected_instances: Vec::new(),
        };

        // Create simulated merged schema (source takes precedence)
        let merged_schema = if let Some(source_schema) = source_schema {
            source_schema
        } else if let Some(target_schema) = target_schema {
            target_schema
        } else {
            result.simulated_schema_valid = false;
            result.can_merge = false;
            return Ok(result);
        };

        // Collect all instances that would exist after merge
        // Source instances override target instances with same ID
        let mut merged_instances = HashMap::new();

        // Add target instances first
        for instance in target_instances {
            merged_instances.insert(instance.id.clone(), instance);
        }

        // Add source instances (overriding any conflicts)
        for instance in source_instances {
            merged_instances.insert(instance.id.clone(), instance);
        }

        // Validate all instances against the merged schema
        let mut validation_errors = Vec::new();
        let mut validation_warnings = Vec::new();
        let mut validated_instances = Vec::new();

        for (instance_id, instance) in &merged_instances {
            validated_instances.push(instance_id.clone());
            result.affected_instances.push(instance_id.clone());

            match SimpleValidator::validate_instance(store, instance, &merged_schema).await {
                Ok(instance_validation) => {
                    if !instance_validation.valid {
                        result.can_merge = false;
                        validation_errors.extend(instance_validation.errors);
                        validation_warnings.extend(instance_validation.warnings);
                    }
                }
                Err(e) => {
                    result.can_merge = false;
                    // Convert error to validation error format
                    validation_errors.push(crate::logic::validate_simple::ValidationError {
                        instance_id: instance_id.clone(),
                        error_type:
                            crate::logic::validate_simple::ValidationErrorType::InvalidValue,
                        message: format!("Validation failed: {}", e),
                        property_name: None,
                        expected: None,
                        actual: None,
                    });
                }
            }
        }

        result.validation_result = Some(ValidationResult {
            valid: validation_errors.is_empty(),
            errors: validation_errors,
            warnings: validation_warnings,
            instance_count: merged_instances.len(),
            validated_instances,
        });

        Ok(result)
    }

    /// Pre-merge validation check - public API for checking merge compatibility
    pub async fn check_merge_validation<S: Store>(
        store: &S,
        source_database_id: &Id,
        source_branch_name: &str,
        target_database_id: &Id,
        target_branch_name: &str,
    ) -> Result<MergeValidationResult> {
        // Get both branches
        let source_branch = store
            .get_branch(source_database_id, source_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Source branch '{}' not found", source_branch_name))?;
        let target_branch = store
            .get_branch(target_database_id, target_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Target branch '{}' not found", target_branch_name))?;

        // Validate merge preconditions
        if !source_branch.can_be_merged() {
            let result = MergeValidationResult {
                can_merge: false,
                conflicts: vec![MergeConflict {
                    conflict_type: ConflictType::ValidationConflict,
                    resource_id: source_branch.name.clone(),
                    description: format!(
                        "Source branch '{}' cannot be merged (status: {:?})",
                        source_branch.name, source_branch.status
                    ),
                }],
                validation_result: None,
                simulated_schema_valid: false,
                affected_instances: Vec::new(),
            };
            return Ok(result);
        }

        if source_branch.database_id != target_branch.database_id {
            let result = MergeValidationResult {
                can_merge: false,
                conflicts: vec![MergeConflict {
                    conflict_type: ConflictType::ValidationConflict,
                    resource_id: "database_mismatch".to_string(),
                    description: "Cannot merge branches from different databases".to_string(),
                }],
                validation_result: None,
                simulated_schema_valid: false,
                affected_instances: Vec::new(),
            };
            return Ok(result);
        }

        // First check all types of conflicts (schema, instance, validation)
        let all_conflicts = Self::detect_conflicts(store, &source_branch, &target_branch).await?;

        // Then run validation compatibility check
        let mut validation_result =
            Self::validate_merge_compatibility(store, &source_branch, &target_branch).await?;

        // Merge all conflicts together
        validation_result.conflicts.extend(all_conflicts);
        validation_result.can_merge =
            validation_result.can_merge && validation_result.conflicts.is_empty();

        Ok(validation_result)
    }

    /// Rebase a feature branch onto a target branch (usually main)
    /// This replays the feature branch changes on top of the latest target branch state
    pub async fn rebase_branch<S: Store>(
        store: &S,
        feature_database_id: &Id,
        feature_branch_name: &str,
        target_database_id: &Id,
        target_branch_name: &str,
        author: Option<String>,
        force: bool,
    ) -> Result<RebaseResult> {
        // Get both branches
        let feature_branch = store
            .get_branch(feature_database_id, feature_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Feature branch '{}' not found", feature_branch_name))?;
        let target_branch = store
            .get_branch(target_database_id, target_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Target branch '{}' not found", target_branch_name))?;

        // Validate rebase preconditions
        if !feature_branch.can_be_merged() {
            return Err(anyhow!(
                "Feature branch '{}' cannot be rebased (status: {:?})",
                feature_branch.name,
                feature_branch.status
            ));
        }

        if feature_branch.database_id != target_branch.database_id {
            return Err(anyhow!("Cannot rebase branches from different databases"));
        }

        // Check if feature branch is already based on target branch
        if feature_branch.parent_branch_name.as_ref() == Some(&target_branch.name) {
            // Check if target branch has new commits since feature branch was created
            if !Self::has_new_commits(store, &feature_branch, &target_branch).await? {
                return Ok(RebaseResult {
                    success: true,
                    conflicts: Vec::new(),
                    message: "Branch is already up to date".to_string(),
                    rebased_instances: 0,
                    rebased_schema_changes: false,
                });
            }
        }

        // Detect conflicts that would occur during rebase
        let conflicts = if !force {
            Self::detect_rebase_conflicts(store, &feature_branch, &target_branch).await?
        } else {
            Vec::new()
        };

        if !conflicts.is_empty() && !force {
            let conflict_count = conflicts.len();
            return Ok(RebaseResult {
                success: false,
                conflicts,
                message: format!("Rebase failed due to {} conflicts", conflict_count),
                rebased_instances: 0,
                rebased_schema_changes: false,
            });
        }

        // Perform the rebase
        let rebase_result =
            Self::perform_rebase(store, &feature_branch, &target_branch, author).await?;

        Ok(rebase_result)
    }

    /// Check if target branch has new commits since feature branch was created
    async fn has_new_commits<S: Store>(
        _store: &S,
        feature_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<bool> {
        // Simple check: if target branch commit hash differs from feature branch parent
        // In a real implementation, this would check the commit history
        Ok(feature_branch.current_commit_hash != target_branch.current_commit_hash)
    }

    /// Detect conflicts that would occur during rebase
    async fn detect_rebase_conflicts<S: Store>(
        store: &S,
        feature_branch: &Branch,
        target_branch: &Branch,
    ) -> Result<Vec<MergeConflict>> {
        // For rebase, we need to check what conflicts would occur when applying
        // feature branch changes on top of target branch
        let mut conflicts = Vec::new();

        // Check schema conflicts (if both branches have modified schema)
        if let Some(schema_conflicts) =
            Self::detect_schema_conflicts(store, feature_branch, target_branch).await?
        {
            conflicts.extend(schema_conflicts);
        }

        // Check instance conflicts (if same instances modified in both branches)
        if let Some(instance_conflicts) =
            Self::detect_instance_conflicts(store, feature_branch, target_branch).await?
        {
            conflicts.extend(instance_conflicts);
        }

        // Check validation conflicts (if rebased result would have validation errors)
        if let Some(validation_conflicts) =
            Self::detect_validation_conflicts(store, feature_branch, target_branch).await?
        {
            conflicts.extend(validation_conflicts);
        }

        Ok(conflicts)
    }

    /// Perform the actual rebase operation
    async fn perform_rebase<S: Store>(
        store: &S,
        feature_branch: &Branch,
        target_branch: &Branch,
        author: Option<String>,
    ) -> Result<RebaseResult> {
        // TODO: Update for new commit-based architecture - rebase operations currently disabled
        return Err(anyhow::anyhow!("Rebase operations disabled pending commit-based architecture update"));
        let mut rebased_instances = 0;
        let mut rebased_schema_changes = false;

        // Step 1: Update feature branch to point to target branch as parent
        let mut updated_feature_branch = feature_branch.clone();
        updated_feature_branch.parent_branch_name = Some(target_branch.name.clone());
        updated_feature_branch.current_commit_hash = crate::model::generate_id(); // New commit hash
        updated_feature_branch.commit_message = Some(format!(
            "Rebased '{}' onto '{}'",
            feature_branch.name, target_branch.name
        ));
        if let Some(author) = &author {
            updated_feature_branch.author = Some(author.clone());
        }

        // Step 2: Copy target branch schema as base, then apply feature branch changes
        if let Some(target_schema) = store.get_schema(&target_branch.database_id, &target_branch.name).await? {
            if let Some(feature_schema) = store.get_schema(&feature_branch.database_id, &feature_branch.name).await? {
                // Create rebased schema by merging target base with feature changes
                let rebased_schema =
                    Self::merge_schemas(target_schema, feature_schema, &updated_feature_branch.name);
                // store.upsert_schema(rebased_schema).await?; // upsert_schema method removed
                rebased_schema_changes = true;
            } else {
                // No feature schema, just copy target schema to feature branch
                let mut rebased_schema = target_schema;
                // rebased_schema.branch_id = updated_feature_branch.name.clone(); // branch_id field removed
                // store.upsert_schema(rebased_schema).await?; // upsert_schema method removed
                rebased_schema_changes = true;
            }
        }

        // Step 3: Copy target branch instances as base, then apply feature branch changes
        let target_instances = store
            .list_instances_for_branch(&target_branch.database_id, &target_branch.name, None)
            .await?;
        let feature_instances = store
            .list_instances_for_branch(&feature_branch.database_id, &feature_branch.name, None)
            .await?;

        // Create a map of target instances
        let target_instance_map: std::collections::HashMap<String, Instance> = target_instances
            .into_iter()
            .map(|i| (i.id.clone(), i))
            .collect();

        // Delete all existing instances in feature branch
        let existing_feature_instances = store
            .list_instances_for_branch(&feature_branch.database_id, &feature_branch.name, None)
            .await?;
        for instance in existing_feature_instances {
            // store.delete_instance(&instance.id).await?; // delete_instance method removed
        }

        // Add target instances to feature branch (as base)
        for (_, mut instance) in target_instance_map {
            // instance.branch_id = updated_feature_branch.name.clone(); // branch_id field removed
            // store.upsert_instance(instance).await?; // upsert_instance method removed
            rebased_instances += 1;
        }

        // Apply feature branch changes (feature instances override target instances)
        for mut feature_instance in feature_instances {
            // feature_instance.branch_id = updated_feature_branch.name.clone(); // branch_id field removed
            // store.upsert_instance(feature_instance).await?; // upsert_instance method removed
        }

        // Step 4: Update the branch record
        store.upsert_branch(updated_feature_branch).await?;

        Ok(RebaseResult {
            success: true,
            conflicts: Vec::new(),
            message: format!(
                "Successfully rebased '{}' onto '{}'",
                feature_branch.name, target_branch.name
            ),
            rebased_instances,
            rebased_schema_changes,
        })
    }

    /// Merge two schemas, with feature schema changes taking precedence
    fn merge_schemas(
        mut target_schema: crate::model::Schema,
        feature_schema: crate::model::Schema,
        new_branch_name: &str,
    ) -> crate::model::Schema {
        // target_schema.branch_id = new_branch_name.clone(); // branch_id field removed
        target_schema.description = feature_schema.description.or(target_schema.description);

        // Merge classes: feature classes override target classes with same name
        let mut merged_classes = Vec::new();
        let mut feature_class_names: std::collections::HashSet<String> = feature_schema
            .classes
            .iter()
            .map(|c| c.name.clone())
            .collect();

        // Add target classes that aren't overridden by feature
        for target_class in target_schema.classes {
            if !feature_class_names.contains(&target_class.name) {
                merged_classes.push(target_class);
            }
        }

        // Add all feature classes (these override target classes)
        for feature_class in feature_schema.classes {
            feature_class_names.remove(&feature_class.name);
            merged_classes.push(feature_class);
        }

        target_schema.classes = merged_classes;
        target_schema
    }

    /// Validate rebase compatibility - public API
    pub async fn check_rebase_validation<S: Store>(
        store: &S,
        feature_database_id: &Id,
        feature_branch_name: &str,
        target_database_id: &Id,
        target_branch_name: &str,
    ) -> Result<RebaseValidationResult> {
        // Get both branches
        let feature_branch = store
            .get_branch(feature_database_id, feature_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Feature branch '{}' not found", feature_branch_name))?;
        let target_branch = store
            .get_branch(target_database_id, target_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Target branch '{}' not found", target_branch_name))?;

        // Validate rebase preconditions
        if !feature_branch.can_be_merged() {
            let result = RebaseValidationResult {
                can_rebase: false,
                conflicts: vec![MergeConflict {
                    conflict_type: ConflictType::ValidationConflict,
                    resource_id: feature_branch.name.clone(),
                    description: format!(
                        "Feature branch '{}' cannot be rebased (status: {:?})",
                        feature_branch.name, feature_branch.status
                    ),
                }],
                validation_result: None,
                needs_rebase: false,
                affected_instances: Vec::new(),
            };
            return Ok(result);
        }

        if feature_branch.database_id != target_branch.database_id {
            let result = RebaseValidationResult {
                can_rebase: false,
                conflicts: vec![MergeConflict {
                    conflict_type: ConflictType::ValidationConflict,
                    resource_id: "database_mismatch".to_string(),
                    description: "Cannot rebase branches from different databases".to_string(),
                }],
                validation_result: None,
                needs_rebase: false,
                affected_instances: Vec::new(),
            };
            return Ok(result);
        }

        // Check if rebase is needed
        let needs_rebase = Self::has_new_commits(store, &feature_branch, &target_branch).await?;

        // Detect conflicts
        let conflicts =
            Self::detect_rebase_conflicts(store, &feature_branch, &target_branch).await?;

        // Simulate rebase validation (similar to merge validation)
        let validation_result =
            Self::validate_merge_compatibility(store, &feature_branch, &target_branch).await?;

        Ok(RebaseValidationResult {
            can_rebase: conflicts.is_empty()
                && validation_result
                    .validation_result
                    .as_ref()
                    .map(|v| v.valid)
                    .unwrap_or(true),
            conflicts,
            validation_result: validation_result.validation_result,
            needs_rebase,
            affected_instances: validation_result.affected_instances,
        })
    }
}
