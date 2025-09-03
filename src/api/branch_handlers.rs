use crate::api::handlers::{AppState, ErrorResponse};
use crate::logic::BranchOperations;
use crate::store::traits::Store;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    Json as RequestJson,
};
use serde::{Deserialize, Serialize};

// Branch operation request/response structures
#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub target_branch_id: String,
    pub author: Option<String>,
    pub force: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct MergeResponse {
    pub success: bool,
    pub conflicts: Vec<ConflictInfo>,
    pub merged_instances: usize,
    pub merged_schema_changes: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ConflictInfo {
    pub conflict_type: String,
    pub resource_id: String,
    pub description: String,
}

#[derive(Debug, Deserialize)]
pub struct DeleteBranchRequest {
    pub force: Option<bool>,
}

/// POST /databases/{db_id}/versions/{branch_id}/merge
/// Merge this branch into target branch
pub async fn merge_branch<S: Store>(
    Path((db_id, branch_id)): Path<(String, String)>,
    State(store): State<AppState<S>>,
    RequestJson(req): RequestJson<MergeRequest>,
) -> Result<Json<MergeResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Validate that the database exists
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

    // Perform the merge
    match BranchOperations::merge_branch(
        &*store,
        &db_id,
        &branch_id,
        &db_id,  // Target database is the same as source for now
        &req.target_branch_id,
        req.author,
        req.force.unwrap_or(false),
    )
    .await
    {
        Ok(result) => {
            let conflicts: Vec<ConflictInfo> = result
                .conflicts
                .into_iter()
                .map(|c| ConflictInfo {
                    conflict_type: format!("{:?}", c.conflict_type),
                    resource_id: c.resource_id,
                    description: c.description,
                })
                .collect();

            let message = if result.success {
                format!(
                    "Successfully merged branch '{}' into '{}'",
                    branch_id, req.target_branch_id
                )
            } else {
                format!("Merge failed due to {} conflicts", conflicts.len())
            };

            Ok(Json(MergeResponse {
                success: result.success,
                conflicts,
                merged_instances: result.merged_instances,
                merged_schema_changes: result.merged_schema_changes,
                message,
            }))
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// DELETE /databases/{db_id}/versions/{branch_id}
/// Delete a branch (only if merged or archived)
pub async fn delete_branch<S: Store>(
    Path((db_id, branch_id)): Path<(String, String)>,
    State(store): State<AppState<S>>,
    RequestJson(req): RequestJson<DeleteBranchRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Validate that the database exists
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

    // Perform the deletion
    match BranchOperations::delete_branch(&*store, &db_id, &branch_id, req.force.unwrap_or(false)).await {
        Ok(deleted) => {
            if deleted {
                Ok(Json(serde_json::json!({
                    "success": true,
                    "message": format!("Branch '{}' deleted successfully", branch_id)
                })))
            } else {
                Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Branch not found")),
                ))
            }
        }
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

