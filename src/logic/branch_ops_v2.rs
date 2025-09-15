use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::logic::merge::MergeEngine;
use crate::logic::validate_simple::ValidationResult;
use crate::model::merge::{ConflictResolution, MergeState};
use crate::model::{
    Id, NewWorkingCommit, WorkingCommit, WorkingCommitStatus,
};
use crate::store::traits::{BranchStore, CommitStore, Store, WorkingCommitStore};

/// Result of a merge operation (renamed to avoid conflict with model::merge::MergeResult)
#[derive(Debug, Serialize, Deserialize)]
pub struct MergeOperationResult {
    pub success: bool,
    pub working_commit_id: Option<Id>,
    pub conflicts: Vec<crate::model::merge::MergeConflict>,
    pub message: String,
}

/// Result of a merge validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeValidationResult {
    pub can_merge: bool,
    pub common_ancestor: Option<String>,
    pub conflicts: Vec<crate::model::merge::MergeConflict>,
    pub validation_result: Option<ValidationResult>,
}

/// Request to resolve conflicts
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveConflictsRequest {
    pub resolutions: HashMap<usize, ConflictResolution>,
}

/// Branch operations for commit-based architecture
pub struct BranchOperationsV2;

impl BranchOperationsV2 {
    /// Start a merge operation by creating a merge working commit
    pub async fn start_merge<S: Store + CommitStore + WorkingCommitStore + BranchStore>(
        store: &S,
        source_database_id: &Id,
        source_branch_name: &str,
        target_database_id: &Id,
        target_branch_name: &str,
        author: Option<String>,
    ) -> Result<MergeOperationResult> {
        // First check if there's already a merge in progress
        let existing_wcs = store
            .list_working_commits_for_branch(target_database_id, target_branch_name)
            .await?;

        for wc in existing_wcs {
            if wc.status == WorkingCommitStatus::Merging {
                return Err(anyhow!(
                    "A merge is already in progress on branch '{}'. Please complete or abort it first.",
                    target_branch_name
                ));
            }
        }
        // Validate databases match
        if source_database_id != target_database_id {
            return Err(anyhow!("Cannot merge branches from different databases"));
        }

        // Get both branches
        let source_branch = store
            .get_branch(source_database_id, source_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Source branch '{}' not found", source_branch_name))?;

        let target_branch = store
            .get_branch(target_database_id, target_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Target branch '{}' not found", target_branch_name))?;

        // Get current commits
        if source_branch.current_commit_hash.is_empty() {
            return Err(anyhow!("Source branch has no commits"));
        }
        let source_commit = source_branch.current_commit_hash;

        if target_branch.current_commit_hash.is_empty() {
            return Err(anyhow!("Target branch has no commits"));
        }
        let target_commit = target_branch.current_commit_hash;

        // Find common ancestor
        let common_ancestor =
            MergeEngine::find_common_ancestor(store, &target_commit, &source_commit)
                .await?
                .ok_or_else(|| anyhow!("No common ancestor found between branches"))?;

        // Perform three-way merge
        let merge_result = MergeEngine::three_way_merge(
            store,
            &common_ancestor,
            &target_commit, // left (current)
            &source_commit, // right (incoming)
        )
        .await?;

        // Create a merge working commit
        // Note: We always create a new working commit for merge operations,
        // regardless of any existing working commits on the target branch
        let target_commit_obj = store
            .get_commit(&target_commit)
            .await?
            .ok_or_else(|| anyhow!("Target commit not found"))?;

        let mut working_commit = WorkingCommit::new(
            target_database_id.clone(),
            Some(target_branch_name.to_string()),
            &target_commit_obj,
            author.clone(),
        )
        .map_err(|e| anyhow!("Failed to create working commit: {}", e))?;

        // If there are conflicts, set up merge state
        if !merge_result.conflicts.is_empty() {
            working_commit.status = WorkingCommitStatus::Merging;
            working_commit.merge_state = Some(MergeState {
                base_commit: common_ancestor.clone(),
                left_commit: target_commit.clone(),
                right_commit: source_commit.clone(),
                conflicts: merge_result.conflicts.clone(),
                resolutions: HashMap::new(),
                is_rebase: false,
            });

            // Apply non-conflicting changes
            let base_data = store
                .get_commit(&common_ancestor)
                .await?
                .ok_or_else(|| anyhow!("Common ancestor commit not found"))?
                .get_data()
                .map_err(|e| anyhow!("Failed to get commit data: {}", e))?;

            let merged_data = MergeEngine::apply_merge_result(&base_data, &merge_result)?;
            working_commit.schema_data = merged_data.schema;
            working_commit.instances_data = merged_data.instances;
        } else {
            // No conflicts - apply all changes
            let base_data = store
                .get_commit(&common_ancestor)
                .await?
                .ok_or_else(|| anyhow!("Common ancestor commit not found"))?
                .get_data()
                .map_err(|e| anyhow!("Failed to get commit data: {}", e))?;

            let merged_data = MergeEngine::apply_merge_result(&base_data, &merge_result)?;
            working_commit.schema_data = merged_data.schema;
            working_commit.instances_data = merged_data.instances;
        }

        // First create a basic working commit
        let created_wc = store
            .create_working_commit(
                target_database_id,
                target_branch_name,
                NewWorkingCommit {
                    author: working_commit.author.clone(),
                },
            )
            .await?;

        // Now update the created working commit with our merge data
        // Copy the ID from the created commit to ensure we're updating the right one
        working_commit.id = created_wc.id.clone();
        working_commit.database_id = created_wc.database_id.clone();
        working_commit.branch_name = created_wc.branch_name.clone();
        working_commit.created_at = created_wc.created_at.clone();
        working_commit.updated_at = created_wc.updated_at.clone();

        // Update with all our merge-specific data
        eprintln!(
            "DEBUG: Updating working commit {} with merge state",
            working_commit.id
        );
        eprintln!(
            "DEBUG: Status: {:?}, Has merge_state: {}",
            working_commit.status,
            working_commit.merge_state.is_some()
        );

        store
            .update_working_commit(working_commit.clone())
            .await
            .map_err(|e| anyhow!("Failed to update working commit with merge data: {}", e))?;

        // Verify the working commit was properly saved with merge state
        match store.get_working_commit(&created_wc.id).await? {
            Some(wc) => {
                eprintln!("DEBUG: Verified working commit {} exists with status: {:?}, has merge_state: {}", 
                    wc.id, wc.status, wc.merge_state.is_some());
            }
            None => {
                return Err(anyhow!(
                    "Working commit {} was not found after creation",
                    created_wc.id
                ));
            }
        }

        let conflicts_count = merge_result.conflicts.len();
        Ok(MergeOperationResult {
            success: merge_result.conflicts.is_empty(),
            working_commit_id: Some(created_wc.id),
            conflicts: merge_result.conflicts,
            message: if conflicts_count == 0 {
                "Merge completed successfully. Review changes and commit when ready.".to_string()
            } else {
                format!(
                    "Merge has {} conflicts. Resolve them before committing.",
                    conflicts_count
                )
            },
        })
    }

    /// Validate if a merge can be performed
    pub async fn validate_merge<S: Store + CommitStore>(
        store: &S,
        source_database_id: &Id,
        source_branch_name: &str,
        target_database_id: &Id,
        target_branch_name: &str,
    ) -> Result<MergeValidationResult> {
        // Validate databases match
        if source_database_id != target_database_id {
            return Ok(MergeValidationResult {
                can_merge: false,
                common_ancestor: None,
                conflicts: vec![],
                validation_result: None,
            });
        }

        // Get both branches
        let source_branch = store
            .get_branch(source_database_id, source_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Source branch '{}' not found", source_branch_name))?;

        let target_branch = store
            .get_branch(target_database_id, target_branch_name)
            .await?
            .ok_or_else(|| anyhow!("Target branch '{}' not found", target_branch_name))?;

        // Get current commits
        if source_branch.current_commit_hash.is_empty() {
            return Ok(MergeValidationResult {
                can_merge: false,
                common_ancestor: None,
                conflicts: vec![],
                validation_result: None,
            });
        }
        let source_commit = source_branch.current_commit_hash;

        if target_branch.current_commit_hash.is_empty() {
            return Ok(MergeValidationResult {
                can_merge: false,
                common_ancestor: None,
                conflicts: vec![],
                validation_result: None,
            });
        }
        let target_commit = target_branch.current_commit_hash;

        // Find common ancestor
        let common_ancestor =
            MergeEngine::find_common_ancestor(store, &target_commit, &source_commit).await?;

        if common_ancestor.is_none() {
            return Ok(MergeValidationResult {
                can_merge: false,
                common_ancestor: None,
                conflicts: vec![],
                validation_result: None,
            });
        }

        let ancestor = common_ancestor.unwrap();

        // Perform three-way merge to detect conflicts
        let merge_result =
            MergeEngine::three_way_merge(store, &ancestor, &target_commit, &source_commit).await?;

        // If no conflicts, validate the merged result
        let validation_result = if merge_result.conflicts.is_empty() {
            // TODO: Implement validation of merged data
            // For now, skip validation
            None
        } else {
            None
        };

        Ok(MergeValidationResult {
            can_merge: merge_result.conflicts.is_empty(),
            common_ancestor: Some(ancestor),
            conflicts: merge_result.conflicts,
            validation_result,
        })
    }

    /// Resolve conflicts in a merge working commit
    pub async fn resolve_conflicts<S: WorkingCommitStore + CommitStore + Store>(
        store: &S,
        working_commit_id: &Id,
        resolutions: HashMap<usize, ConflictResolution>,
    ) -> Result<()> {
        // Get the working commit
        let mut working_commit = store
            .get_working_commit(working_commit_id)
            .await?
            .ok_or_else(|| anyhow!("Working commit not found"))?;

        // Verify it's in merging state
        if working_commit.status != WorkingCommitStatus::Merging {
            return Err(anyhow!("Working commit is not in merging state"));
        }

        let merge_state = working_commit
            .merge_state
            .as_mut()
            .ok_or_else(|| anyhow!("Working commit has no merge state"))?;

        // Apply resolutions
        merge_state.resolutions.extend(resolutions);

        // Check if all conflicts are resolved
        if merge_state.resolutions.len() < merge_state.conflicts.len() {
            // Still have unresolved conflicts
            store.update_working_commit(working_commit).await?;
            return Ok(());
        }

        // All conflicts resolved - rebuild the merged data
        let base_commit = store
            .get_commit(&merge_state.base_commit)
            .await?
            .ok_or_else(|| anyhow!("Base commit not found"))?;
        let left_commit = store
            .get_commit(&merge_state.left_commit)
            .await?
            .ok_or_else(|| anyhow!("Left commit not found"))?;
        let right_commit = store
            .get_commit(&merge_state.right_commit)
            .await?
            .ok_or_else(|| anyhow!("Right commit not found"))?;

        let base_data = base_commit
            .get_data()
            .map_err(|e| anyhow!("Failed to get base commit data: {}", e))?;
        let left_data = left_commit
            .get_data()
            .map_err(|e| anyhow!("Failed to get left commit data: {}", e))?;
        let right_data = right_commit
            .get_data()
            .map_err(|e| anyhow!("Failed to get right commit data: {}", e))?;

        // Re-compute diffs
        let left_diff = MergeEngine::compute_diff(&base_data, &left_data)?;
        let right_diff = MergeEngine::compute_diff(&base_data, &right_data)?;

        // Apply resolutions and merge
        let resolved_merge = Self::apply_resolutions_to_merge(
            left_diff,
            right_diff,
            &merge_state.conflicts,
            &merge_state.resolutions,
        )?;

        let merged_data = MergeEngine::apply_merge_result(&base_data, &resolved_merge)?;

        // Update working commit with resolved data
        working_commit.schema_data = merged_data.schema;
        working_commit.instances_data = merged_data.instances;
        working_commit.status = WorkingCommitStatus::Active; // Back to normal state
        working_commit.merge_state = None; // Clear merge state

        store.update_working_commit(working_commit).await?;

        Ok(())
    }

    /// Apply conflict resolutions to produce a final merge result
    fn apply_resolutions_to_merge(
        left_diff: crate::model::merge::CommitDiff,
        right_diff: crate::model::merge::CommitDiff,
        conflicts: &[crate::model::merge::MergeConflict],
        resolutions: &HashMap<usize, ConflictResolution>,
    ) -> Result<crate::model::merge::MergeResult> {
        // This is a simplified implementation
        // In a real system, you'd need to carefully apply each resolution

        // For now, just merge non-conflicting operations
        let base_merge = MergeEngine::merge_diffs(left_diff, right_diff);

        // Apply resolutions to override conflicts
        // TODO: Implement proper resolution application

        Ok(crate::model::merge::MergeResult {
            success: true,
            conflicts: vec![], // All resolved
            merged_operations: base_merge.merged_operations,
            needs_validation: true,
        })
    }

    /// Abort a merge operation
    pub async fn abort_merge<S: WorkingCommitStore>(
        store: &S,
        working_commit_id: &Id,
    ) -> Result<()> {
        let working_commit = store
            .get_working_commit(working_commit_id)
            .await?
            .ok_or_else(|| anyhow!("Working commit not found"))?;

        if working_commit.status != WorkingCommitStatus::Merging {
            return Err(anyhow!("Working commit is not in merging state"));
        }

        // Delete the working commit
        store.delete_working_commit(working_commit_id).await?;

        Ok(())
    }
}
