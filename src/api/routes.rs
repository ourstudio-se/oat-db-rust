use axum::{
    routing::{delete, get, patch, post},
    Router,
};
use std::sync::Arc;

use crate::api::{branch_handlers, handlers, merge_handlers};
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
        // DEFAULT BRANCH working-commit endpoints (assumes main branch)
        .route(
            "/databases/:db_id/working-commit",
            get(handlers::get_default_branch_working_commit::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/schema",
            get(handlers::get_default_branch_working_commit_schema::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/schema/classes/:class_id",
            get(handlers::get_default_branch_working_commit_class::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/instances",
            get(handlers::list_default_branch_working_commit_instances::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/instances/:instance_id",
            get(handlers::get_default_branch_working_commit_instance::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/instances/:instance_id/query",
            post(handlers::query_default_branch_working_commit_instance::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/instances/:instance_id/query",
            get(handlers::get_default_branch_working_commit_instance_query::<S>),
        )
        .route(
            "/databases/:db_id/working-commit/instances/:instance_id/propagate",
            get(handlers::get_default_branch_working_commit_instance_propagate::<S>),
        )
        // Database-level READ-ONLY queries (automatically use main branch)
        // For modifications, use working-commit endpoints
        .route(
            "/databases/:db_id/schema",
            get(handlers::get_database_schema::<S>),
        )
        .route(
            "/databases/:db_id/schema/classes/:class_id",
            get(handlers::get_database_class::<S>),
        )
        .route(
            "/databases/:db_id/instances",
            get(handlers::list_database_instances::<S>),
        )
        .route(
            "/databases/:db_id/instances/:id",
            get(handlers::get_database_instance::<S>),
        )
        // Branch management - READ-ONLY
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
        // Branch-level data access - READ-ONLY
        // For modifications, use working-commit endpoints
        .route(
            "/databases/:db_id/branches/:branch_id/schema",
            get(handlers::get_schema::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/schema/classes/:class_id",
            get(handlers::get_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances",
            get(handlers::list_instances::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:id",
            get(handlers::get_instance::<S>),
        )
        // Branch operations (git-like)
        // NEW: Two-phase merge operations with conflict resolution
        .route(
            "/databases/:db_id/branches/:branch_name/merge",
            post(merge_handlers::start_merge::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_name/merge/validate",
            post(merge_handlers::validate_merge::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_name/merge/status",
            get(merge_handlers::get_merge_status::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_name/merge/resolve",
            post(merge_handlers::resolve_merge_conflicts::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_name/merge",
            delete(merge_handlers::abort_merge::<S>),
        )
        // Legacy merge endpoint (deprecated)
        .route(
            "/databases/:db_id/branches/:branch_id/merge-legacy",
            post(branch_handlers::merge_branch::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/delete",
            post(branch_handlers::delete_branch::<S>),
        )
        // Instance-specific query/solve endpoints (GET with URL parameters)
        .route(
            "/databases/:db_id/instances/:instance_id/query",
            get(handlers::get_database_instance_query::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/query",
            get(handlers::get_branch_instance_query::<S>),
        )
        // Propagate endpoints
        .route(
            "/databases/:db_id/instances/:instance_id/propagate",
            get(handlers::get_database_instance_propagate::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/propagate",
            get(handlers::get_branch_instance_propagate::<S>),
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
        // Analysis endpoints
        .route(
            "/databases/:db_id/instances/:instance_id/analysis",
            post(handlers::analyze_database_instance::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/instances/:instance_id/analysis",
            post(handlers::analyze_branch_instance::<S>),
        )
        // Commit-specific query endpoint
        .route(
            "/databases/:db_id/commits/:commit_hash/instances/:instance_id/query",
            get(handlers::get_commit_instance_query::<S>),
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
        // NEW: Working Commit READ endpoints for current state 
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema", 
            get(handlers::get_working_commit_schema::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema/classes/:class_id",
            get(handlers::get_working_commit_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances",
            get(handlers::list_working_commit_instances::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id",
            get(handlers::get_working_commit_instance::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id/query",
            post(handlers::query_working_commit_instance_configuration::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id/query",
            get(handlers::get_working_commit_instance_query::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id/propagate",
            get(handlers::get_working_commit_instance_propagate::<S>),
        )
        .route(
            "/databases/:db_id/commits/:commit_hash/validate",
            get(handlers::validate_commit::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/raw",
            get(handlers::get_active_working_commit_raw::<S>),
        )
        // NOTE: instance staging route removed due to conflict with new working-commit endpoint
        // NEW: Working Commit Modification Endpoints (RECOMMENDED)
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema/classes",
            post(handlers::create_working_commit_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema/classes/:class_id",
            patch(handlers::update_working_commit_class::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema/classes/:class_id",
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
        // Bulk update endpoints for working commits
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/schema/classes/bulk",
            patch(handlers::bulk_update_working_commit_classes::<S>),
        )
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/bulk",
            patch(handlers::bulk_update_working_commit_instances::<S>),
        )
        // Batch query endpoints for working commits
        .route(
            "/databases/:db_id/branches/:branch_id/working-commit/instances/:instance_id/batch-query",
            post(handlers::batch_query_working_commit_instance_configuration::<S>),
        )
        // Batch query endpoints for specific commits
        .route(
            "/databases/:db_id/commits/:commit_hash/instances/:instance_id/batch-query",
            post(handlers::batch_query_commit_instance_configuration::<S>),
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
