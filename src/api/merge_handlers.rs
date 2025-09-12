use crate::api::handlers::{AppState, ErrorResponse};
use crate::logic::branch_ops_v2::{BranchOperationsV2, ResolveConflictsRequest, MergeOperationResult};
use crate::store::traits::{BranchStore, CommitStore, Store, WorkingCommitStore};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as RequestJson,
};
use serde::{Deserialize, Serialize};

// Request/response structures
#[derive(Debug, Deserialize)]
pub struct StartMergeRequest {
    pub source_branch: String,
    pub author: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StartMergeResponse {
    pub success: bool,
    pub working_commit_id: Option<String>,
    pub conflicts: Vec<MergeConflictInfo>,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct MergeConflictInfo {
    pub index: usize,
    pub conflict_type: String,
    pub resource_type: String,
    pub resource_id: String,
    pub field_path: Option<Vec<String>>,
    pub description: String,
    pub base_value: Option<serde_json::Value>,
    pub left_value: Option<serde_json::Value>,
    pub right_value: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct MergeStatusResponse {
    pub status: String,
    pub working_commit_id: String,
    pub conflicts: Vec<MergeConflictInfo>,
    pub resolved_conflicts: usize,
    pub total_conflicts: usize,
}

/// POST /databases/{db_id}/branches/{branch_name}/merge
/// Start a merge operation by creating a merge working commit
pub async fn start_merge<S: Store + CommitStore + WorkingCommitStore + BranchStore>(
    Path((db_id, target_branch)): Path<(String, String)>,
    State(store): State<AppState<S>>,
    RequestJson(req): RequestJson<StartMergeRequest>,
) -> Result<Json<StartMergeResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ));
        }
    }

    // Start the merge
    match BranchOperationsV2::start_merge(
        &*store,
        &db_id,
        &req.source_branch,
        &db_id,
        &target_branch,
        req.author,
    )
    .await
    {
        Ok(result) => {
            let conflicts: Vec<MergeConflictInfo> = result
                .conflicts
                .into_iter()
                .enumerate()
                .map(|(idx, c)| MergeConflictInfo {
                    index: idx,
                    conflict_type: format!("{:?}", c.conflict_type),
                    resource_type: format!("{:?}", c.resource_type),
                    resource_id: c.resource_id,
                    field_path: c.field_path,
                    description: c.description,
                    base_value: c.base_value,
                    left_value: c.left_value,
                    right_value: c.right_value,
                })
                .collect();

            Ok(Json(StartMergeResponse {
                success: result.success,
                working_commit_id: result.working_commit_id,
                conflicts,
                message: result.message,
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// GET /databases/{db_id}/branches/{branch_name}/merge/validate
/// Validate if a merge can be performed
pub async fn validate_merge<S: Store + CommitStore>(
    Path((db_id, target_branch)): Path<(String, String)>,
    State(store): State<AppState<S>>,
    RequestJson(req): RequestJson<StartMergeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Validate database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ));
        }
    }

    // Validate the merge
    match BranchOperationsV2::validate_merge(
        &*store,
        &db_id,
        &req.source_branch,
        &db_id,
        &target_branch,
    )
    .await
    {
        Ok(result) => {
            let conflicts: Vec<MergeConflictInfo> = result
                .conflicts
                .into_iter()
                .enumerate()
                .map(|(idx, c)| MergeConflictInfo {
                    index: idx,
                    conflict_type: format!("{:?}", c.conflict_type),
                    resource_type: format!("{:?}", c.resource_type),
                    resource_id: c.resource_id,
                    field_path: c.field_path,
                    description: c.description,
                    base_value: c.base_value,
                    left_value: c.left_value,
                    right_value: c.right_value,
                })
                .collect();

            Ok(Json(serde_json::json!({
                "can_merge": result.can_merge,
                "common_ancestor": result.common_ancestor,
                "conflicts": conflicts,
                "validation_result": result.validation_result,
            })))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// GET /databases/{db_id}/branches/{branch_name}/merge/status
/// Get the status of an ongoing merge
pub async fn get_merge_status<S: WorkingCommitStore>(
    Path((db_id, branch_name)): Path<(String, String)>,
    State(store): State<AppState<S>>,
) -> Result<Json<MergeStatusResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Get all working commits for the branch and find one with merge state
    match store
        .list_working_commits_for_branch(&db_id, &branch_name)
        .await
    {
        Ok(working_commits) => {
            eprintln!("DEBUG: Found {} working commits for branch {}/{}", working_commits.len(), db_id, branch_name);
            for wc in &working_commits {
                eprintln!("DEBUG: Working commit {} - status: {:?}, has_merge_state: {}", wc.id, wc.status, wc.merge_state.is_some());
            }
            
            // Find the most recent working commit with merge state (status should be Merging)
            // Sort by updated_at to get the most recent first
            let mut merge_wcs: Vec<_> = working_commits
                .into_iter()
                .filter(|wc| wc.status == crate::model::WorkingCommitStatus::Merging && wc.merge_state.is_some())
                .collect();
            
            merge_wcs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
            let merge_wc = merge_wcs.into_iter().next();
            
            match merge_wc {
                Some(wc) => {
                    if let Some(merge_state) = &wc.merge_state {
                let conflicts: Vec<MergeConflictInfo> = merge_state
                    .conflicts
                    .iter()
                    .enumerate()
                    .map(|(idx, c)| MergeConflictInfo {
                        index: idx,
                        conflict_type: format!("{:?}", c.conflict_type),
                        resource_type: format!("{:?}", c.resource_type),
                        resource_id: c.resource_id.clone(),
                        field_path: c.field_path.clone(),
                        description: c.description.clone(),
                        base_value: c.base_value.clone(),
                        left_value: c.left_value.clone(),
                        right_value: c.right_value.clone(),
                    })
                    .collect();

                        Ok(Json(MergeStatusResponse {
                            status: format!("{:?}", wc.status),
                            working_commit_id: wc.id,
                            conflicts,
                            resolved_conflicts: merge_state.resolutions.len(),
                            total_conflicts: merge_state.conflicts.len(),
                        }))
                    } else {
                        Err((
                            StatusCode::NOT_FOUND,
                            Json(ErrorResponse::new("Working commit found but has no merge state")),
                        ))
                    }
                }
                None => Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("No merge in progress on this branch")),
                )),
            }
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// POST /databases/{db_id}/branches/{branch_name}/merge/resolve
/// Resolve conflicts in an ongoing merge
pub async fn resolve_merge_conflicts<S: WorkingCommitStore + CommitStore + Store>(
    Path((db_id, branch_name)): Path<(String, String)>,
    State(store): State<AppState<S>>,
    RequestJson(req): RequestJson<ResolveConflictsRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Get the merge working commit for the branch
    let working_commits = match store
        .list_working_commits_for_branch(&db_id, &branch_name)
        .await
    {
        Ok(wcs) => wcs,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ));
        }
    };
    
    // Find the most recent working commit with merge state
    let mut merge_wcs: Vec<_> = working_commits
        .into_iter()
        .filter(|wc| wc.status == crate::model::WorkingCommitStatus::Merging && wc.merge_state.is_some())
        .collect();
    
    merge_wcs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let wc = merge_wcs.into_iter().next()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("No merge in progress on this branch")),
            )
        })?;

    // Resolve conflicts
    match BranchOperationsV2::resolve_conflicts(&*store, &wc.id, req.resolutions).await {
        Ok(()) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Conflicts resolved successfully"
        }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// DELETE /databases/{db_id}/branches/{branch_name}/merge
/// Abort an ongoing merge
pub async fn abort_merge<S: WorkingCommitStore>(
    Path((db_id, branch_name)): Path<(String, String)>,
    State(store): State<AppState<S>>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Get the merge working commit for the branch
    let working_commits = match store
        .list_working_commits_for_branch(&db_id, &branch_name)
        .await
    {
        Ok(wcs) => wcs,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ));
        }
    };
    
    // Find the most recent working commit with merge state
    let mut merge_wcs: Vec<_> = working_commits
        .into_iter()
        .filter(|wc| wc.status == crate::model::WorkingCommitStatus::Merging && wc.merge_state.is_some())
        .collect();
    
    merge_wcs.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    let wc = merge_wcs.into_iter().next()
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("No merge in progress on this branch")),
            )
        })?;

    // Abort merge
    match BranchOperationsV2::abort_merge(&*store, &wc.id).await {
        Ok(()) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Merge aborted successfully"
        }))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}