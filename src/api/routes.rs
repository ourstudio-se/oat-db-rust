use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use std::sync::Arc;

use crate::api::{branch_handlers, handlers};
use crate::store::traits::Store;

pub fn create_router<S: Store + 'static>() -> Router<Arc<S>> {
    Router::new()
        // Health check
        .route("/health", get(handlers::health_check))
        // API Documentation
        .route("/docs", get(handlers::get_api_docs::<S>))
        .route("/docs/openapi.json", get(handlers::get_openapi_spec::<S>))
        // Type Validation endpoints
        .route(
            "/databases/:db_id/validate",
            get(handlers::validate_database_instances::<S>),
        )
        .route(
            "/databases/:db_id/instances/:instance_id/validate",
            get(handlers::validate_single_instance::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/validate",
            get(handlers::validate_branch_instances::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/validate",
            get(handlers::validate_branch_single_instance::<S>),
        )
        // Merge Validation endpoints
        .route(
            "/databases/:db_id/branches/:source_branch_id/validate-merge",
            get(handlers::validate_database_merge::<S>),
        )
        .route(
            "/databases/:db_id/branches/:source_branch_id/validate-merge/:target_branch_id",
            get(handlers::validate_branch_merge::<S>),
        )
        // Rebase endpoints
        .route(
            "/databases/:db_id/branches/:feature_branch_id/rebase",
            post(handlers::rebase_database_branch::<S>),
        )
        .route(
            "/databases/:db_id/branches/:feature_branch_id/rebase/:target_branch_id",
            post(handlers::rebase_branch::<S>),
        )
        // Rebase Validation endpoints
        .route(
            "/databases/:db_id/branches/:feature_branch_id/validate-rebase",
            get(handlers::validate_database_rebase::<S>),
        )
        .route(
            "/databases/:db_id/branches/:feature_branch_id/validate-rebase/:target_branch_id",
            get(handlers::validate_branch_rebase::<S>),
        )
        // Database management
        .route("/databases", get(handlers::list_databases::<S>))
        .route("/databases", post(handlers::upsert_database::<S>))
        .route("/databases/:db_id", get(handlers::get_database::<S>))
        .route("/databases/:db_id", delete(handlers::delete_database::<S>))
        .route("/databases/:db_id/commits", get(handlers::list_database_commits::<S>))
        // NEW: Commit-specific data access endpoints
        .route("/databases/:db_id/commits/:commit_hash/schema", get(handlers::get_commit_schema::<S>))
        .route("/databases/:db_id/commits/:commit_hash/instances", get(handlers::get_commit_instances::<S>))
        .route("/databases/:db_id/commits/:commit_hash/schema/classes/:class_id", get(handlers::get_commit_class::<S>))
        .route("/databases/:db_id/commits/:commit_hash/instances/:instance_id", get(handlers::get_commit_instance::<S>))
        // Database-level queries (automatically use main branch) - DEPRECATED
        // These endpoints are DEPRECATED and will be removed in future versions.
        // Use commit-specific endpoints instead: /databases/{db}/commits/{commit}/...
        .route(
            "/databases/:db_id/schema",
            get(handlers::get_database_schema::<S>),
        )
        .route(
            "/databases/:db_id/schema",
            post(handlers::upsert_database_schema::<S>),
        )
        .route(
            "/databases/:db_id/schema/classes",
            post(handlers::add_database_class::<S>),
        )
        .route(
            "/databases/:db_id/schema/classes/:class_id",
            get(handlers::get_database_class::<S>),
        )
        .route(
            "/databases/:db_id/schema/classes/:class_id",
            patch(handlers::update_database_class::<S>),
        )
        .route(
            "/databases/:db_id/schema/classes/:class_id",
            delete(handlers::delete_database_class::<S>),
        )
        .route(
            "/databases/:db_id/instances",
            get(handlers::list_database_instances::<S>),
        )
        .route(
            "/databases/:db_id/instances",
            post(handlers::upsert_database_instance::<S>),
        )
        .route(
            "/databases/:db_id/instances/:id",
            get(handlers::get_database_instance::<S>),
        )
        .route(
            "/databases/:db_id/instances/:id",
            patch(handlers::update_database_instance::<S>),
        )
        .route(
            "/databases/:db_id/instances/:id",
            delete(handlers::delete_database_instance::<S>),
        )
        // Branch management
        .route(
            "/databases/:db_id/branches",
            get(handlers::list_branches::<S>),
        )
        .route(
            "/databases/:db_id/branches",
            post(handlers::upsert_branch::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id",
            get(handlers::get_branch::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id",
            patch(handlers::update_branch_status::<S>),
        )
        // Branch-level data access - READ-ONLY (use commit-specific endpoints for reads)
        // MODIFICATION ENDPOINTS DEPRECATED - use working-commit endpoints instead
        .route(
            "/databases/:db_id/branches/:branch_id/schema",
            get(handlers::get_schema::<S>),
        )
        // DEPRECATED: Schema modifications must go through working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/schema",
            post(handlers::upsert_schema::<S>),
        )
        // DEPRECATED: Class additions must go through working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/schema/classes",
            post(handlers::add_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/schema/classes/:class_id",
            get(handlers::get_class::<S>),
        )
        // DEPRECATED: Class updates must go through working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/schema/classes/:class_id",
            patch(handlers::update_class::<S>),
        )
        // DEPRECATED: Class deletions must go through working-commit endpoints  
        .route(
            "/databases/:db_id/branches/:branch_id/schema/classes/:class_id",
            delete(handlers::delete_class::<S>),
        )
        // Instance management (many per branch) - READ-ONLY
        .route(
            "/databases/:db_id/branches/:branch_id/instances",
            get(handlers::list_instances::<S>),
        )
        // DEPRECATED: Instance creation must go through working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/instances",
            post(handlers::upsert_instance::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:id",
            get(handlers::get_instance::<S>),
        )
        // DEPRECATED: Instance updates must go through working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:id",
            patch(handlers::update_instance::<S>),
        )
        // DEPRECATED: Instance deletions must go through working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:id",
            delete(handlers::delete_instance::<S>),
        )
        // Branch operations (git-like)
        .route(
            "/databases/:db_id/branches/:branch_id/merge",
            post(branch_handlers::merge_branch::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/delete",
            post(branch_handlers::delete_branch::<S>),
        )
        // Instance-specific query/solve endpoints
        .route(
            "/databases/:db_id/instances/:instance_id/query",
            post(handlers::query_instance_configuration::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/query",
            post(handlers::query_branch_instance_configuration::<S>),
        )
        // Batch query endpoints for multiple objectives
        .route(
            "/databases/:db_id/instances/:instance_id/batch-query",
            post(handlers::batch_query_instance_configuration::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/batch-query",
            post(handlers::batch_query_branch_instance_configuration::<S>),
        )
        // GET query endpoints with URL parameters for solver objectives
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/query",
            get(handlers::get_branch_instance_query::<S>),
        )
        .route(
            "/databases/:db_id/commits/:commit_hash/instances/:instance_id/query",
            get(handlers::get_commit_instance_query::<S>),
        )
        // Legacy solve endpoint (deprecated - use instance-specific endpoints)
        .route("/solve", post(handlers::solve_configuration::<S>))
        .route("/artifacts", get(handlers::list_artifacts::<S>))
        .route("/artifacts/:artifact_id", get(handlers::get_artifact::<S>))
        .route(
            "/artifacts/:artifact_id/summary",
            get(handlers::get_artifact_summary::<S>),
        )
        // Working Commit endpoints (staging changes before commit)
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit",
            post(handlers::create_working_commit::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit",
            get(handlers::get_working_commit_resolved::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit",
            delete(handlers::abandon_working_commit::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/commit",
            post(handlers::commit_working_changes::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/validate",
            get(handlers::validate_working_commit::<S>),
        )
        .route(
            "/databases/:db_id/commits/:commit_hash/validate",
            get(handlers::validate_commit::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/raw",
            get(handlers::get_active_working_commit_raw::<S>),
        )
        // Legacy Working Commit Staging Routes - DEPRECATED (kept for backward compatibility)
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema/classes/:class_id",
            patch(handlers::stage_class_schema_update::<S>),
        )
        // NOTE: instance staging route removed due to conflict with new working-commit endpoint
        // NEW: Working Commit Modification Endpoints (RECOMMENDED)
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/classes/:class_id",
            patch(handlers::update_working_commit_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/classes/:class_id",
            delete(handlers::delete_working_commit_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances",
            post(handlers::create_working_commit_instance::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id",
            patch(handlers::update_working_commit_instance::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id",
            delete(handlers::delete_working_commit_instance::<S>),
        )
        // Commit Tagging endpoints
        .route(
            "/commits/:commit_hash/tags",
            post(handlers::create_commit_tag::<S>),
        )
        .route(
            "/commits/:commit_hash/tags",
            get(handlers::get_commit_tags::<S>),
        )
        .route(
            "/tags/:tag_id",
            delete(handlers::delete_commit_tag::<S>),
        )
        .route(
            "/commits/:commit_hash/tagged",
            get(handlers::get_tagged_commit::<S>),
        )
        .route(
            "/databases/:db_id/tagged-commits",
            get(handlers::list_tagged_commits::<S>),
        )
        .route(
            "/databases/:db_id/commits/search",
            get(handlers::search_commits_by_tags::<S>),
        )
}
