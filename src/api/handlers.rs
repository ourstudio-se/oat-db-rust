use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Json},
    Json as RequestJson,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::logic::branch_ops::{
    BranchOperations, MergeValidationResult, RebaseResult, RebaseValidationResult,
};
use crate::logic::validate_simple::ValidationResult;
use crate::logic::{Expander, SimpleValidator};
use crate::model::{
    Branch, ClassDef, ClassDefUpdate, ConfigurationArtifact, Database, Domain, ExpandedInstance, Id, Instance, InstanceFilter,
    InstanceQueryRequest, NewClassDef, NewConfigurationArtifact, NewDatabase, PropertyValue, RelationshipSelection, Schema, Version,
    WorkingCommit, NewWorkingCommit, NewCommit, Commit, UserContext, WorkingCommitStatus,
};
use crate::store::traits::{Store, VersionCompat, WorkingCommitStore, CommitStore, DatabaseStore};

pub type AppState<S> = Arc<S>;

#[derive(Debug, Deserialize)]
pub struct InstanceQuery {
    #[serde(rename = "class")]
    pub class_id: Option<String>,
    pub expand: Option<String>,
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct ExpandQuery {
    pub expand: Option<String>,
    pub depth: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct WorkingCommitQuery {
    /// If true, return only changes compared to base commit
    pub changes_only: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct ListResponse<T> {
    pub items: Vec<T>,
    pub total: usize,
}

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
pub enum InstanceResponse {
    Raw(Instance),
    Expanded(ExpandedInstance),
}

/// Sanitized commit response that excludes internal binary data
#[derive(Debug, Serialize)]
pub struct CommitResponse {
    pub hash: String,
    pub database_id: Id,
    pub parent_hash: Option<String>,
    pub author: Option<String>,
    pub message: Option<String>,
    pub created_at: String,
    pub data_size: i64,
    pub schema_classes_count: i32,
    pub instances_count: i32,
}

/// Enhanced working commit response with expanded relationships
#[derive(Debug, Clone, Serialize)]
pub struct WorkingCommitResponse {
    pub id: Id,
    pub database_id: Id,
    pub branch_name: Option<String>,
    pub based_on_hash: Option<String>,
    pub author: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub schema_data: Schema,
    pub instances_data: Vec<WorkingCommitInstance>, // Instances with both original and resolved relationships
    pub status: WorkingCommitStatus,
}

/// Instance with both original relationship configuration and resolved data
#[derive(Debug, Clone, Serialize)]
pub struct WorkingCommitInstance {
    pub id: Id,
    pub class: Id, // Use "class" for consistency with other endpoints
    pub properties: std::collections::HashMap<String, serde_json::Value>,
    pub relationships: std::collections::HashMap<String, WorkingCommitRelationship>,
    pub created_by: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_by: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Relationship with both original configuration and resolved data
#[derive(Debug, Clone, Serialize)]
pub struct WorkingCommitRelationship {
    /// Original relationship configuration (what was stored)
    pub original: RelationshipSelection,
    /// Resolved relationship data (what it currently resolves to)
    pub resolved: crate::model::ResolvedRelationship,
}

/// Enhanced changes response with expanded relationships
#[derive(Debug, Clone, Serialize)]
pub struct WorkingCommitChangesResponse {
    pub id: Id,
    pub database_id: Id,
    pub branch_name: Option<String>,
    pub based_on_hash: Option<String>,
    pub author: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub status: WorkingCommitStatus,
    pub schema_changes: crate::model::SchemaChanges,
    pub instance_changes: EnhancedInstanceChanges,
}

/// Instance changes with enhanced relationship data
#[derive(Debug, Clone, Serialize)]
pub struct EnhancedInstanceChanges {
    pub added: Vec<WorkingCommitInstance>,
    pub modified: Vec<WorkingCommitInstance>,
    pub deleted: Vec<Id>,
}

impl From<Commit> for CommitResponse {
    fn from(commit: Commit) -> Self {
        Self {
            hash: commit.hash,
            database_id: commit.database_id,
            parent_hash: commit.parent_hash,
            author: commit.author,
            message: commit.message,
            created_at: commit.created_at,
            data_size: commit.data_size,
            schema_classes_count: commit.schema_classes_count,
            instances_count: commit.instances_count,
        }
    }
}

impl ErrorResponse {
    pub fn new(message: &str) -> Self {
        Self {
            error: message.to_string(),
        }
    }
}

// Helper function to get the main branch name for a database
async fn get_main_branch_name<S: Store>(
    store: &S,
    db_id: &Id,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    match store.get_database(db_id).await {
        Ok(Some(database)) => {
            // Use the default branch name from database
            let default_branch_name = &database.default_branch_name;
            
            // Verify the default branch exists
            match store.get_branch(db_id, default_branch_name).await {
                Ok(Some(_)) => {
                    Ok(default_branch_name.clone())
                }
                Ok(None) => {
                    // Default branch doesn't exist, fall back to "main"
                    match store.get_branch(db_id, "main").await {
                        Ok(Some(_)) => Ok("main".to_string()),
                        _ => Err((
                            StatusCode::NOT_FOUND,
                            Json(ErrorResponse::new(
                                "Main branch 'main' not found in database",
                            )),
                        ))
                    }
                }
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(&e.to_string())),
                ))
            }
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Database not found")),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))
    }
}

// Helper function to convert legacy branch_id to branch name (temporary compatibility layer)
async fn get_branch_name_from_legacy_id<S: Store>(
    store: &S,
    db_id: &Id,
    branch_id: &Id,
) -> Result<String, (StatusCode, Json<ErrorResponse>)> {
    // In the new system, we need to find the branch by looking through all branches
    // Since branch_id is no longer used, we'll treat it as potentially being the branch name as a string
    let branch_id_as_name = branch_id.to_string();
    
    // First try to get the branch using the ID as a name
    match store.get_branch(db_id, &branch_id_as_name).await {
        Ok(Some(_)) => Ok(branch_id_as_name),
        Ok(None) => {
            // If not found, look through all branches to find one with the matching legacy ID
            // For now, we'll just return an error since this is a breaking change
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(&format!(
                    "Branch '{}' not found. The API now uses branch names instead of IDs.", 
                    branch_id
                ))),
            ))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))
    }
}

// API Documentation handlers
pub async fn get_api_docs<S: Store>(_state: State<AppState<S>>) -> Html<String> {
    let html = r#"
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>OAT Database API Documentation</title>
    <link rel="stylesheet" type="text/css" href="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui.css" />
    <style>
        html {
            box-sizing: border-box;
            overflow: -moz-scrollbars-vertical;
            overflow-y: scroll;
        }
        *, *:before, *:after {
            box-sizing: inherit;
        }
        body {
            margin: 0;
            background: #fafafa;
        }
    </style>
</head>
<body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-bundle.js"></script>
    <script src="https://unpkg.com/swagger-ui-dist@5.9.0/swagger-ui-standalone-preset.js"></script>
    <script>
        window.onload = function() {
            const ui = SwaggerUIBundle({
                url: '/docs/openapi.json',
                dom_id: '#swagger-ui',
                deepLinking: true,
                presets: [
                    SwaggerUIBundle.presets.apis,
                    SwaggerUIStandalonePreset
                ],
                plugins: [
                    SwaggerUIBundle.plugins.DownloadUrl
                ],
                layout: "StandaloneLayout"
            });
        };
    </script>
</body>
</html>
"#;
    Html(html.to_string())
}

pub async fn get_openapi_spec<S: Store>(_state: State<AppState<S>>) -> Json<serde_json::Value> {
    let spec = serde_json::json!({
        "openapi": "3.0.3",
        "info": {
            "title": "OAT Database API",
            "version": "1.0.0",
            "description": "A git-like combinatorial database API with branching support",
            "contact": {
                "name": "API Support"
            }
        },
        "servers": [
            {
                "url": "/",
                "description": "Current server"
            }
        ],
        "tags": [
            {
                "name": "Documentation",
                "description": "API documentation endpoints"
            },
            {
                "name": "Databases",
                "description": "Database management operations"
            },
            {
                "name": "Database Operations",
                "description": "Convenient operations that automatically use main branch"
            },
            {
                "name": "Branches",
                "description": "Git-like branch management"
            },
            {
                "name": "Branch Schema",
                "description": "Schema operations on specific branches"
            },
            {
                "name": "Branch Instances",
                "description": "Instance operations on specific branches"
            },
            {
                "name": "Branch Operations",
                "description": "Git-like operations (merge, commit, delete)"
            },
            {
                "name": "Type Validation",
                "description": "Instance type checking and validation against schema - user-controlled validation approach"
            },
            {
                "name": "Working Commits",
                "description": "Git-like staging system for grouping changes before committing. Features enhanced relationship resolution including schema default pools. PATCH operations work without validation blocking - use explicit validation when ready."
            },
            {
                "name": "Solve System",
                "description": "Configuration solve pipeline operations"
            },
            {
                "name": "Artifacts",
                "description": "Configuration artifact management and retrieval"
            }
        ],
        "paths": {
            "/docs": {
                "get": {
                    "tags": ["Documentation"],
                    "summary": "Get interactive API documentation",
                    "description": "Returns Swagger UI for interactive API exploration",
                    "responses": {
                        "200": {
                            "description": "HTML page with Swagger UI",
                            "content": {
                                "text/html": {
                                    "schema": {
                                        "type": "string"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/docs/openapi.json": {
                "get": {
                    "tags": ["Documentation"],
                    "summary": "Get OpenAPI specification",
                    "description": "Returns the OpenAPI 3.0 specification in JSON format",
                    "responses": {
                        "200": {
                            "description": "OpenAPI specification",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases": {
                "get": {
                    "tags": ["Databases"],
                    "summary": "List all databases",
                    "description": "Returns a list of all databases with total count",
                    "responses": {
                        "200": {
                            "description": "List of databases",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ListResponseDatabase"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Databases"],
                    "summary": "Create or update a database",
                    "description": "Creates a new database or updates an existing one",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/NewDatabase"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Database created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Database"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Bad request",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}": {
                "get": {
                    "tags": ["Databases"],
                    "summary": "Get a specific database",
                    "description": "Returns details of a specific database",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Database found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Database"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/schema": {
                "get": {
                    "tags": ["Database Operations"],
                    "summary": "Get database schema (main branch)",
                    "description": "Returns the schema for the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Schema found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Schema"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database or schema not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Database Operations"],
                    "summary": "Create/update database schema (main branch)",
                    "description": "Creates or updates the schema for the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Schema"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Schema created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Schema"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Bad request",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/schema/classes": {
                "post": {
                    "tags": ["Database Operations"],
                    "summary": "Add class to database schema (main branch)",
                    "description": "Adds a new class to the database's main branch schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/NewClassDef"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Class added successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Bad request",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/instances": {
                "get": {
                    "tags": ["Database Operations"],
                    "summary": "List database instances (main branch)",
                    "description": "Returns instances from the database's main branch with relationships expanded by default",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "type",
                            "in": "query",
                            "required": false,
                            "description": "Filter by instance type",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "expand",
                            "in": "query",
                            "required": false,
                            "description": "Comma-separated list of relationships to expand (defaults to all relationships)",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "depth",
                            "in": "query",
                            "required": false,
                            "description": "Expansion depth for including related instances (default 0 - relationships resolved but instances not included)",
                            "schema": {
                                "type": "integer",
                                "minimum": 0
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of instances",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ListResponseInstanceResponse"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Database Operations"],
                    "summary": "Create/update database instance (main branch)",
                    "description": "Creates or updates an instance in the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Instance"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Instance created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Bad request",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches": {
                "get": {
                    "tags": ["Branches"],
                    "summary": "List database branches",
                    "description": "Returns all branches for a database",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of branches",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ListResponseBranch"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/validate": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate all instances in database (main branch)",
                    "description": "Validates all instances in the database's main branch against the schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Validation completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ValidationResult"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/instances/{instance_id}/validate": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate single instance in database (main branch)",
                    "description": "Validates a specific instance in the database's main branch against the schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "instance_id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Validation completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ValidationResult"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database or instance not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/validate": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate all instances in branch",
                    "description": "Validates all instances in a specific branch against the schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Validation completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ValidationResult"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database or branch not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/instances/{instance_id}/validate": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate single instance in branch",
                    "description": "Validates a specific instance in a branch against the schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "instance_id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Validation completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ValidationResult"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database, branch, or instance not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{source_branch_id}/validate-merge": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate merge into database main branch",
                    "description": "Validates if a source branch can be merged into the database's main branch without validation errors",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "source_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Source branch ID to merge from",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Merge validation completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/MergeValidationResult"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database or branch not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{source_branch_id}/validate-merge/{target_branch_id}": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate merge between specific branches",
                    "description": "Validates if a source branch can be merged into a target branch without validation errors",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "source_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Source branch ID to merge from",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "target_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Target branch ID to merge into",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Merge validation completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/MergeValidationResult"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database or branches not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{feature_branch_id}/rebase": {
                "post": {
                    "tags": ["Branch Operations"],
                    "summary": "Rebase feature branch onto main",
                    "description": "Rebase a feature branch onto the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "feature_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Feature branch ID to rebase",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/RebaseRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Rebase completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/RebaseResult"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{feature_branch_id}/rebase/{target_branch_id}": {
                "post": {
                    "tags": ["Branch Operations"],
                    "summary": "Rebase feature branch onto target branch",
                    "description": "Rebase a feature branch onto a specific target branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "feature_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Feature branch ID to rebase",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "target_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Target branch ID to rebase onto",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/RebaseRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Rebase completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/RebaseResult"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{feature_branch_id}/validate-rebase": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate rebase onto main branch",
                    "description": "Validate if a feature branch can be rebased onto the main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "feature_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Feature branch ID to validate rebase for",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Rebase validation result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/RebaseValidationResult"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{feature_branch_id}/validate-rebase/{target_branch_id}": {
                "get": {
                    "tags": ["Type Validation"],
                    "summary": "Validate rebase onto target branch",
                    "description": "Validate if a feature branch can be rebased onto a specific target branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "feature_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Feature branch ID to validate rebase for",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "target_branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Target branch ID to validate rebase against",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Rebase validation result",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/RebaseValidationResult"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/schema/classes": {
                "post": {
                    "tags": ["Database Operations"],
                    "summary": "Add class to database schema (main branch)",
                    "description": "Add a new class definition to the database's main branch schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/NewClassDef"
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Class added successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/schema/classes/{class_id}": {
                "get": {
                    "tags": ["Database Operations"],
                    "summary": "Get class from database (main branch)",
                    "description": "Get a specific class definition from the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Class definition",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        }
                    }
                },
                "patch": {
                    "tags": ["Database Operations"],
                    "summary": "Update class in database (main branch)",
                    "description": "Update an existing class definition in the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ClassDefUpdate"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Class updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "tags": ["Database Operations"],
                    "summary": "Delete class from database (main branch)",
                    "description": "Delete a class definition from the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "204": {
                            "description": "Class deleted successfully"
                        }
                    }
                }
            },
            "/databases/{db_id}/instances": {
                "get": {
                    "tags": ["Database Operations"],
                    "summary": "List database instances (main branch)",
                    "description": "List all instances in the database's main branch with relationships expanded by default",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "type",
                            "in": "query",
                            "required": false,
                            "description": "Filter by instance type",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "expand",
                            "in": "query",
                            "required": false,
                            "description": "Comma-separated list of relationships to expand (defaults to all relationships)",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "depth",
                            "in": "query",
                            "required": false,
                            "description": "Expansion depth for including related instances (default 0 - relationships resolved but instances not included)",
                            "schema": {
                                "type": "integer",
                                "minimum": 0
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of instances",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/InstanceList"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Database Operations"],
                    "summary": "Create/update instance in database (main branch)",
                    "description": "Create or update an instance in the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Instance"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Instance created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/instances/{id}": {
                "get": {
                    "tags": ["Database Operations"],
                    "summary": "Get instance from database (main branch)",
                    "description": "Get a specific instance from the database's main branch with relationships expanded by default",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "expand",
                            "in": "query",
                            "required": false,
                            "description": "Comma-separated list of relationships to expand (defaults to all relationships)",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "depth",
                            "in": "query",
                            "required": false,
                            "description": "Expansion depth for including related instances (default 0 - relationships resolved but instances not included)",
                            "schema": {
                                "type": "integer",
                                "minimum": 0
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Instance data",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        }
                    }
                },
                "patch": {
                    "tags": ["Database Operations"],
                    "summary": "Update instance in database (main branch)",
                    "description": "Update an existing instance in the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/InstanceUpdate"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Instance updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "tags": ["Database Operations"],
                    "summary": "Delete instance from database (main branch)",
                    "description": "Delete an instance from the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "204": {
                            "description": "Instance deleted successfully"
                        }
                    }
                }
            },
            "/databases/{db_id}/instances/{id}/query": {
                "post": {
                    "tags": ["Database Operations"],
                    "summary": "Query instance configuration (main branch)",
                    "description": "Execute a configuration solve/query for a specific instance in the database's main branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/InstanceQueryRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Configuration artifact generated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ConfigurationArtifact"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database or instance not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Query execution failed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches": {
                "get": {
                    "tags": ["Branches"],
                    "summary": "List branches for database",
                    "description": "List all branches in the database",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of branches",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/BranchList"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Branches"],
                    "summary": "Create/update branch",
                    "description": "Create or update a branch in the database",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Branch"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Branch created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Branch"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}": {
                "get": {
                    "tags": ["Branches"],
                    "summary": "Get specific branch",
                    "description": "Get details of a specific branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Branch details",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Branch"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/schema": {
                "get": {
                    "tags": ["Branch Schema"],
                    "summary": "Get schema for branch",
                    "description": "Get the schema definition for a specific branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Schema definition",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Schema"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Branch Schema"],
                    "summary": "Create/update schema for branch",
                    "description": "Create or update the schema definition for a specific branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Schema"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Schema created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Schema"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/schema/classes": {
                "post": {
                    "tags": ["Branch Schema"],
                    "summary": "Add class to branch schema",
                    "description": "Add a new class definition to the branch schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/NewClassDef"
                                }
                            }
                        }
                    },
                    "responses": {
                        "201": {
                            "description": "Class added successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/schema/classes/{class_id}": {
                "get": {
                    "tags": ["Branch Schema"],
                    "summary": "Get class from branch",
                    "description": "Get a specific class definition from the branch schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Class definition",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        }
                    }
                },
                "patch": {
                    "tags": ["Branch Schema"],
                    "summary": "Update class in branch",
                    "description": "Update an existing class definition in the branch schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ClassDefUpdate"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Class updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ClassDef"
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "tags": ["Branch Schema"],
                    "summary": "Delete class from branch",
                    "description": "Delete a class definition from the branch schema",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "204": {
                            "description": "Class deleted successfully"
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/instances": {
                "get": {
                    "tags": ["Branch Instances"],
                    "summary": "List instances in branch",
                    "description": "List all instances in a specific branch with relationships expanded by default",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "type",
                            "in": "query",
                            "required": false,
                            "description": "Filter by instance type",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "expand",
                            "in": "query",
                            "required": false,
                            "description": "Comma-separated list of relationships to expand (defaults to all relationships)",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "depth",
                            "in": "query",
                            "required": false,
                            "description": "Expansion depth for including related instances (default 0 - relationships resolved but instances not included)",
                            "schema": {
                                "type": "integer",
                                "minimum": 0
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of instances",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/InstanceList"
                                    }
                                }
                            }
                        }
                    }
                },
                "post": {
                    "tags": ["Branch Instances"],
                    "summary": "Create/update instance in branch",
                    "description": "Create or update an instance in a specific branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/Instance"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Instance created/updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/instances/{id}": {
                "get": {
                    "tags": ["Branch Instances"],
                    "summary": "Get instance from branch",
                    "description": "Get a specific instance from a branch with relationships expanded by default",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "expand",
                            "in": "query",
                            "required": false,
                            "description": "Comma-separated list of relationships to expand (defaults to all relationships)",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "depth",
                            "in": "query",
                            "required": false,
                            "description": "Expansion depth for including related instances (default 0 - relationships resolved but instances not included)",
                            "schema": {
                                "type": "integer",
                                "minimum": 0
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Instance data",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        }
                    }
                },
                "patch": {
                    "tags": ["Branch Instances"],
                    "summary": "Update instance in branch",
                    "description": "Update an existing instance in a branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/InstanceUpdate"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Instance updated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Instance"
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "tags": ["Branch Instances"],
                    "summary": "Delete instance from branch",
                    "description": "Delete an instance from a branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "204": {
                            "description": "Instance deleted successfully"
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/instances/{id}/query": {
                "post": {
                    "tags": ["Branch Instances"],
                    "summary": "Query instance configuration (specific branch)",
                    "description": "Execute a configuration solve/query for a specific instance in a specific branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/InstanceQueryRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Configuration artifact generated successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ConfigurationArtifact"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Database, branch, or instance not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Query execution failed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/merge": {
                "post": {
                    "tags": ["Branch Operations"],
                    "summary": "Merge branch",
                    "description": "Merge a source branch into a target branch",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Source branch ID to merge",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/MergeRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Merge completed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/MergeResult"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/commit": {
                "post": {
                    "tags": ["Branch Operations"],
                    "summary": "Commit branch changes",
                    "description": "Commit changes to a branch with a message",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID to commit",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/CommitRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Branch committed successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/Branch"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/delete": {
                "post": {
                    "tags": ["Branch Operations"],
                    "summary": "Delete branch",
                    "description": "Delete a branch (only if merged or archived)",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID to delete",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": false,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/DeleteBranchRequest"
                                }
                            }
                        }
                    },
                    "responses": {
                        "204": {
                            "description": "Branch deleted successfully"
                        }
                    }
                }
            },
            "/solve": {
                "post": {
                    "tags": ["Solve System"],
                    "summary": "Create configuration artifact",
                    "description": "Execute solve pipeline to generate an immutable configuration artifact with resolved domains, properties, and selector snapshots",
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/NewConfigurationArtifact"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Configuration artifact created successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ConfigurationArtifact"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Solve operation failed",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/artifacts": {
                "get": {
                    "tags": ["Artifacts"],
                    "summary": "List configuration artifacts",
                    "description": "Retrieve a list of configuration artifacts with optional filtering",
                    "parameters": [
                        {
                            "name": "database_id",
                            "in": "query",
                            "required": false,
                            "description": "Filter by database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id", 
                            "in": "query",
                            "required": false,
                            "description": "Filter by branch ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "List of configuration artifacts",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ListResponseConfigurationArtifact"
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/artifacts/{artifact_id}": {
                "get": {
                    "tags": ["Artifacts"],
                    "summary": "Get configuration artifact",
                    "description": "Retrieve a specific configuration artifact by ID",
                    "parameters": [
                        {
                            "name": "artifact_id",
                            "in": "path", 
                            "required": true,
                            "description": "Configuration artifact ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Configuration artifact details",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ConfigurationArtifact"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Artifact not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/artifacts/{artifact_id}/summary": {
                "get": {
                    "tags": ["Artifacts"],
                    "summary": "Get artifact solve summary",
                    "description": "Retrieve a concise summary of the solve operation for a configuration artifact",
                    "parameters": [
                        {
                            "name": "artifact_id",
                            "in": "path",
                            "required": true, 
                            "description": "Configuration artifact ID",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Solve summary information",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "summary": {
                                                "type": "string",
                                                "description": "Human-readable solve summary"
                                            },
                                            "complete": {
                                                "type": "boolean", 
                                                "description": "Whether this is a complete configuration"
                                            },
                                            "instance_count": {
                                                "type": "integer",
                                                "description": "Number of instances in configuration"
                                            },
                                            "solve_time_ms": {
                                                "type": "integer",
                                                "description": "Total solve time in milliseconds"
                                            },
                                            "has_issues": {
                                                "type": "boolean",
                                                "description": "Whether solve had any issues or warnings"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Artifact not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/working-commit": {
                "post": {
                    "tags": ["Working Commits"],
                    "summary": "Create working commit (start staging)",
                    "description": "Creates a new working commit for staging changes before committing them as a group",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "X-User-Id",
                            "in": "header",
                            "required": false,
                            "description": "User ID for audit trail",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "X-User-Email",
                            "in": "header", 
                            "required": false,
                            "description": "User email for audit trail",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "X-User-Name",
                            "in": "header",
                            "required": false,
                            "description": "User name for audit trail",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["author"],
                                    "properties": {
                                        "author": {
                                            "type": "string",
                                            "description": "Author of the working commit",
                                            "example": "developer@company.com"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Working commit created successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "id": {
                                                "type": "string",
                                                "description": "Working commit ID"
                                            },
                                            "database_id": {
                                                "type": "string"
                                            },
                                            "branch_name": {
                                                "type": "string"
                                            },
                                            "author": {
                                                "type": "string"
                                            },
                                            "status": {
                                                "type": "string",
                                                "enum": ["active", "committing", "abandoned"]
                                            },
                                            "created_at": {
                                                "type": "string",
                                                "format": "date-time"
                                            },
                                            "updated_at": {
                                                "type": "string",
                                                "format": "date-time"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "400": {
                            "description": "Bad request - branch already has active working commit",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "Branch not found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                },
                "get": {
                    "tags": ["Working Commits"],
                    "summary": "Get active working commit with enhanced relationship resolution",
                    "description": "Retrieves the current active working commit for a branch showing staged changes. Features comprehensive relationship resolution including schema default pools - shows both explicit instance relationships and class schema default pool relationships.",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "changes_only",
                            "in": "query",
                            "required": false,
                            "description": "Return only changes compared to base commit",
                            "schema": {
                                "type": "boolean",
                                "default": false
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Active working commit found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "description": "Working commit with staged schema and instances"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "No active working commit found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                },
                "delete": {
                    "tags": ["Working Commits"],
                    "summary": "Abandon working commit",
                    "description": "Discards the active working commit and all staged changes without committing them",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Working commit abandoned successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "message": {
                                                "type": "string"
                                            },
                                            "working_commit_id": {
                                                "type": "string"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "No active working commit found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/working-commit/validate": {
                "get": {
                    "tags": ["Working Commits"],
                    "summary": "Validate staged changes",
                    "description": "Validates all instances in the working commit before committing. Returns detailed validation results including errors and warnings.",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "X-User-Id",
                            "in": "header",
                            "required": false,
                            "description": "User ID for audit trail",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "X-User-Email",
                            "in": "header",
                            "required": false,
                            "description": "User email for audit trail",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "X-User-Name",
                            "in": "header",
                            "required": false,
                            "description": "User name for audit trail",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Validation results",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "valid": {
                                                "type": "boolean",
                                                "description": "Whether all instances are valid"
                                            },
                                            "errors": {
                                                "type": "array",
                                                "description": "List of validation errors",
                                                "items": {
                                                    "type": "object",
                                                    "properties": {
                                                        "instance_id": {
                                                            "type": "string",
                                                            "description": "ID of the instance with error"
                                                        },
                                                        "error_type": {
                                                            "type": "string",
                                                            "enum": ["TypeMismatch", "MissingRequiredProperty", "UndefinedProperty", "InvalidValue", "ClassNotFound", "RelationshipError", "ValueTypeInconsistency"],
                                                            "description": "Type of validation error"
                                                        },
                                                        "message": {
                                                            "type": "string",
                                                            "description": "Human-readable error message"
                                                        },
                                                        "property_name": {
                                                            "type": "string",
                                                            "nullable": true,
                                                            "description": "Name/ID of the property causing the error"
                                                        },
                                                        "expected": {
                                                            "type": "string",
                                                            "nullable": true,
                                                            "description": "Expected value or type"
                                                        },
                                                        "actual": {
                                                            "type": "string",
                                                            "nullable": true,
                                                            "description": "Actual value found"
                                                        }
                                                    }
                                                }
                                            },
                                            "warnings": {
                                                "type": "array",
                                                "description": "List of validation warnings",
                                                "items": {
                                                    "type": "object"
                                                }
                                            },
                                            "instance_count": {
                                                "type": "integer",
                                                "description": "Total number of instances validated"
                                            },
                                            "validated_instances": {
                                                "type": "array",
                                                "description": "List of instance IDs that were validated",
                                                "items": {
                                                    "type": "string"
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "No active working commit found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "error": {
                                                "type": "string",
                                                "example": "No active working commit found"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "500": {
                            "description": "Internal server error"
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/working-commit/commit": {
                "post": {
                    "tags": ["Working Commits"],
                    "summary": "Commit staged changes",
                    "description": "Converts the active working commit into a permanent commit, making all staged changes permanent",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "type": "object",
                                    "required": ["message"],
                                    "properties": {
                                        "message": {
                                            "type": "string",
                                            "description": "Commit message describing the changes",
                                            "example": "Add description property to Color class and update all instances"
                                        },
                                        "author": {
                                            "type": "string",
                                            "description": "Optional commit author (defaults to working commit author)",
                                            "example": "developer@company.com"
                                        }
                                    }
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Changes committed successfully",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "properties": {
                                            "commit_hash": {
                                                "type": "string",
                                                "description": "SHA-256 hash of the new commit"
                                            },
                                            "message": {
                                                "type": "string"
                                            },
                                            "author": {
                                                "type": "string"
                                            },
                                            "created_at": {
                                                "type": "string",
                                                "format": "date-time"
                                            },
                                            "parent_hash": {
                                                "type": "string"
                                            },
                                            "schema_classes_count": {
                                                "type": "integer"
                                            },
                                            "instances_count": {
                                                "type": "integer"
                                            }
                                        }
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "No active working commit found",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        },
                        "422": {
                            "description": "Validation error - staged changes are invalid",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "$ref": "#/components/schemas/ErrorResponse"
                                    }
                                }
                            }
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/working-commit/raw": {
                "get": {
                    "tags": ["Working Commits"],
                    "summary": "Get raw working commit",
                    "description": "Retrieves the raw working commit data without relationship resolution - shows original configuration only",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "responses": {
                        "200": {
                            "description": "Raw working commit data",
                            "content": {
                                "application/json": {
                                    "schema": {
                                        "type": "object",
                                        "description": "Working commit without relationship resolution enhancements"
                                    }
                                }
                            }
                        },
                        "404": {
                            "description": "No active working commit found"
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/working-commit/schema/classes/{class_id}": {
                "patch": {
                    "tags": ["Working Commits"],
                    "summary": "Stage class schema update",
                    "description": "Updates a class schema in the working commit. If no working commit exists, one is automatically created.",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "class_id",
                            "in": "path",
                            "required": true,
                            "description": "Class ID to update",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/ClassDefUpdate"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Class staged successfully"
                        },
                        "404": {
                            "description": "Class or branch not found"
                        }
                    }
                }
            },
            "/databases/{db_id}/branches/{branch_id}/working-commit/instances/{instance_id}": {
                "patch": {
                    "tags": ["Working Commits"],
                    "summary": "Stage instance property update",
                    "description": "Updates instance properties in the working commit. If no working commit exists, one is automatically created.",
                    "parameters": [
                        {
                            "name": "db_id",
                            "in": "path",
                            "required": true,
                            "description": "Database ID",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "branch_id",
                            "in": "path",
                            "required": true,
                            "description": "Branch ID/Name",
                            "schema": {
                                "type": "string"
                            }
                        },
                        {
                            "name": "instance_id",
                            "in": "path",
                            "required": true,
                            "description": "Instance ID to update",
                            "schema": {
                                "type": "string"
                            }
                        }
                    ],
                    "requestBody": {
                        "required": true,
                        "content": {
                            "application/json": {
                                "schema": {
                                    "$ref": "#/components/schemas/InstanceUpdate"
                                }
                            }
                        }
                    },
                    "responses": {
                        "200": {
                            "description": "Instance properties staged successfully"
                        },
                        "404": {
                            "description": "Instance or branch not found"
                        }
                    }
                }
            }
        },
        "components": {
            "schemas": {
                "NewDatabase": {
                    "type": "object",
                    "required": ["id", "name"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique database identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Human-readable database name"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional database description"
                        }
                    },
                    "example": {
                        "id": "furniture-db",
                        "name": "Furniture Database",
                        "description": "Kitchen bundles: tables, chairs, options"
                    }
                },
                "Database": {
                    "type": "object",
                    "required": ["id", "name", "created_at"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique database identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Human-readable database name"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional database description"
                        },
                        "default_branch_id": {
                            "type": "string",
                            "nullable": true,
                            "description": "Default branch ID (usually main)"
                        },
                        "created_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "Creation timestamp"
                        }
                    },
                    "example": {
                        "id": "furniture_catalog",
                        "name": "Furniture Catalog",
                        "description": "Sample furniture database with beds, fabrics, and components",
                        "default_branch_id": "main",
                        "created_at": "2024-01-01T00:00:00Z"
                    }
                },
                "Branch": {
                    "type": "object",
                    "required": ["id", "database_id", "name", "status", "created_at", "updated_at"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique branch identifier"
                        },
                        "database_id": {
                            "type": "string",
                            "description": "Database this branch belongs to"
                        },
                        "name": {
                            "type": "string",
                            "description": "Branch name (e.g., 'main', 'feature-xyz')"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional branch description"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["Active", "Merged", "Archived"],
                            "description": "Branch status"
                        },
                        "parent_branch_id": {
                            "type": "string",
                            "nullable": true,
                            "description": "Parent branch ID if branched from another branch"
                        },
                        "created_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "Creation timestamp"
                        },
                        "updated_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "Last update timestamp"
                        },
                        "author": {
                            "type": "string",
                            "nullable": true,
                            "description": "Branch creator"
                        }
                    }
                },
                "Schema": {
                    "type": "object",
                    "required": ["id", "branch_id", "classes"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Schema identifier"
                        },
                        "branch_id": {
                            "type": "string",
                            "description": "Branch this schema belongs to"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional schema description"
                        },
                        "classes": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ClassDef"
                            },
                            "description": "Class definitions in this schema"
                        }
                    }
                },
                "ClassDef": {
                    "type": "object",
                    "required": ["id", "name", "properties", "relationships", "derived"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Class identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Class name"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional class description"
                        },
                        "properties": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/PropertyDef"
                            },
                            "description": "Property definitions"
                        },
                        "relationships": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/RelationshipDef"
                            },
                            "description": "Relationship definitions"
                        },
                        "derived": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DerivedDef"
                            },
                            "description": "Derived property definitions"
                        }
                    }
                },
                "NewClassDef": {
                    "type": "object",
                    "required": ["name", "properties", "relationships", "derived"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Class name"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional class description"
                        },
                        "properties": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/PropertyDef"
                            },
                            "description": "Property definitions"
                        },
                        "relationships": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/RelationshipDef"
                            },
                            "description": "Relationship definitions"
                        },
                        "derived": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DerivedDef"
                            },
                            "description": "Derived property definitions"
                        }
                    }
                },
                "PropertyDef": {
                    "type": "object",
                    "required": ["id", "name", "data_type"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Property identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Property name"
                        },
                        "data_type": {
                            "$ref": "#/components/schemas/DataType"
                        },
                        "required": {
                            "type": "boolean",
                            "nullable": true,
                            "description": "Whether property is required"
                        }
                    }
                },
                "RelationshipDef": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string"
                        },
                        "name": {
                            "type": "string"
                        },
                        "targets": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            }
                        }
                    }
                },
                "DerivedDef": {
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string"
                        },
                        "name": {
                            "type": "string"
                        },
                        "data_type": {
                            "$ref": "#/components/schemas/DataType"
                        }
                    }
                },
                "Instance": {
                    "type": "object",
                    "required": ["id", "branch_id", "type", "properties", "relationships"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Instance identifier"
                        },
                        "branch_id": {
                            "type": "string",
                            "description": "Branch this instance belongs to"
                        },
                        "type": {
                            "type": "string",
                            "description": "Instance type (class name)"
                        },
                        "domain": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional domain identifier"
                        },
                        "properties": {
                            "type": "object",
                            "additionalProperties": {
                                "$ref": "#/components/schemas/PropertyValue"
                            },
                            "description": "Property values"
                        },
                        "relationships": {
                            "type": "object",
                            "additionalProperties": {
                                "$ref": "#/components/schemas/RelationshipSelection"
                            },
                            "description": "Relationship selections"
                        }
                    }
                },
                "PropertyValue": {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "Literal": {
                                    "$ref": "#/components/schemas/TypedValue"
                                }
                            }
                        },
                        {
                            "type": "object",
                            "properties": {
                                "Conditional": {
                                    "$ref": "#/components/schemas/RuleSet"
                                }
                            }
                        }
                    ]
                },
                "TypedValue": {
                    "type": "object",
                    "required": ["value", "data_type"],
                    "properties": {
                        "value": {
                            "description": "JSON value of any type"
                        },
                        "data_type": {
                            "$ref": "#/components/schemas/DataType"
                        }
                    }
                },
                "RelationshipSelection": {
                    "oneOf": [
                        {
                            "type": "object",
                            "properties": {
                                "Ids": {
                                    "type": "object",
                                    "properties": {
                                        "ids": {
                                            "type": "array",
                                            "items": {
                                                "type": "string"
                                            }
                                        }
                                    }
                                }
                            },
                            "description": "Explicit list of instance IDs"
                        },
                        {
                            "type": "object",
                            "properties": {
                                "PoolBased": {
                                    "type": "object",
                                    "properties": {
                                        "pool": {
                                            "$ref": "#/components/schemas/InstanceFilter",
                                            "description": "Pool filter to determine available instances"
                                        }
                                    }
                                }
                            },
                            "description": "Pool-based relationship with filter for available instances. The solver handles selection from this pool based on quantifiers."
                        }
                    ]
                },
                "InstanceFilter": {
                    "type": "object",
                    "description": "Filter specification for instances - defines pool of available instances",
                    "properties": {
                        "type": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "Filter by instance class types"
                        },
                        "where": {
                            "type": "object",
                            "description": "Boolean expression for filtering instances"
                        },
                        "limit": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum number of instances to include"
                        }
                    }
                },
                "RuleSet": {
                    "type": "object",
                    "description": "Rule set for conditional values"
                },
                "DataType": {
                    "type": "string",
                    "enum": ["String", "Number", "Boolean", "Object", "Array"],
                    "description": "Data type enumeration"
                },
                "ListResponseDatabase": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/Database"
                            }
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of items"
                        }
                    }
                },
                "ListResponseBranch": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/Branch"
                            }
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of items"
                        }
                    }
                },
                "ListResponseInstance": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/Instance"
                            }
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of items"
                        }
                    }
                },
                "ErrorResponse": {
                    "type": "object",
                    "required": ["error"],
                    "properties": {
                        "error": {
                            "type": "string",
                            "description": "Error message"
                        }
                    }
                },
                "ValidationResult": {
                    "type": "object",
                    "required": ["valid", "errors", "warnings", "instance_count", "validated_instances"],
                    "properties": {
                        "valid": {
                            "type": "boolean",
                            "description": "Whether all instances passed validation"
                        },
                        "errors": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ValidationError"
                            },
                            "description": "List of validation errors"
                        },
                        "warnings": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ValidationWarning"
                            },
                            "description": "List of validation warnings"
                        },
                        "instance_count": {
                            "type": "integer",
                            "description": "Total number of instances validated"
                        },
                        "validated_instances": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "List of instance IDs that were validated"
                        }
                    }
                },
                "ValidationError": {
                    "type": "object",
                    "required": ["instance_id", "error_type", "message"],
                    "properties": {
                        "instance_id": {
                            "type": "string",
                            "description": "ID of the instance with the error"
                        },
                        "error_type": {
                            "type": "string",
                            "enum": ["TypeMismatch", "MissingRequiredProperty", "UndefinedProperty", "InvalidValue", "ClassNotFound", "RelationshipError", "ValueTypeInconsistency"],
                            "description": "Type of validation error"
                        },
                        "message": {
                            "type": "string",
                            "description": "Human-readable error message"
                        },
                        "property_name": {
                            "type": "string",
                            "nullable": true,
                            "description": "Name of the property with the error"
                        },
                        "expected": {
                            "type": "string",
                            "nullable": true,
                            "description": "Expected value or type"
                        },
                        "actual": {
                            "type": "string",
                            "nullable": true,
                            "description": "Actual value or type found"
                        }
                    }
                },
                "ValidationWarning": {
                    "type": "object",
                    "required": ["instance_id", "warning_type", "message"],
                    "properties": {
                        "instance_id": {
                            "type": "string",
                            "description": "ID of the instance with the warning"
                        },
                        "warning_type": {
                            "type": "string",
                            "enum": ["UnusedProperty", "ConditionalPropertySkipped", "RelationshipNotValidated"],
                            "description": "Type of validation warning"
                        },
                        "message": {
                            "type": "string",
                            "description": "Human-readable warning message"
                        },
                        "property_name": {
                            "type": "string",
                            "nullable": true,
                            "description": "Name of the property with the warning"
                        }
                    }
                },
                "MergeValidationResult": {
                    "type": "object",
                    "required": ["can_merge", "conflicts", "simulated_schema_valid", "affected_instances"],
                    "properties": {
                        "can_merge": {
                            "type": "boolean",
                            "description": "Whether the merge can proceed without validation errors"
                        },
                        "conflicts": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/MergeConflict"
                            },
                            "description": "List of merge conflicts including validation issues"
                        },
                        "validation_result": {
                            "nullable": true,
                            "allOf": [
                                {
                                    "$ref": "#/components/schemas/ValidationResult"
                                }
                            ],
                            "description": "Detailed validation result for the simulated merge"
                        },
                        "simulated_schema_valid": {
                            "type": "boolean",
                            "description": "Whether the merged schema would be valid"
                        },
                        "affected_instances": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "List of instance IDs that would be affected by the merge"
                        }
                    }
                },
                "MergeConflict": {
                    "type": "object",
                    "required": ["conflict_type", "resource_id", "description"],
                    "properties": {
                        "conflict_type": {
                            "type": "string",
                            "enum": ["SchemaModified", "InstanceModified", "InstanceDeleted", "ClassAdded", "ValidationConflict"],
                            "description": "Type of merge conflict"
                        },
                        "resource_id": {
                            "type": "string",
                            "description": "ID of the resource causing the conflict"
                        },
                        "description": {
                            "type": "string",
                            "description": "Human-readable description of the conflict"
                        }
                    }
                },
                "RebaseRequest": {
                    "type": "object",
                    "properties": {
                        "author": {
                            "type": "string",
                            "nullable": true,
                            "description": "Author of the rebase operation"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force rebase even if conflicts are detected",
                            "default": false
                        }
                    }
                },
                "RebaseResult": {
                    "type": "object",
                    "required": ["success", "conflicts", "message", "rebased_instances", "rebased_schema_changes"],
                    "properties": {
                        "success": {
                            "type": "boolean",
                            "description": "Whether the rebase operation succeeded"
                        },
                        "conflicts": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/MergeConflict"
                            },
                            "description": "List of conflicts encountered during rebase"
                        },
                        "message": {
                            "type": "string",
                            "description": "Human-readable message describing the rebase result"
                        },
                        "rebased_instances": {
                            "type": "integer",
                            "description": "Number of instances that were rebased"
                        },
                        "rebased_schema_changes": {
                            "type": "boolean",
                            "description": "Whether schema changes were applied during rebase"
                        }
                    }
                },
                "RebaseValidationResult": {
                    "type": "object",
                    "required": ["can_rebase", "conflicts", "needs_rebase", "affected_instances"],
                    "properties": {
                        "can_rebase": {
                            "type": "boolean",
                            "description": "Whether the rebase can be performed successfully"
                        },
                        "conflicts": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/MergeConflict"
                            },
                            "description": "List of conflicts that would occur during rebase"
                        },
                        "validation_result": {
                            "$ref": "#/components/schemas/ValidationResult",
                            "nullable": true,
                            "description": "Validation result after simulated rebase"
                        },
                        "needs_rebase": {
                            "type": "boolean",
                            "description": "Whether the feature branch needs to be rebased"
                        },
                        "affected_instances": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "List of instance IDs that would be affected by the rebase"
                        }
                    }
                },
                "NewClassDef": {
                    "type": "object",
                    "required": ["name", "properties", "relationships", "derived"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the class"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional description of the class"
                        },
                        "properties": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/PropertyDef"
                            },
                            "description": "Property definitions for this class"
                        },
                        "relationships": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/RelationshipDef"
                            },
                            "description": "Relationship definitions for this class"
                        },
                        "derived": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DerivedDef"
                            },
                            "description": "Derived property definitions for this class"
                        }
                    }
                },
                "ClassDefUpdate": {
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Name of the class"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional description of the class"
                        },
                        "properties": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/PropertyDef"
                            },
                            "description": "Property definitions for this class"
                        },
                        "relationships": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/RelationshipDef"
                            },
                            "description": "Relationship definitions for this class"
                        },
                        "derived": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/DerivedDef"
                            },
                            "description": "Derived property definitions for this class"
                        }
                    }
                },
                "InstanceList": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/Instance"
                            },
                            "description": "List of instances"
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of instances"
                        }
                    }
                },
                "InstanceUpdate": {
                    "type": "object",
                    "properties": {
                        "class": {
                            "type": "string",
                            "description": "Type/class of the instance"
                        },
                        "domain": {
                            "type": "string",
                            "nullable": true,
                            "description": "Domain scope for the instance"
                        },
                        "properties": {
                            "type": "object",
                            "description": "Instance properties (key-value pairs)"
                        },
                        "relationships": {
                            "type": "object",
                            "description": "Instance relationships"
                        }
                    }
                },
                "BranchList": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/Branch"
                            },
                            "description": "List of branches"
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of branches"
                        }
                    }
                },
                "Branch": {
                    "type": "object",
                    "required": ["id", "database_id", "name", "created_at", "commit_hash", "status"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique branch identifier"
                        },
                        "database_id": {
                            "type": "string",
                            "description": "ID of the database this branch belongs to"
                        },
                        "name": {
                            "type": "string",
                            "description": "Branch name"
                        },
                        "description": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional branch description"
                        },
                        "created_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "ISO 8601 timestamp of branch creation"
                        },
                        "parent_branch_id": {
                            "type": "string",
                            "nullable": true,
                            "description": "ID of the parent branch this was created from"
                        },
                        "commit_hash": {
                            "type": "string",
                            "description": "Current commit hash/state identifier"
                        },
                        "commit_message": {
                            "type": "string",
                            "nullable": true,
                            "description": "Latest commit message"
                        },
                        "author": {
                            "type": "string",
                            "nullable": true,
                            "description": "Author of the latest commit"
                        },
                        "status": {
                            "type": "string",
                            "enum": ["active", "merged", "archived"],
                            "description": "Current branch status"
                        }
                    }
                },
                "MergeRequest": {
                    "type": "object",
                    "properties": {
                        "target_branch_id": {
                            "type": "string",
                            "nullable": true,
                            "description": "Target branch ID (defaults to main)"
                        },
                        "author": {
                            "type": "string",
                            "nullable": true,
                            "description": "Author of the merge operation"
                        },
                        "force": {
                            "type": "boolean",
                            "description": "Force merge even if conflicts are detected",
                            "default": false
                        }
                    }
                },
                "CommitRequest": {
                    "type": "object",
                    "required": ["message"],
                    "properties": {
                        "message": {
                            "type": "string",
                            "description": "Commit message"
                        },
                        "author": {
                            "type": "string",
                            "nullable": true,
                            "description": "Author of the commit"
                        }
                    }
                },
                "DeleteBranchRequest": {
                    "type": "object",
                    "properties": {
                        "force": {
                            "type": "boolean",
                            "description": "Force delete even if branch is active",
                            "default": false
                        }
                    }
                },
                "PropertyDef": {
                    "type": "object",
                    "required": ["id", "name", "data_type"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique property identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Property name"
                        },
                        "data_type": {
                            "type": "string",
                            "enum": ["String", "Number", "Boolean", "Date"],
                            "description": "Data type of the property"
                        },
                        "required": {
                            "type": "boolean",
                            "nullable": true,
                            "description": "Whether this property is required"
                        }
                    }
                },
                "RelationshipDef": {
                    "type": "object",
                    "required": ["id", "name", "targets", "quantifier"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique relationship identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Relationship name"
                        },
                        "targets": {
                            "type": "array",
                            "items": {
                                "type": "string"
                            },
                            "description": "Target class names for this relationship"
                        },
                        "quantifier": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "properties": {
                                        "EXACTLY": {
                                            "type": "integer",
                                            "minimum": 0
                                        }
                                    },
                                    "required": ["EXACTLY"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "AT_LEAST": {
                                            "type": "integer",
                                            "minimum": 0
                                        }
                                    },
                                    "required": ["AT_LEAST"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "AT_MOST": {
                                            "type": "integer",
                                            "minimum": 0
                                        }
                                    },
                                    "required": ["AT_MOST"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "object",
                                    "properties": {
                                        "RANGE": {
                                            "type": "array",
                                            "items": {
                                                "type": "integer",
                                                "minimum": 0
                                            },
                                            "minItems": 2,
                                            "maxItems": 2
                                        }
                                    },
                                    "required": ["RANGE"],
                                    "additionalProperties": false
                                },
                                {
                                    "type": "string",
                                    "enum": ["OPTIONAL", "ANY", "ALL"]
                                }
                            ],
                            "description": "Relationship quantifier - can be numeric (EXACTLY, AT_LEAST, AT_MOST, RANGE) or symbolic (OPTIONAL, ANY, ALL)"
                        },
                        "universe": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional universe constraint"
                        },
                        "default_pool": {
                            "type": "object",
                            "description": "Default pool strategy for relationship instances - determines what instances are available by default"
                        }
                    }
                },
                "DerivedDef": {
                    "type": "object",
                    "required": ["id", "name", "derivation_type", "expression"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique derived property identifier"
                        },
                        "name": {
                            "type": "string",
                            "description": "Derived property name"
                        },
                        "derivation_type": {
                            "type": "string",
                            "description": "Type of derivation"
                        },
                        "expression": {
                            "type": "object",
                            "description": "Expression defining the derivation"
                        }
                    }
                },
                "NewConfigurationArtifact": {
                    "type": "object",
                    "required": ["resolution_context"],
                    "properties": {
                        "resolution_context": {
                            "$ref": "#/components/schemas/ResolutionContext"
                        },
                        "user_metadata": {
                            "$ref": "#/components/schemas/ArtifactUserMetadata"
                        }
                    }
                },
                "InstanceQueryRequest": {
                    "type": "object",
                    "description": "Instance-specific query request where database_id, branch_id, and instance_id are extracted from path parameters",
                    "properties": {
                        "policies": {
                            "$ref": "#/components/schemas/ResolutionPolicies",
                            "description": "Resolution policies for this query (uses defaults if not specified)"
                        },
                        "commit_hash": {
                            "type": "string",
                            "nullable": true,
                            "description": "Optional commit hash for point-in-time resolution"
                        },
                        "user_metadata": {
                            "$ref": "#/components/schemas/ArtifactUserMetadata",
                            "nullable": true,
                            "description": "Optional user metadata for the generated artifact"
                        },
                        "context_metadata": {
                            "$ref": "#/components/schemas/ResolutionContextMetadata",
                            "nullable": true,
                            "description": "Optional metadata for the resolution context"
                        }
                    }
                },
                "ConfigurationArtifact": {
                    "type": "object",
                    "required": ["id", "created_at", "resolution_context", "resolved_domains", "resolved_properties", "selector_snapshots", "solve_metadata"],
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Unique artifact identifier"
                        },
                        "created_at": {
                            "type": "string",
                            "format": "date-time",
                            "description": "When artifact was created"
                        },
                        "resolution_context": {
                            "$ref": "#/components/schemas/ResolutionContext"
                        },
                        "resolved_domains": {
                            "type": "object",
                            "additionalProperties": {
                                "$ref": "#/components/schemas/Domain"
                            },
                            "description": "Resolved domains for all instances"
                        },
                        "resolved_properties": {
                            "type": "object", 
                            "additionalProperties": {
                                "type": "object",
                                "additionalProperties": true
                            },
                            "description": "Resolved property values including conditional properties"
                        },
                        "selector_snapshots": {
                            "type": "object",
                            "additionalProperties": {
                                "type": "object",
                                "additionalProperties": {
                                    "$ref": "#/components/schemas/SelectorSnapshot"
                                }
                            },
                            "description": "Snapshots of selector resolutions by instance and relationship"
                        },
                        "solve_metadata": {
                            "$ref": "#/components/schemas/SolveMetadata"
                        },
                        "user_metadata": {
                            "$ref": "#/components/schemas/ArtifactUserMetadata"
                        }
                    }
                },
                "ResolutionContext": {
                    "type": "object",
                    "required": ["database_id", "branch_id", "policies"],
                    "properties": {
                        "database_id": {
                            "type": "string",
                            "description": "Database to resolve against"
                        },
                        "branch_id": {
                            "type": "string",
                            "description": "Branch to resolve against"
                        },
                        "commit_hash": {
                            "type": "string",
                            "description": "Optional commit hash for point-in-time resolution"
                        },
                        "policies": {
                            "$ref": "#/components/schemas/ResolutionPolicies"
                        },
                        "metadata": {
                            "$ref": "#/components/schemas/ResolutionContextMetadata"
                        }
                    }
                },
                "ResolutionPolicies": {
                    "type": "object",
                    "required": ["cross_branch_policy", "missing_instance_policy", "empty_selection_policy"],
                    "properties": {
                        "cross_branch_policy": {
                            "type": "string",
                            "enum": ["reject", "allow_with_warnings", "allow"],
                            "description": "How to handle cross-branch references"
                        },
                        "missing_instance_policy": {
                            "type": "string",
                            "enum": ["fail", "skip", "placeholder"],
                            "description": "How to handle missing instances in static selectors"
                        },
                        "empty_selection_policy": {
                            "type": "string",
                            "enum": ["fail", "allow", "fallback"],
                            "description": "How to handle empty dynamic selections"
                        },
                        "max_selection_size": {
                            "type": "integer",
                            "minimum": 1,
                            "description": "Maximum instances a dynamic selector can resolve to"
                        },
                        "custom": {
                            "type": "object",
                            "additionalProperties": true,
                            "description": "Custom policy extensions"
                        }
                    }
                },
                "ResolutionContextMetadata": {
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string"
                        },
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"}
                        },
                        "created_at": {
                            "type": "string",
                            "format": "date-time"
                        },
                        "created_by": {
                            "type": "string"
                        },
                        "custom": {
                            "type": "object",
                            "additionalProperties": true
                        }
                    }
                },
                "Selector": {
                    "type": "object",
                    "required": ["resolution_mode"],
                    "properties": {
                        "resolution_mode": {
                            "type": "string",
                            "enum": ["static", "dynamic"],
                            "description": "How instances are resolved"
                        },
                        "filter": {
                            "$ref": "#/components/schemas/InstanceFilter",
                            "description": "Filter for dynamic selectors"
                        },
                        "materialized_ids": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Pre-materialized IDs for static selectors"
                        },
                        "metadata": {
                            "$ref": "#/components/schemas/SelectorMetadata"
                        }
                    }
                },
                "SelectorMetadata": {
                    "type": "object",
                    "properties": {
                        "description": {"type": "string"},
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"}
                        },
                        "custom": {
                            "type": "object",
                            "additionalProperties": true
                        }
                    }
                },
                "SelectorSnapshot": {
                    "type": "object",
                    "required": ["selector", "resolved_ids"],
                    "properties": {
                        "selector": {
                            "$ref": "#/components/schemas/Selector"
                        },
                        "resolved_ids": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Instance IDs that were resolved"
                        },
                        "resolution_notes": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/ResolutionNote"}
                        },
                        "resolution_time_ms": {
                            "type": "integer",
                            "description": "Time taken to resolve in milliseconds"
                        }
                    }
                },
                "ResolutionNote": {
                    "type": "object", 
                    "required": ["note_type", "message"],
                    "properties": {
                        "note_type": {
                            "type": "string",
                            "enum": ["info", "warning", "error", "cross_branch", "skipped_missing", "used_fallback", "truncated"]
                        },
                        "message": {
                            "type": "string"
                        },
                        "context": {
                            "type": "object",
                            "additionalProperties": true
                        }
                    }
                },
                "SolveMetadata": {
                    "type": "object",
                    "required": ["total_time_ms", "pipeline_phases", "statistics"],
                    "properties": {
                        "total_time_ms": {
                            "type": "integer",
                            "description": "Total solve time in milliseconds"
                        },
                        "pipeline_phases": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/PipelinePhase"}
                        },
                        "solver_info": {
                            "$ref": "#/components/schemas/SolverInfo"
                        },
                        "statistics": {
                            "$ref": "#/components/schemas/SolveStatistics"
                        },
                        "issues": {
                            "type": "array",
                            "items": {"$ref": "#/components/schemas/SolveIssue"}
                        }
                    }
                },
                "PipelinePhase": {
                    "type": "object",
                    "required": ["name", "duration_ms"],
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Phase name (snapshot, expand, evaluate, validate, compile)"
                        },
                        "duration_ms": {
                            "type": "integer",
                            "description": "Time taken for this phase"
                        },
                        "details": {
                            "type": "object",
                            "additionalProperties": true,
                            "description": "Additional phase-specific details"
                        }
                    }
                },
                "SolverInfo": {
                    "type": "object",
                    "required": ["name"],
                    "properties": {
                        "name": {"type": "string"},
                        "version": {"type": "string"},
                        "config": {
                            "type": "object",
                            "additionalProperties": true
                        }
                    }
                },
                "SolveStatistics": {
                    "type": "object",
                    "required": ["total_instances", "total_selectors", "conditional_properties_evaluated", "domains_resolved"],
                    "properties": {
                        "total_instances": {"type": "integer"},
                        "total_selectors": {"type": "integer"},
                        "conditional_properties_evaluated": {"type": "integer"},
                        "domains_resolved": {"type": "integer"},
                        "peak_memory_bytes": {"type": "integer"}
                    }
                },
                "SolveIssue": {
                    "type": "object",
                    "required": ["severity", "message"],
                    "properties": {
                        "severity": {
                            "type": "string",
                            "enum": ["info", "warning", "error", "critical"]
                        },
                        "message": {"type": "string"},
                        "component": {"type": "string"},
                        "context": {
                            "type": "object",
                            "additionalProperties": true
                        }
                    }
                },
                "ArtifactUserMetadata": {
                    "type": "object",
                    "properties": {
                        "name": {"type": "string"},
                        "description": {"type": "string"},
                        "tags": {
                            "type": "array",
                            "items": {"type": "string"}
                        },
                        "created_by": {"type": "string"},
                        "custom": {
                            "type": "object",
                            "additionalProperties": true
                        }
                    }
                },
                "Domain": {
                    "type": "object",
                    "required": ["lower", "upper"],
                    "properties": {
                        "lower": {
                            "type": "integer",
                            "description": "Lower bound of domain range"
                        },
                        "upper": {
                            "type": "integer", 
                            "description": "Upper bound of domain range"
                        }
                    }
                },
                "ListResponseConfigurationArtifact": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/ConfigurationArtifact"
                            }
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of artifacts"
                        }
                    }
                },
                "ListResponseInstanceResponse": {
                    "type": "object",
                    "required": ["items", "total"],
                    "properties": {
                        "items": {
                            "type": "array",
                            "items": {
                                "$ref": "#/components/schemas/InstanceResponse"
                            }
                        },
                        "total": {
                            "type": "integer",
                            "description": "Total number of instances"
                        }
                    }
                },
                "ResolvedRelationship": {
                    "type": "object",
                    "required": ["materialized_ids", "resolution_method"],
                    "properties": {
                        "materialized_ids": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "The actual resolved instance IDs"
                        },
                        "resolution_method": {
                            "$ref": "#/components/schemas/ResolutionMethod"
                        },
                        "resolution_details": {
                            "$ref": "#/components/schemas/ResolutionDetails"
                        }
                    }
                },
                "ResolutionMethod": {
                    "type": "string",
                    "enum": [
                        "explicit_ids",
                        "pool_filter_resolved", 
                        "pool_selection_resolved",
                        "dynamic_selector_resolved",
                        "all_instances_resolved",
                        "schema_default_resolved",
                        "empty_resolution"
                    ],
                    "description": "Method used to resolve the relationship"
                },
                "ResolutionDetails": {
                    "type": "object",
                    "properties": {
                        "original_definition": {
                            "type": "object",
                            "additionalProperties": true,
                            "description": "The original relationship definition before resolution"
                        },
                        "resolved_from": {
                            "type": "string",
                            "description": "What triggered the resolution"
                        },
                        "filter_description": {
                            "type": "string",
                            "description": "Description of filters/conditions applied"
                        },
                        "total_pool_size": {
                            "type": "integer",
                            "description": "Total number of instances that matched the pool before selection"
                        },
                        "filtered_out_count": {
                            "type": "integer",
                            "description": "Number of instances that were excluded by filters"
                        },
                        "resolution_time_us": {
                            "type": "integer",
                            "description": "Time taken for this relationship resolution (microseconds)"
                        },
                        "notes": {
                            "type": "array",
                            "items": {"type": "string"},
                            "description": "Any warnings or notes about the resolution"
                        }
                    }
                }
            }
        }
    });

    Json(spec)
}

// Type Validation handlers
pub async fn validate_database_instances<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
) -> Result<Json<ValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = match get_main_branch_name(&*store, &db_id).await {
        Ok(branch_name) => branch_name,
        Err(error_response) => return Err(error_response),
    };

    match SimpleValidator::validate_branch(&*store, &db_id, &main_branch_name).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn validate_branch_instances<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
) -> Result<Json<ValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Verify the database and branch exist
    match store.get_database(&db_id).await {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match SimpleValidator::validate_branch(&*store, &db_id, &branch_name).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn validate_single_instance<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, instance_id)): Path<(Id, Id)>,
) -> Result<Json<ValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = match get_main_branch_name(&*store, &db_id).await {
        Ok(branch_name) => branch_name,
        Err(error_response) => return Err(error_response),
    };

    // Get the instance
    let instance = match store.get_instance(&db_id, &main_branch_name, &instance_id).await {
        Ok(Some(inst)) => inst,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Instance not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Get the schema
    let schema = match store.get_schema(&db_id, &main_branch_name).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Schema not found for database")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match SimpleValidator::validate_instance(&*store, &instance, &schema).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn validate_branch_single_instance<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_id, instance_id)): Path<(Id, Id, Id)>,
) -> Result<Json<ValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Verify the database and branch exist
    match store.get_database(&db_id).await {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    // Get the instance
    let instance = match store.get_instance(&db_id, &branch_name, &instance_id).await {
        Ok(Some(inst)) => inst,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Instance not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Get the schema
    let schema = match store.get_schema(&db_id, &branch_name).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Schema not found for branch")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match SimpleValidator::validate_instance(&*store, &instance, &schema).await {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

// Merge Validation handlers
pub async fn validate_database_merge<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, source_branch_id)): Path<(Id, Id)>,
) -> Result<Json<MergeValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Get the main branch as target
    let target_branch_id = match get_main_branch_name(&*store, &db_id).await {
        Ok(branch_id) => branch_id,
        Err(error_response) => return Err(error_response),
    };

    match BranchOperations::check_merge_validation(&*store, &db_id, &source_branch_id, &db_id, &target_branch_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn validate_branch_merge<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, source_branch_id, target_branch_id)): Path<(Id, Id, Id)>,
) -> Result<Json<MergeValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Verify database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Verify both branches exist and belong to the database
    match store.get_version(&source_branch_id).await {
        Ok(Some(branch)) if branch.database_id == db_id => (),
        Ok(Some(_)) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "Source branch does not belong to the specified database",
                )),
            ))
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Source branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match store.get_version(&target_branch_id).await {
        Ok(Some(branch)) if branch.database_id == db_id => (),
        Ok(Some(_)) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(
                    "Target branch does not belong to the specified database",
                )),
            ))
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Target branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match BranchOperations::check_merge_validation(&*store, &db_id, &source_branch_id, &db_id, &target_branch_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

// Rebase handlers
#[derive(Debug, Deserialize)]
pub struct RebaseRequest {
    pub target_branch_id: String,
    pub author: Option<String>,
    pub force: Option<bool>,
}

pub async fn rebase_database_branch<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, feature_branch_id)): Path<(Id, Id)>,
    RequestJson(request): RequestJson<RebaseRequest>,
) -> Result<Json<RebaseResult>, (StatusCode, Json<ErrorResponse>)> {
    // If no target specified, use main branch
    let target_branch_id = if request.target_branch_id.is_empty() {
        match get_main_branch_name(&*store, &db_id).await {
            Ok(branch_id) => branch_id,
            Err(error_response) => return Err(error_response),
        }
    } else {
        request.target_branch_id
    };

    match BranchOperations::rebase_branch(
        &*store,
        &db_id,
        &feature_branch_id,
        &db_id,
        &target_branch_id,
        request.author,
        request.force.unwrap_or(false),
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn rebase_branch<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, feature_branch_id, target_branch_id)): Path<(Id, Id, Id)>,
    RequestJson(request): RequestJson<RebaseRequest>,
) -> Result<Json<RebaseResult>, (StatusCode, Json<ErrorResponse>)> {
    // Verify database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Use provided target branch instead of request target
    match BranchOperations::rebase_branch(
        &*store,
        &db_id,
        &feature_branch_id,
        &db_id,
        &target_branch_id,
        request.author,
        request.force.unwrap_or(false),
    )
    .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

// Rebase Validation handlers
pub async fn validate_database_rebase<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, feature_branch_id)): Path<(Id, Id)>,
) -> Result<Json<RebaseValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Get the main branch as target
    let target_branch_id = match get_main_branch_name(&*store, &db_id).await {
        Ok(branch_id) => branch_id,
        Err(error_response) => return Err(error_response),
    };

    match BranchOperations::check_rebase_validation(&*store, &db_id, &feature_branch_id, &db_id, &target_branch_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn validate_branch_rebase<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, feature_branch_id, target_branch_id)): Path<(Id, Id, Id)>,
) -> Result<Json<RebaseValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Verify database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => (),
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    match BranchOperations::check_rebase_validation(&*store, &db_id, &feature_branch_id, &db_id, &target_branch_id)
        .await
    {
        Ok(result) => Ok(Json(result)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

// Database handlers
pub async fn list_databases<S: Store>(
    State(store): State<AppState<S>>,
) -> Result<Json<ListResponse<Database>>, (StatusCode, Json<ErrorResponse>)> {
    match store.list_databases().await {
        Ok(databases) => {
            let total = databases.len();
            Ok(Json(ListResponse {
                items: databases,
                total,
            }))
        }
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn get_database<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
) -> Result<Json<Database>, (StatusCode, Json<ErrorResponse>)> {
    match store.get_database(&db_id).await {
        Ok(Some(database)) => Ok(Json(database)),
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Database not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn upsert_database<S: Store>(
    State(store): State<AppState<S>>,
    RequestJson(new_database): RequestJson<NewDatabase>,
) -> Result<Json<Database>, (StatusCode, Json<ErrorResponse>)> {
    let mut database = new_database.into_database();
    
    // Create the main branch for this database
    let main_branch = Branch::new_main_branch(database.id.clone(), Some("System".to_string()));
    
    // Store the database first
    match store.upsert_database(database.clone()).await {
        Ok(()) => {},
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to create database: {}", e))),
        )),
    }
    
    // Store the main branch
    match store.upsert_branch(main_branch.clone()).await {
        Ok(()) => {},
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to create main branch: {}", e))),
        )),
    }
    
    // Update the database to reference the main branch
    database.default_branch_name = main_branch.name.clone();
    match store.upsert_database(database.clone()).await {
        Ok(()) => Ok(Json(database)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update database with main branch: {}", e))),
        )),
    }
}

pub async fn delete_database<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Check if database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => {
            // Database exists, continue with validation
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to check database: {}", e))),
            ))
        }
    }

    // Check for active branches (besides main)
    match store.list_branches_for_database(&db_id).await {
        Ok(branches) => {
            // Check if there are branches other than main
            if branches.len() > 1 {
                return Err((
                    StatusCode::CONFLICT,
                    Json(ErrorResponse::new("Cannot delete database: contains active branches besides main. Delete all feature branches first.")),
                ));
            }
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to check branches: {}", e))),
            ))
        }
    }

    // Check for existing commits
    match store.list_commits_for_database(&db_id, None).await {
        Ok(commits) => {
            if !commits.is_empty() {
                return Err((
                    StatusCode::CONFLICT,
                    Json(ErrorResponse::new("Cannot delete database: contains commit history. This operation would cause data loss.")),
                ));
            }
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to check commits: {}", e))),
            ))
        }
    }

    // Check for active working commits
    if let Ok(branches) = store.list_branches_for_database(&db_id).await {
        for branch in branches {
            match store.get_active_working_commit_for_branch(&db_id, &branch.name).await {
                Ok(Some(_)) => {
                    return Err((
                        StatusCode::CONFLICT,
                        Json(ErrorResponse::new(&format!("Cannot delete database: has active working commit on branch '{}'. Commit or abandon working changes first.", branch.name))),
                    ));
                }
                Ok(None) => {
                    // No active working commit for this branch, continue
                }
                Err(e) => {
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(&format!("Failed to check working commits: {}", e))),
                    ))
                }
            }
        }
    }

    // All validations passed, proceed with deletion
    match store.delete_database(&db_id).await {
        Ok(true) => {
            Ok(Json(serde_json::json!({
                "message": "Database deleted successfully",
                "deleted_database_id": db_id
            })))
        }
        Ok(false) => {
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to delete database: {}", e))),
            ))
        }
    }
}

// Branch handlers
pub async fn list_branches<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
) -> Result<Json<ListResponse<Version>>, (StatusCode, Json<ErrorResponse>)> {
    match store.list_versions_for_database(&db_id).await {
        Ok(versions) => {
            let total = versions.len();
            Ok(Json(ListResponse {
                items: versions,
                total,
            }))
        }
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn get_branch<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id)): Path<(Id, Id)>,
) -> Result<Json<Version>, (StatusCode, Json<ErrorResponse>)> {
    let branch_id = version_id;
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err((status, response)) => return Err((status, response)),
    };
    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(version)) => {
            if version.database_id != db_id {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Version not found in this database")),
                ));
            }
            Ok(Json(version))
        }
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Version not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn upsert_branch<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
    RequestJson(mut version): RequestJson<Version>,
) -> Result<Json<Version>, (StatusCode, Json<ErrorResponse>)> {
    version.database_id = db_id;
    match store.upsert_version(version.clone()).await {
        Ok(()) => Ok(Json(version)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

// Schema handlers
pub async fn get_schema<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id)): Path<(Id, Id)>,
) -> Result<Json<Schema>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &version_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    match store.get_schema(&db_id, &branch_name).await {
        Ok(Some(schema)) => Ok(Json(schema)),
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Schema not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn upsert_schema<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id)): Path<(Id, Id)>,
    RequestJson(mut schema): RequestJson<Schema>,
) -> Result<Json<Schema>, (StatusCode, Json<ErrorResponse>)> {
    let branch_id = version_id;
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err((status, response)) => return Err((status, response)),
    };
    // Verify branch belongs to database
    if let Ok(Some(version)) = store.get_branch(&db_id, &branch_name).await {
        if version.database_id != db_id {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Version not found in this database")),
            ));
        }
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Version not found")),
        ));
    }

    // In the new commit-based architecture, schema updates must be done through working commits
    // This endpoint should be deprecated in favor of working commit operations
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Schema updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ))
}

// Instance handlers
pub async fn list_instances<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id)): Path<(Id, Id)>,
    Query(query): Query<InstanceQuery>,
) -> Result<Json<ListResponse<ExpandedInstance>>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &version_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    let filter = query.class_id.map(|class_id| InstanceFilter {
        types: Some(vec![class_id]),
        where_clause: None,
        sort: None,
        limit: None,
    });

    match store.list_instances_for_branch(&db_id, &branch_name, filter).await {
        Ok(instances) => {
            let expand_rels = query
                .expand
                .as_ref()
                .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
                .unwrap_or_default();

            let depth = query.depth.unwrap_or(0);
            let mut expanded_instances = Vec::new();

            for instance in instances {
                match Expander::expand_instance_with_branch(&*store, &instance, &expand_rels, depth, &db_id, Some(&branch_name)).await {
                    Ok(expanded) => expanded_instances.push(expanded),
                    Err(e) => {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse::new(&e.to_string())),
                        ))
                    }
                }
            }

            let total = expanded_instances.len();
            Ok(Json(ListResponse {
                items: expanded_instances,
                total,
            }))
        }
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn get_instance<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id, id)): Path<(Id, Id, Id)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Json<InstanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &version_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    match store.get_instance(&db_id, &branch_name, &id).await {
        Ok(Some(instance)) => {
            // Always expand by default
            let should_expand = true;

            if should_expand {
                let expand_rels = query
                    .expand
                    .as_ref()
                    .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
                    .unwrap_or_default();

                let depth = query.depth.unwrap_or(0);

                match Expander::expand_instance_with_branch(&*store, &instance, &expand_rels, depth, &db_id, Some(&branch_name)).await {
                    Ok(expanded) => Ok(Json(InstanceResponse::Expanded(expanded))),
                    Err(e) => return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(&e.to_string())),
                    )),
                }
            } else {
                // Return raw instance without expansion
                Ok(Json(InstanceResponse::Raw(instance)))
            }
        }
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn upsert_instance<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id)): Path<(Id, Id)>,
    user_context: UserContext,
    RequestJson(mut instance): RequestJson<Instance>,
) -> Result<Json<Instance>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &version_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    // Verify branch belongs to database
    if let Ok(Some(branch)) = store.get_branch(&db_id, &branch_name).await {
        if branch.database_id != db_id {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found in this database")),
            ));
        }
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Branch not found")),
        ));
    }

    // Enhanced workflow: Automatically handle working commits for instance upsert
    // Get or create a working commit for this branch
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Validate the instance against the working commit's schema
    if let Err(e) = SimpleValidator::validate_instance_basic(&*store, &instance, &working_commit.schema_data).await
    {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(&e.to_string())),
        ));
    }

    // Update audit trail based on whether this is create or update
    let existing_index = working_commit
        .instances_data
        .iter()
        .position(|i| i.id == instance.id);

    match existing_index {
        Some(index) => {
            // Update existing instance - preserve created_by/created_at
            let existing_instance = &working_commit.instances_data[index];
            instance.created_by = existing_instance.created_by.clone();
            instance.created_at = existing_instance.created_at;
            instance.updated_by = user_context.user_id.clone();
            instance.updated_at = chrono::Utc::now();
            working_commit.instances_data[index] = instance.clone();
        }
        None => {
            // Add new instance - set all audit fields
            let now = chrono::Utc::now();
            instance.created_by = user_context.user_id.clone();
            instance.created_at = now;
            instance.updated_by = user_context.user_id.clone();
            instance.updated_at = now;
            working_commit.instances_data.push(instance.clone());
        }
    }
    
    // Update the working commit timestamp
    working_commit.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated working commit back to the store
    if let Err(e) = store.update_working_commit(working_commit).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
        ));
    }

    // Return the successfully upserted instance
    Ok(Json(instance))
}

pub async fn update_instance<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, version_id, id)): Path<(Id, Id, Id)>,
    user_context: UserContext,
    RequestJson(updates): RequestJson<HashMap<String, serde_json::Value>>,
) -> Result<Json<Instance>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &version_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    // Get or create a working commit for this branch
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Find the existing instance in the working commit
    let mut instance = working_commit
        .instances_data
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ))?;

    // Apply the updates to the instance
    for (key, value) in updates {
        match key.as_str() {
            "properties" => {
                if let Ok(props) = serde_json::from_value::<HashMap<String, PropertyValue>>(value) {
                    // PATCH semantics: merge new properties with existing ones
                    for (prop_key, prop_value) in props {
                        instance.properties.insert(prop_key, prop_value);
                    }
                }
            }
            "relationships" => {
                if let Ok(rels) = serde_json::from_value::<HashMap<String, RelationshipSelection>>(value) {
                    // PATCH semantics: merge new relationships with existing ones\n                    for (rel_key, rel_value) in rels {\n                        instance.relationships.insert(rel_key, rel_value);\n                    }
                }
            }
            "class" => {
                if let Ok(class_id) = serde_json::from_value::<String>(value) {
                    instance.class_id = class_id;
                }
            }
            "domain" => {
                if let Ok(domain) = serde_json::from_value::<Domain>(value) {
                    instance.domain = Some(domain);
                }
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Update audit trail
    instance.updated_by = user_context.user_id.clone();
    instance.updated_at = chrono::Utc::now();

    // Note: Validation removed from PATCH operations - use explicit /validate endpoints

    // Update the instance in the working commit
    if let Some(existing) = working_commit.instances_data.iter_mut().find(|i| i.id == id) {
        *existing = instance.clone();
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ));
    }

    // Update the working commit timestamp
    working_commit.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated working commit back to the store
    if let Err(e) = store.update_working_commit(working_commit).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
        ));
    }

    // Return the successfully updated instance
    Ok(Json(instance))
}


// Database-level schema handler (uses main branch)
pub async fn get_database_schema<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
) -> Result<Json<Schema>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    match store.get_schema(&db_id, &main_branch_name).await {
        Ok(Some(schema)) => Ok(Json(schema)),
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Schema not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn upsert_database_schema<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
    RequestJson(mut schema): RequestJson<Schema>,
) -> Result<Json<Schema>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    // In the new commit-based architecture, schema updates must be done through working commits
    // This endpoint should be deprecated in favor of working commit operations
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Schema updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ))
}

// Database-level instance handlers (uses main branch)
pub async fn list_database_instances<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
    Query(query): Query<InstanceQuery>,
) -> Result<Json<ListResponse<InstanceResponse>>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    let filter = query.class_id.map(|class_id| InstanceFilter {
        types: Some(vec![class_id]),
        where_clause: None,
        sort: None,
        limit: None,
    });

    match store
        .list_instances_for_branch(&db_id, &main_branch_name, filter)
        .await
    {
        Ok(instances) => {
            // Always expand by default
            let should_expand = true;
            
            let mut instance_responses = Vec::new();

            if should_expand {
                let expand_rels = query
                    .expand
                    .as_ref()
                    .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
                    .unwrap_or_default();

                let depth = query.depth.unwrap_or(0);

                for instance in instances {
                    match Expander::expand_instance_with_branch(&*store, &instance, &expand_rels, depth, &db_id, Some(&main_branch_name)).await {
                        Ok(expanded) => instance_responses.push(InstanceResponse::Expanded(expanded)),
                        Err(e) => {
                            return Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse::new(&e.to_string())),
                            ))
                        }
                    }
                }
            } else {
                // Return raw instances by default (consistent with get_database_instance)
                for instance in instances {
                    instance_responses.push(InstanceResponse::Raw(instance));
                }
            }

            let total = instance_responses.len();
            Ok(Json(ListResponse {
                items: instance_responses,
                total,
            }))
        }
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn get_database_instance<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, id)): Path<(Id, Id)>,
    Query(query): Query<ExpandQuery>,
) -> Result<Json<InstanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    match store.get_instance(&db_id, &main_branch_name, &id).await {
        Ok(Some(instance)) => {
            // Always expand by default
            let should_expand = true;

            if should_expand {
                let expand_rels = query
                    .expand
                    .as_ref()
                    .map(|s| s.split(',').map(|s| s.to_string()).collect::<Vec<_>>())
                    .unwrap_or_default();

                let depth = query.depth.unwrap_or(0);

                match Expander::expand_instance_with_branch(&*store, &instance, &expand_rels, depth, &db_id, Some(&main_branch_name)).await {
                    Ok(expanded) => Ok(Json(InstanceResponse::Expanded(expanded))),
                    Err(e) => return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(&e.to_string())),
                    )),
                }
            } else {
                // Return raw instance without expansion
                Ok(Json(InstanceResponse::Raw(instance)))
            }
        }
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn upsert_database_instance<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
    RequestJson(mut instance): RequestJson<Instance>,
) -> Result<Json<Instance>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    // branch_id is no longer a field in Instance - removed in commit-based architecture

    if let Ok(Some(schema)) = store.get_schema(&db_id, &main_branch_name).await {
        if let Err(e) = SimpleValidator::validate_instance_basic(&*store, &instance, &schema).await
        {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(&e.to_string())),
            ));
        }
    }

    // Instance updates must be done through working commits in the new architecture
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Instance updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ))
}

pub async fn update_database_instance<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, id)): Path<(Id, Id)>,
    user_context: UserContext,
    RequestJson(updates): RequestJson<HashMap<String, serde_json::Value>>,
) -> Result<Json<Instance>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    // Get or create a working commit for the main branch
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &main_branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Find the existing instance in the working commit
    let mut instance = working_commit
        .instances_data
        .iter()
        .find(|i| i.id == id)
        .cloned()
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ))?;

    // Apply the updates to the instance
    for (key, value) in updates {
        match key.as_str() {
            "properties" => {
                if let Ok(props) = serde_json::from_value::<HashMap<String, PropertyValue>>(value) {
                    // PATCH semantics: merge new properties with existing ones
                    for (prop_key, prop_value) in props {
                        instance.properties.insert(prop_key, prop_value);
                    }
                }
            }
            "relationships" => {
                if let Ok(rels) = serde_json::from_value::<HashMap<String, RelationshipSelection>>(value) {
                    // PATCH semantics: merge new relationships with existing ones\n                    for (rel_key, rel_value) in rels {\n                        instance.relationships.insert(rel_key, rel_value);\n                    }
                }
            }
            "class" => {
                if let Ok(class_id) = serde_json::from_value::<String>(value) {
                    instance.class_id = class_id;
                }
            }
            "domain" => {
                if let Ok(domain) = serde_json::from_value::<Domain>(value) {
                    instance.domain = Some(domain);
                }
            }
            _ => {
                // Ignore unknown fields
            }
        }
    }

    // Update audit trail
    instance.updated_by = user_context.user_id.clone();
    instance.updated_at = chrono::Utc::now();

    // Note: Validation removed from PATCH operations - use explicit /validate endpoints

    // Update the instance in the working commit
    if let Some(existing) = working_commit.instances_data.iter_mut().find(|i| i.id == id) {
        *existing = instance.clone();
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ));
    }

    // Update the working commit timestamp
    working_commit.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated working commit back to the store
    if let Err(e) = store.update_working_commit(working_commit).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
        ));
    }

    // Return the successfully updated instance
    Ok(Json(instance))
}


// Individual class handlers (branch-specific)
pub async fn get_class<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_id, class_id)): Path<(Id, Id, Id)>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    match store.get_class(&db_id, &branch_name, &class_id).await {
        Ok(Some(class)) => Ok(Json(class)),
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Class not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn add_class<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
    user_context: crate::model::UserContext,
    RequestJson(new_class): RequestJson<NewClassDef>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    if let Ok(Some(branch)) = store.get_branch(&db_id, &branch_name).await {
        if branch.database_id != db_id {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found in this database")),
            ));
        }
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Branch not found")),
        ));
    }

    let class = ClassDef::from_new(new_class, user_context.user_id.clone());

    // Enhanced workflow: Automatically handle working commits for new class creation
    // Get or create a working commit for this branch
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Check if class already exists in the working commit schema
    if working_commit.schema_data.classes.iter().any(|c| c.id == class.id) {
        return Err((
            StatusCode::CONFLICT,
            Json(ErrorResponse::new(&format!("Class '{}' already exists", class.id))),
        ));
    }

    // Validate that all relationship targets reference existing class IDs
    for relationship in &class.relationships {
        for target_class_id in &relationship.targets {
            if working_commit.schema_data.get_class_by_id(target_class_id).is_none() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(&format!(
                        "Relationship '{}' references non-existent class ID '{}'. Available classes: {}",
                        relationship.name,
                        target_class_id,
                        working_commit.schema_data.classes.iter()
                            .map(|c| c.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))),
                ));
            }
        }
    }

    // Add the new class to the working commit's schema
    working_commit.schema_data.classes.push(class.clone());
    
    // Update the working commit timestamp
    working_commit.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated working commit back to the store
    if let Err(e) = store.update_working_commit(working_commit).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
        ));
    }

    // Return the successfully added class
    // Add a note in the response headers to inform the user about the working commit workflow
    Ok(Json(class))
}

pub async fn update_class<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    user_context: UserContext,
    Path((db_id, branch_id, class_id)): Path<(Id, Id, Id)>,
    RequestJson(update): RequestJson<ClassDefUpdate>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    // Get or create a working commit for this branch (automatic working commit management)
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Find the existing class in the working commit's schema
    let class_index = working_commit.schema_data.classes
        .iter()
        .position(|c| c.id == class_id);

    let existing_class = match class_index {
        Some(index) => working_commit.schema_data.classes[index].clone(),
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Class not found in working commit")),
            ))
        }
    };

    // Apply partial updates using the apply_update method
    let mut updated_class = existing_class.clone();
    updated_class.apply_update(update, user_context.user_id.clone());

    // Validate that all relationship targets reference existing class IDs
    for relationship in &updated_class.relationships {
        for target_class_id in &relationship.targets {
            // Skip validation for the class being updated itself, as it exists in the schema
            if target_class_id != &class_id && working_commit.schema_data.get_class_by_id(target_class_id).is_none() {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(&format!(
                        "Relationship '{}' references non-existent class ID '{}'. Available classes: {}",
                        relationship.name,
                        target_class_id,
                        working_commit.schema_data.classes.iter()
                            .map(|c| c.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ))),
                ));
            }
        }
    }

    // Update the class in the working commit's schema
    if let Some(index) = class_index {
        working_commit.schema_data.classes[index] = updated_class.clone();
        working_commit.updated_at = chrono::Utc::now().to_rfc3339();

        // Save the updated working commit back to the store
        if let Err(e) = store.update_working_commit(working_commit).await {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
            ));
        }

        Ok(Json(updated_class))
    } else {
        Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new("Failed to find class in working commit after verification")),
        ))
    }
}

pub async fn delete_class<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name, class_id)): Path<(Id, String, Id)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    if let Ok(Some(branch)) = store.get_branch(&db_id, &branch_name).await {
        if branch.database_id != db_id {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found in this database")),
            ));
        }
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Branch not found")),
        ));
    }

    // Class schema updates must be done through working commits in the new architecture
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Class schema updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ));
}

// Database-level class handlers (auto-select main branch)
pub async fn get_database_class<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, class_id)): Path<(Id, Id)>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    match store.get_class(&db_id, &main_branch_name, &class_id).await {
        Ok(Some(class)) => Ok(Json(class)),
        Ok(None) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Class not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

pub async fn add_database_class<S: Store>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
    user_context: crate::model::UserContext,
    RequestJson(new_class): RequestJson<NewClassDef>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    let _class = ClassDef::from_new(new_class, user_context.user_id.clone());

    // Class schema updates must be done through working commits in the new architecture
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Class schema updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ));
}

pub async fn update_database_class<S: Store>(
    State(store): State<AppState<S>>,
    user_context: UserContext,
    Path((db_id, class_id)): Path<(Id, Id)>,
    RequestJson(update): RequestJson<ClassDefUpdate>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    // Get existing class
    let existing_class = match store.get_class(&db_id, &main_branch_name, &class_id).await {
        Ok(Some(class)) => class,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Class not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Apply partial updates using the apply_update method
    let mut updated_class = existing_class.clone();
    updated_class.apply_update(update, user_context.user_id.clone());

    // Class schema updates must be done through working commits in the new architecture
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Class schema updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ));
}

pub async fn delete_database_class<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, class_id)): Path<(Id, Id)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    // Class schema updates must be done through working commits in the new architecture
    return Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            "Class schema updates must be done through working commits. Use the working commit endpoints instead."
        )),
    ));
}

// Individual instance delete handlers
pub async fn delete_instance<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_id, id)): Path<(Id, Id, Id)>,
    user_context: UserContext,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };

    // Get or create a working commit for this branch
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Find the instance in the working commit
    let instance_index = working_commit
        .instances_data
        .iter()
        .position(|i| i.id == id)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ))?;

    // Remove the instance from the working commit
    let deleted_instance = working_commit.instances_data.remove(instance_index);

    // Update the working commit timestamp
    working_commit.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated working commit back to the store
    if let Err(e) = store.update_working_commit(working_commit).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
        ));
    }

    // Return success response with deleted instance info
    Ok(Json(serde_json::json!({
        "message": "Instance deleted successfully",
        "deleted_instance_id": deleted_instance.id,
        "deleted_instance_class": deleted_instance.class_id
    })))
}

pub async fn delete_database_instance<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, id)): Path<(Id, Id)>,
    user_context: UserContext,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    let main_branch_name = get_main_branch_name(&*store, &db_id).await?;

    // Get or create a working commit for the main branch
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &main_branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Find the instance in the working commit
    let instance_index = working_commit
        .instances_data
        .iter()
        .position(|i| i.id == id)
        .ok_or_else(|| (
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ))?;

    // Remove the instance from the working commit
    let deleted_instance = working_commit.instances_data.remove(instance_index);

    // Update the working commit timestamp
    working_commit.updated_at = chrono::Utc::now().to_rfc3339();

    // Save the updated working commit back to the store
    if let Err(e) = store.update_working_commit(working_commit).await {
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Failed to update working commit: {}", e))),
        ));
    }

    // Return success response with deleted instance info
    Ok(Json(serde_json::json!({
        "message": "Instance deleted successfully",
        "deleted_instance_id": deleted_instance.id,
        "deleted_instance_class": deleted_instance.class_id
    })))
}

// Backward compatibility aliases for version-based naming
pub use get_branch as get_version;
pub use list_branches as list_versions;
pub use upsert_branch as upsert_version;

// =====================================
// Solve and Artifacts API
// =====================================

/// Create and execute a solve operation to generate a configuration artifact
pub async fn solve_configuration<S: Store>(
    State(store): State<AppState<S>>,
    RequestJson(request): RequestJson<NewConfigurationArtifact>,
) -> Result<Json<ConfigurationArtifact>, (StatusCode, Json<ErrorResponse>)> {
    use crate::logic::SolvePipeline;
    
    // Create solve pipeline with a reference to the store
    let pipeline = SolvePipeline::new(store.as_ref());
    
    // Execute solve
    match pipeline.solve(request).await {
        Ok(artifact) => Ok(Json(artifact)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Solve failed: {}", e))),
        )),
    }
}

/// Query/solve configuration for a specific instance on main branch
pub async fn query_instance_configuration<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, instance_id)): Path<(Id, Id)>,
    RequestJson(request): RequestJson<InstanceQueryRequest>,
) -> Result<Json<ConfigurationArtifact>, (StatusCode, Json<ErrorResponse>)> {
    // Get main branch for this database
    let main_branch_name = match get_main_branch_name(&*store, &db_id).await {
        Ok(branch_id) => branch_id,
        Err((status, error)) => return Err((status, error)),
    };
    
    query_instance_configuration_impl(&*store, db_id, main_branch_name, instance_id, request).await
}

/// Query/solve configuration for a specific instance on a specific branch
pub async fn query_branch_instance_configuration<S: Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_id, instance_id)): Path<(Id, Id, Id)>,
    RequestJson(request): RequestJson<InstanceQueryRequest>,
) -> Result<Json<ConfigurationArtifact>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err(error_response) => return Err(error_response),
    };
    
    query_instance_configuration_impl(&*store, db_id, branch_name, instance_id, request).await
}

/// Implementation for instance-specific configuration queries
async fn query_instance_configuration_impl<S: Store>(
    store: &S,
    database_id: Id,
    branch_name: String,
    instance_id: Id,
    request: InstanceQueryRequest,
) -> Result<Json<ConfigurationArtifact>, (StatusCode, Json<ErrorResponse>)> {
    use crate::logic::SolvePipeline;
    use crate::model::{ResolutionContext, NewConfigurationArtifact};
    
    // Verify instance exists and belongs to the branch
    match store.get_instance(&database_id, &branch_name, &instance_id).await {
        Ok(Some(_instance)) => {
            // Instance exists in the branch
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Instance not found")),
            ));
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ));
        }
    }
    
    // Build ResolutionContext from path parameters and request
    let resolution_context = ResolutionContext {
        database_id,
        branch_id: branch_name.clone(), // Temporarily use branch_name as branch_id
        commit_hash: request.commit_hash,
        policies: request.policies,
        metadata: request.context_metadata,
    };
    
    // Create NewConfigurationArtifact for the solve pipeline
    let solve_request = NewConfigurationArtifact {
        resolution_context,
        user_metadata: request.user_metadata,
    };
    
    // Create solve pipeline and execute with optional derived properties
    let pipeline = SolvePipeline::new(store);
    match pipeline.solve_instance_with_derived(solve_request, instance_id, request.derived_properties).await {
        Ok(artifact) => Ok(Json(artifact)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&format!("Instance solve failed: {}", e))),
        )),
    }
}

/// List all configuration artifacts
pub async fn list_artifacts<S: Store>(
    State(_store): State<AppState<S>>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<ListResponse<ConfigurationArtifact>>, (StatusCode, Json<ErrorResponse>)> {
    let _database_id = query.get("database_id");
    let _branch_id = query.get("branch_id");
    
    // For now, return empty list as artifacts aren't persisted in current implementation
    // In a real implementation, this would query the artifact store
    let artifacts = Vec::new(); // TODO: Implement artifact persistence
    
    Ok(Json(ListResponse {
        total: artifacts.len(),
        items: artifacts,
    }))
}

/// Get a specific configuration artifact by ID
pub async fn get_artifact<S: Store>(
    State(_store): State<AppState<S>>,
    Path(_artifact_id): Path<Id>,
) -> Result<Json<ConfigurationArtifact>, (StatusCode, Json<ErrorResponse>)> {
    // For now, return not found as artifacts aren't persisted in current implementation
    // TODO: Implement artifact persistence
    return Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::new("Artifact not found - persistence not yet implemented")),
    ))
}

/// Get the solve summary for a configuration artifact
pub async fn get_artifact_summary<S: Store>(
    State(_store): State<AppState<S>>,
    Path(_artifact_id): Path<Id>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // For now, return not found as artifacts aren't persisted in current implementation
    // TODO: Implement artifact persistence
    return Err((
        StatusCode::NOT_FOUND,
        Json(ErrorResponse::new("Artifact not found - persistence not yet implemented")),
    ))
}

// ========== Working Commit Handlers ==========

/// Create a working commit for a branch (starts staging area)
pub async fn create_working_commit<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
    RequestJson(request): RequestJson<NewWorkingCommit>,
) -> Result<Json<WorkingCommit>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(version)) => {
            if version.database_id != db_id {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Branch not found in this database")),
                ));
            }
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    // Check if there's already an active working commit for this branch
    match store.get_active_working_commit_for_branch(&db_id, &branch_name).await {
        Ok(Some(_)) => {
            return Err((
                StatusCode::CONFLICT,
                Json(ErrorResponse::new("Branch already has an active working commit. Commit or abandon the existing one first.")),
            ))
        }
        Ok(None) => {}, // Good, no active working commit
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    // Create the working commit using path parameters
    let new_working_commit = crate::model::NewWorkingCommit {
        author: request.author,
    };

    match store.create_working_commit(&db_id, &branch_name, new_working_commit).await {
        Ok(working_commit) => Ok(Json(working_commit)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// Get the active working commit for a branch
/// Helper function to automatically get or create a working commit for a branch
async fn get_or_create_working_commit<S: WorkingCommitStore + Store>(
    store: &S,
    db_id: &Id,
    branch_name: &str,
) -> anyhow::Result<WorkingCommit> {
    // Try to get existing working commit
    match store.get_active_working_commit_for_branch(db_id, branch_name).await? {
        Some(working_commit) => Ok(working_commit),
        None => {
            // No working commit exists, create one automatically
            let new_working_commit = NewWorkingCommit {
                author: Some("system".to_string()), // System-created working commits
            };

            let working_commit = store.create_working_commit(db_id, branch_name, new_working_commit).await?;
            Ok(working_commit)
        }
    }
}

/// Helper function to create a RelationshipSelection from a class relationship definition's default pool
fn create_default_pool_selection(rel_def: &crate::model::RelationshipDef) -> RelationshipSelection {
    use crate::model::{DefaultPool, RelationshipSelection, InstanceFilter};
    
    match &rel_def.default_pool {
        DefaultPool::All => {
            // All instances of the target types
            RelationshipSelection::PoolBased {
                pool: Some(InstanceFilter {
                    types: Some(rel_def.targets.clone()),
                    where_clause: None,
                    sort: None,
                    limit: None,
                }),
                selection: None,
            }
        }
        DefaultPool::None => {
            // This shouldn't be called for None, but return empty IDs as fallback
            RelationshipSelection::SimpleIds(vec![])
        }
        DefaultPool::Filter { types, filter } => {
            // Use the specific filter from the default pool
            let instance_filter = if let Some(ref inner_filter) = filter {
                // Use the inner filter from the DefaultPool
                inner_filter.clone()
            } else {
                // Create a basic type-only filter
                InstanceFilter {
                    types: types.clone(),
                    where_clause: None,
                    sort: None,
                    limit: None,
                }
            };
            
            RelationshipSelection::PoolBased {
                pool: Some(instance_filter),
                selection: None,
            }
        }
    }
}

/// Helper function to enhance working commit response with resolved relationships
async fn enhance_working_commit_response_with_resolved_relationships<S: Store>(
    store: &S,
    db_id: &Id,
    branch_name: &str,
    working_commit: &WorkingCommit,
    mut response: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    // Check if this is a changes-only response (has instance_changes field)
    if let Some(instance_changes) = response.get_mut("instance_changes") {
        // Handle changes-only view
        for change_type in ["added", "modified", "deleted"] {
            if let Some(instances) = instance_changes.get_mut(change_type) {
                if let Some(instances_array) = instances.as_array_mut() {
                    for instance_value in instances_array {
                        if let Some(instance_obj) = instance_value.as_object_mut() {
                            if let Some(relationships) = instance_obj.get("relationships").cloned() {
                                let mut enhanced_rels = serde_json::Map::new();
                                
                                if let Some(rels_obj) = relationships.as_object() {
                                    for (rel_name, original_selection_value) in rels_obj {
                                        // Parse the original selection
                                        if let Ok(original_selection) = serde_json::from_value::<RelationshipSelection>(original_selection_value.clone()) {
                                            // Resolve the relationship using working commit context
                                            match resolve_selection_with_working_commit_context(
                                                store,
                                                &original_selection,
                                                db_id,
                                                branch_name,
                                                working_commit,
                                            ).await {
                                                Ok(resolved_rel) => {
                                                    let enhanced_rel = serde_json::json!({
                                                        "original": original_selection,
                                                        "resolved": {
                                                            "materialized_ids": resolved_rel.materialized_ids,
                                                            "resolution_method": resolved_rel.resolution_method,
                                                            "resolution_details": resolved_rel.resolution_details
                                                        }
                                                    });
                                                    enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                }
                                                Err(_) => {
                                                    // If resolution fails, just show the original
                                                    let enhanced_rel = serde_json::json!({
                                                        "original": original_selection,
                                                        "resolved": null
                                                    });
                                                    enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                instance_obj.insert("relationships".to_string(), serde_json::Value::Object(enhanced_rels));
                            }
                        }
                    }
                }
            }
        }
    }
    
    // Handle full working commit view (has instances_data field)
    if let Some(instances_data) = response.get_mut("instances_data") {
        if let Some(instances_array) = instances_data.as_array_mut() {
            for instance_value in instances_array {
                if let Some(instance_obj) = instance_value.as_object_mut() {
                    if let Some(relationships) = instance_obj.get("relationships").cloned() {
                        let mut enhanced_rels = serde_json::Map::new();
                        
                        if let Some(rels_obj) = relationships.as_object() {
                            for (rel_name, original_selection_value) in rels_obj {
                                // Parse the original selection
                                if let Ok(original_selection) = serde_json::from_value::<RelationshipSelection>(original_selection_value.clone()) {
                                    // Resolve the relationship using working commit context
                                    match resolve_selection_with_working_commit_context(
                                        store,
                                        &original_selection,
                                        db_id,
                                        branch_name,
                                        working_commit,
                                    ).await {
                                        Ok(resolved_rel) => {
                                            let enhanced_rel = serde_json::json!({
                                                "original": original_selection,
                                                "resolved": {
                                                    "materialized_ids": resolved_rel.materialized_ids,
                                                    "resolution_method": resolved_rel.resolution_method,
                                                    "resolution_details": resolved_rel.resolution_details
                                                }
                                            });
                                            enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                        }
                                        Err(_) => {
                                            // If resolution fails, just show the original
                                            let enhanced_rel = serde_json::json!({
                                                "original": original_selection,
                                                "resolved": null
                                            });
                                            enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                        }
                                    }
                                }
                            }
                        }
                        
                        // Also check class schema for relationships with default pools that aren't explicitly configured
                        if let Some(instance_class) = instance_obj.get("class") {
                            if let Some(class_id_str) = instance_class.as_str() {
                                // Get the class definition from the working commit schema
                                if let Some(class_def) = working_commit.schema_data.classes.iter().find(|c| c.id == class_id_str) {
                                    for rel_def in &class_def.relationships {
                                        let rel_name = &rel_def.id;
                                        
                                        // Only process if this relationship isn't already in enhanced_rels (i.e., not explicitly configured on instance)
                                        if !enhanced_rels.contains_key(rel_name) {
                                            // Check if this relationship has a default pool
                                            if rel_def.default_pool != crate::model::DefaultPool::None {
                                                // Create a pool-based relationship selection using the default pool
                                                let default_selection = create_default_pool_selection(rel_def);
                                                
                                                // Resolve the default pool relationship
                                                match resolve_selection_with_working_commit_context(
                                                    store,
                                                    &default_selection,
                                                    db_id,
                                                    branch_name,
                                                    working_commit,
                                                ).await {
                                                    Ok(resolved_rel) => {
                                                        let enhanced_rel = serde_json::json!({
                                                            "original": default_selection,
                                                            "resolved": {
                                                                "materialized_ids": resolved_rel.materialized_ids,
                                                                "resolution_method": resolved_rel.resolution_method,
                                                                "resolution_details": resolved_rel.resolution_details
                                                            }
                                                        });
                                                        enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                    }
                                                    Err(_) => {
                                                        // If resolution fails, show the default selection
                                                        let enhanced_rel = serde_json::json!({
                                                            "original": default_selection,
                                                            "resolved": null
                                                        });
                                                        enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        instance_obj.insert("relationships".to_string(), serde_json::Value::Object(enhanced_rels));
                    }
                }
            }
        }
    }

    Ok(response)
}

pub async fn get_active_working_commit_raw<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
    Query(query): Query<WorkingCommitQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(version)) => {
            if version.database_id != db_id {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Branch not found in this database")),
                ));
            }
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    let working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    if query.changes_only.unwrap_or(false) {
        // Return changes-only view (raw, no resolved relationships)
        match working_commit.to_changes(&*store).await {
            Ok(changes) => Ok(Json(serde_json::to_value(changes).unwrap())),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to compute changes: {}", e))),
            ))
        }
    } else {
        // Return full working commit (raw, no resolved relationships)
        Ok(Json(serde_json::to_value(working_commit).unwrap()))
    }
}

#[derive(Debug, Deserialize)]
pub struct CommitRequest {
    pub message: String,
    pub author: Option<String>,
}

/// Commit staged changes (convert working commit to permanent commit)
pub async fn commit_working_changes<S: WorkingCommitStore + CommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
    RequestJson(request): RequestJson<CommitRequest>,
) -> Result<Json<CommitResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(version)) => {
            if version.database_id != db_id {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Branch not found in this database")),
                ));
            }
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    // Get or create the working commit
    let working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Create the commit
    let new_commit = NewCommit {
        database_id: db_id.clone(),
        working_commit_id: working_commit.id.clone(),
        message: request.message,
        author: request.author.or(working_commit.author),
    };

    match store.create_commit(new_commit).await {
        Ok(commit) => {
            // Update the branch to point to the new commit
            let mut branch = store.get_branch(&db_id, &branch_name).await.unwrap().unwrap();
            branch.current_commit_hash = commit.hash.clone();
            branch.commit_message = commit.message.clone();
            branch.author = commit.author.clone();

            // Update branch in store
            match store.upsert_version(branch).await {
                Ok(()) => {
                    // Clean up the working commit
                    let _ = store.delete_working_commit(&working_commit.id).await;
                    Ok(Json(CommitResponse::from(commit)))
                }
                Err(e) => return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(&format!(
                        "Commit created but failed to update branch: {}",
                        e
                    ))),
                ))
            }
        }
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

// ========== Working Commit Helper Functions ==========

/// Get or create working commit for staging changes

/// Stage a class update in the working commit
pub async fn stage_class_update<S: WorkingCommitStore + Store>(
    store: &S,
    working_commit: &mut WorkingCommit,
    class_id: &Id,
    updated_class: ClassDef,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Find and update the class in the working commit's schema
    if let Some(class) = working_commit
        .schema_data
        .classes
        .iter_mut()
        .find(|c| c.id == *class_id)
    {
        *class = updated_class;
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Class not found in schema")),
        ));
    }

    // Update the working commit in storage
    match store.update_working_commit(working_commit.clone()).await {
        Ok(()) => Ok(()),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// HTTP handler: Stage a class schema update in the active working commit
pub async fn stage_class_schema_update<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_id, class_id)): Path<(Id, Id, Id)>,
    RequestJson(update): RequestJson<ClassDefUpdate>,
) -> Result<Json<ClassDef>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err((status, response)) => return Err((status, response)),
    };
    // Get the active working commit
    let mut working_commit = match store.get_active_working_commit_for_branch(&db_id, &branch_name).await {
        Ok(Some(wc)) => wc,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("No active working commit for this branch. Create a working commit first.")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Verify working commit belongs to the correct database and branch
    if working_commit.database_id != db_id || working_commit.branch_name.as_ref() != Some(&branch_name) {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Working commit not found for this database and branch")),
        ));
    }

    // Find and update the class in the working commit's schema
    let updated_class = if let Some(class) = working_commit
        .schema_data
        .classes
        .iter_mut()
        .find(|c| c.id == class_id)
    {
        // Apply partial updates
        class.name = update.name.unwrap_or(class.name.clone());
        class.properties = update.properties.unwrap_or(class.properties.clone());
        class.relationships = update.relationships.unwrap_or(class.relationships.clone());
        class.derived = update.derived.unwrap_or(class.derived.clone());
        class.description = if update.description.is_some() {
            update.description
        } else {
            class.description.clone()
        };
        class.domain_constraint = if update.domain_constraint.is_some() {
            update.domain_constraint
        } else {
            class.domain_constraint.clone()
        };
        class.clone()
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Class not found in schema")),
        ));
    };

    // Update the working commit timestamp
    working_commit.touch();

    // Save the updated working commit
    match store.update_working_commit(working_commit).await {
        Ok(()) => Ok(Json(updated_class)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// HTTP handler: Stage an instance update in the active working commit
pub async fn stage_instance_property_update<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_id, instance_id)): Path<(Id, Id, Id)>,
    RequestJson(request): RequestJson<serde_json::Value>,
) -> Result<Json<Instance>, (StatusCode, Json<ErrorResponse>)> {
    let branch_name = match get_branch_name_from_legacy_id(&*store, &db_id, &branch_id).await {
        Ok(name) => name,
        Err((status, response)) => return Err((status, response)),
    };
    
    // Parse the request - support partial updates for properties, relationships, class, domain
    if !request.is_object() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Request must be a JSON object with fields to update")),
        ));
    }

    // Get or create a working commit for this branch (automatic creation like regular instance PATCH)
    let mut working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Find and update the instance in the working commit
    let updated_instance = if let Some(instance) = working_commit
        .instances_data
        .iter_mut()
        .find(|i| i.id == instance_id)
    {
        // Apply partial updates using the same logic as regular instance PATCH
        for (key, value) in request.as_object().unwrap() {
            match key.as_str() {
                "properties" => {
                    if let Ok(props) = serde_json::from_value::<HashMap<String, PropertyValue>>(value.clone()) {
                        // PATCH semantics: merge new properties with existing ones
                        for (prop_key, prop_value) in props {
                            instance.properties.insert(prop_key, prop_value);
                        }
                    } else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::new(&format!("Invalid properties format. Received: {}", value))),
                        ));
                    }
                }
                "relationships" => {
                    if let Ok(rels) = serde_json::from_value::<HashMap<String, RelationshipSelection>>(value.clone()) {
                        // PATCH semantics: merge new relationships with existing ones
                        for (rel_key, rel_value) in rels {
                            instance.relationships.insert(rel_key, rel_value);
                        }
                    } else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::new(&format!("Invalid relationships format. Received: {}", value))),
                        ));
                    }
                }
                "class" => {
                    if let Ok(class_id) = serde_json::from_value::<String>(value.clone()) {
                        instance.class_id = class_id;
                    } else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::new(&format!("Invalid class format. Received: {}", value))),
                        ));
                    }
                }
                "domain" => {
                    if let Ok(domain) = serde_json::from_value::<Domain>(value.clone()) {
                        instance.domain = Some(domain);
                    } else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::new(&format!("Invalid domain format. Received: {}", value))),
                        ));
                    }
                }
                _ => {
                    // Ignore unknown fields in PATCH operations
                }
            }
        }
        
        instance.clone()
    } else {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Instance not found in working commit")),
        ));
    };

    // Update the working commit timestamp
    working_commit.touch();

    // Save the updated working commit
    match store.update_working_commit(working_commit).await {
        Ok(()) => Ok(Json(updated_instance)),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// Stage an instance update in the working commit
pub async fn stage_instance_update<S: WorkingCommitStore + Store>(
    store: &S,
    working_commit: &mut WorkingCommit,
    instance: Instance,
) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
    // Find and update or add the instance in the working commit's instances
    if let Some(existing) = working_commit
        .instances_data
        .iter_mut()
        .find(|i| i.id == instance.id)
    {
        *existing = instance;
    } else {
        working_commit.instances_data.push(instance);
    }

    // Update the working commit in storage
    match store.update_working_commit(working_commit.clone()).await {
        Ok(()) => Ok(()),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// Abandon working commit (discard staged changes)
pub async fn abandon_working_commit<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(version)) => {
            if version.database_id != db_id {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Branch not found in this database")),
                ));
            }
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    // Get the active working commit
    let working_commit = match store.get_active_working_commit_for_branch(&db_id, &branch_name).await {
        Ok(Some(working_commit)) => working_commit,
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("No active working commit found for this branch")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    };

    // Delete the working commit
    match store.delete_working_commit(&working_commit.id).await {
        Ok(true) => Ok(Json(serde_json::json!({
            "message": "Working commit abandoned successfully",
            "working_commit_id": working_commit.id
        }))),
        Ok(false) => return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Working commit not found")),
        )),
        Err(e) => return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// List all commits for a database
pub async fn list_database_commits<S: CommitStore + DatabaseStore>(
    State(store): State<AppState<S>>,
    Path(db_id): Path<Id>,
) -> Result<Json<ListResponse<CommitResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Verify database exists
    match store.get_database(&db_id).await {
        Ok(Some(_)) => {
            // Database exists, continue
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Database not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    // List commits for the database
    match store.list_commits_for_database(&db_id, None).await {
        Ok(commits) => {
            let commit_responses: Vec<CommitResponse> = commits
                .into_iter()
                .map(|commit| CommitResponse::from(commit))
                .collect();
            
            Ok(Json(ListResponse {
                total: commit_responses.len(),
                items: commit_responses,
            }))
        }
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        )),
    }
}

/// Validate all instances in the working commit
pub async fn validate_working_commit<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
) -> Result<Json<ValidationResult>, (StatusCode, Json<ErrorResponse>)> {
    // Get or create the working commit
    let working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    // Validate all instances in the working commit
    let mut result = ValidationResult {
        valid: true,
        errors: Vec::new(),
        warnings: Vec::new(),
        instance_count: working_commit.instances_data.len(),
        validated_instances: Vec::new(),
    };

    for instance in &working_commit.instances_data {
        result.validated_instances.push(instance.id.clone());
        
        match SimpleValidator::validate_instance(&*store, instance, &working_commit.schema_data).await {
            Ok(mut instance_result) => {
                if !instance_result.valid {
                    result.valid = false;
                }
                result.errors.append(&mut instance_result.errors);
                result.warnings.append(&mut instance_result.warnings);
            }
            Err(e) => {
                result.valid = false;
                result.errors.push(crate::logic::validate_simple::ValidationError {
                    instance_id: instance.id.clone(),
                    error_type: crate::logic::validate_simple::ValidationErrorType::InvalidValue,
                    message: format!("Validation failed: {}", e),
                    property_name: None,
                    expected: None,
                    actual: None,
                });
            }
        }
    }

    Ok(Json(result))
}

/// Get the active working commit with resolved relationships
pub async fn get_working_commit_resolved<S: WorkingCommitStore + Store>(
    State(store): State<AppState<S>>,
    Path((db_id, branch_name)): Path<(Id, String)>,
    Query(query): Query<WorkingCommitQuery>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Verify branch belongs to database
    match store.get_branch(&db_id, &branch_name).await {
        Ok(Some(version)) => {
            if version.database_id != db_id {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Branch not found in this database")),
                ));
            }
        }
        Ok(None) => {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Branch not found")),
            ))
        }
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&e.to_string())),
            ))
        }
    }

    let working_commit = get_or_create_working_commit(&*store, &db_id, &branch_name).await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(&e.to_string())),
        ))?;

    if query.changes_only.unwrap_or(false) {
        // Return changes-only view with resolved relationships
        let changes = match working_commit.to_changes(&*store).await {
            Ok(c) => c,
            Err(e) => return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(&format!("Failed to compute changes: {}", e))),
            ))
        };

        // Create enhanced response with resolved relationships
        let mut enhanced_changes = serde_json::to_value(changes).unwrap();

        // Enhance modified instances
        if let Some(instance_changes) = enhanced_changes.get_mut("instance_changes") {
            if let Some(modified) = instance_changes.get_mut("modified") {
                if let Some(modified_array) = modified.as_array_mut() {
                    for instance_value in modified_array {
                        if let Some(instance_obj) = instance_value.as_object_mut() {
                            if let Some(relationships) = instance_obj.get("relationships").cloned() {
                                let mut enhanced_rels = serde_json::Map::new();
                                
                                if let Some(rels_obj) = relationships.as_object() {
                                    for (rel_name, original_selection_value) in rels_obj {
                                        // Parse the original selection
                                        if let Ok(original_selection) = serde_json::from_value::<RelationshipSelection>(original_selection_value.clone()) {
                                            // Resolve the relationship using working commit context
                                            match resolve_selection_with_working_commit_context(
                                                &*store,
                                                &original_selection,
                                                &db_id,
                                                &branch_name,
                                                &working_commit,
                                            ).await {
                                                Ok(resolved_rel) => {
                                                    let enhanced_rel = serde_json::json!({
                                                        "original": original_selection,
                                                        "resolved": {
                                                            "materialized_ids": resolved_rel.materialized_ids,
                                                            "resolution_method": resolved_rel.resolution_method,
                                                            "resolution_details": resolved_rel.resolution_details
                                                        }
                                                    });
                                                    enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                }
                                                Err(_) => {
                                                    // If resolution fails, just show the original
                                                    let enhanced_rel = serde_json::json!({
                                                        "original": original_selection,
                                                        "resolved": null
                                                    });
                                                    enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                // Also check class schema for relationships with default pools that aren't explicitly configured
                                if let Some(instance_class) = instance_obj.get("class") {
                                    if let Some(class_id_str) = instance_class.as_str() {
                                        // Get the class definition from the working commit schema
                                        if let Some(class_def) = working_commit.schema_data.classes.iter().find(|c| c.id == class_id_str) {
                                            for rel_def in &class_def.relationships {
                                                let rel_name = &rel_def.id;
                                                
                                                // Only process if this relationship isn't already in enhanced_rels (i.e., not explicitly configured on instance)
                                                if !enhanced_rels.contains_key(rel_name) {
                                                    // Check if this relationship has a default pool
                                                    if rel_def.default_pool != crate::model::DefaultPool::None {
                                                        // Create a pool-based relationship selection using the default pool
                                                        let default_selection = create_default_pool_selection(rel_def);
                                                        
                                                        // Resolve the default pool relationship
                                                        match resolve_selection_with_working_commit_context(
                                                            &*store,
                                                            &default_selection,
                                                            &db_id,
                                                            &branch_name,
                                                            &working_commit,
                                                        ).await {
                                                            Ok(resolved_rel) => {
                                                                let enhanced_rel = serde_json::json!({
                                                                    "original": default_selection,
                                                                    "resolved": {
                                                                        "materialized_ids": resolved_rel.materialized_ids,
                                                                        "resolution_method": resolved_rel.resolution_method,
                                                                        "resolution_details": resolved_rel.resolution_details
                                                                    }
                                                                });
                                                                enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                            }
                                                            Err(_) => {
                                                                // If resolution fails, show the default selection
                                                                let enhanced_rel = serde_json::json!({
                                                                    "original": default_selection,
                                                                    "resolved": null
                                                                });
                                                                enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                
                                instance_obj.insert("relationships".to_string(), serde_json::Value::Object(enhanced_rels));
                            }
                        }
                    }
                }
            }
        }

        Ok(Json(enhanced_changes))
    } else {
        // Return full working commit with resolved relationships
        let mut enhanced_commit = serde_json::to_value(&working_commit).unwrap();

        // Enhance instances in the full working commit
        if let Some(instances_data) = enhanced_commit.get_mut("instances_data") {
            if let Some(instances_array) = instances_data.as_array_mut() {
                for instance_value in instances_array {
                    if let Some(instance_obj) = instance_value.as_object_mut() {
                        if let Some(relationships) = instance_obj.get("relationships").cloned() {
                            let mut enhanced_rels = serde_json::Map::new();
                            
                            if let Some(rels_obj) = relationships.as_object() {
                                for (rel_name, original_selection_value) in rels_obj {
                                    // Parse the original selection
                                    if let Ok(original_selection) = serde_json::from_value::<RelationshipSelection>(original_selection_value.clone()) {
                                        // Resolve the relationship using working commit context
                                        match resolve_selection_with_working_commit_context(
                                            &*store,
                                            &original_selection,
                                            &db_id,
                                            &branch_name,
                                            &working_commit,
                                        ).await {
                                            Ok(resolved_rel) => {
                                                let enhanced_rel = serde_json::json!({
                                                    "original": original_selection,
                                                    "resolved": {
                                                        "materialized_ids": resolved_rel.materialized_ids,
                                                        "resolution_method": resolved_rel.resolution_method,
                                                        "resolution_details": resolved_rel.resolution_details
                                                    }
                                                });
                                                enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                            }
                                            Err(_) => {
                                                // If resolution fails, just show the original
                                                let enhanced_rel = serde_json::json!({
                                                    "original": original_selection,
                                                    "resolved": null
                                                });
                                                enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                            }
                                        }
                                    }
                                }
                            }
                            
                            // Also check class schema for relationships with default pools that aren't explicitly configured
                            if let Some(instance_class) = instance_obj.get("class") {
                                if let Some(class_id_str) = instance_class.as_str() {
                                    // Get the class definition from the working commit schema
                                    if let Some(class_def) = working_commit.schema_data.classes.iter().find(|c| c.id == class_id_str) {
                                        for rel_def in &class_def.relationships {
                                            let rel_name = &rel_def.id;
                                            
                                            // Only process if this relationship isn't already in enhanced_rels (i.e., not explicitly configured on instance)
                                            if !enhanced_rels.contains_key(rel_name) {
                                                // Check if this relationship has a default pool
                                                if rel_def.default_pool != crate::model::DefaultPool::None {
                                                    // Create a pool-based relationship selection using the default pool
                                                    let default_selection = create_default_pool_selection(rel_def);
                                                    
                                                    // Resolve the default pool relationship
                                                    match resolve_selection_with_working_commit_context(
                                                        &*store,
                                                        &default_selection,
                                                        &db_id,
                                                        &branch_name,
                                                        &working_commit,
                                                    ).await {
                                                        Ok(resolved_rel) => {
                                                            let enhanced_rel = serde_json::json!({
                                                                "original": default_selection,
                                                                "resolved": {
                                                                    "materialized_ids": resolved_rel.materialized_ids,
                                                                    "resolution_method": resolved_rel.resolution_method,
                                                                    "resolution_details": resolved_rel.resolution_details
                                                                }
                                                            });
                                                            enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                        }
                                                        Err(_) => {
                                                            // If resolution fails, show the default selection
                                                            let enhanced_rel = serde_json::json!({
                                                                "original": default_selection,
                                                                "resolved": null
                                                            });
                                                            enhanced_rels.insert(rel_name.clone(), enhanced_rel);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            
                            instance_obj.insert("relationships".to_string(), serde_json::Value::Object(enhanced_rels));
                        }
                    }
                }
            }
        }

        Ok(Json(enhanced_commit))
    }
}

/// Expand relationships in working commit instances and create enhanced response
async fn expand_working_commit_relationships<S: Store>(
    store: &S,
    working_commit: WorkingCommit,
    database_id: &Id,
    branch_name: &str,
) -> anyhow::Result<WorkingCommitResponse> {
    let mut enhanced_instances = Vec::new();
    
    // Process each instance in the working commit
    for instance in &working_commit.instances_data {
        // Expand properties (literal and conditional)
        let mut expanded_props = std::collections::HashMap::new();
        for (key, prop_value) in &instance.properties {
            match prop_value {
                PropertyValue::Literal(typed_value) => {
                    expanded_props.insert(key.clone(), typed_value.value.clone());
                }
                PropertyValue::Conditional(rule_set) => {
                    let value = crate::logic::SimpleEvaluator::evaluate_rule_set(rule_set, instance);
                    expanded_props.insert(key.clone(), value);
                }
            }
        }
        
        // Process relationships: preserve original + add resolved data
        let mut enhanced_relationships = std::collections::HashMap::new();
        for (rel_name, original_selection) in &instance.relationships {
            // Resolve the relationship to get materialized IDs
            let resolved_rel = Expander::resolve_selection_enhanced_with_branch(
                store,
                original_selection,
                database_id,
                Some(branch_name),
            ).await?;
            
            enhanced_relationships.insert(rel_name.clone(), WorkingCommitRelationship {
                original: original_selection.clone(),
                resolved: resolved_rel,
            });
        }
        
        // Also add schema default relationships that aren't explicitly set
        let schema_resolved_rels = Expander::resolve_all_relationships_from_schema(
            store, instance, database_id, branch_name
        ).await?;
        
        for (schema_rel_name, schema_resolved_rel) in schema_resolved_rels {
            if !enhanced_relationships.contains_key(&schema_rel_name) {
                // Create a "default" RelationshipSelection to represent schema defaults
                let default_selection = RelationshipSelection::All; // Represents schema default behavior
                enhanced_relationships.insert(schema_rel_name, WorkingCommitRelationship {
                    original: default_selection,
                    resolved: schema_resolved_rel,
                });
            }
        }
        
        enhanced_instances.push(WorkingCommitInstance {
            id: instance.id.clone(),
            class: instance.class_id.clone(),
            properties: expanded_props,
            relationships: enhanced_relationships,
            created_by: instance.created_by.clone(),
            created_at: instance.created_at,
            updated_by: instance.updated_by.clone(),
            updated_at: instance.updated_at,
        });
    }
    
    Ok(WorkingCommitResponse {
        id: working_commit.id,
        database_id: working_commit.database_id,
        branch_name: working_commit.branch_name,
        based_on_hash: working_commit.based_on_hash,
        author: working_commit.author,
        created_at: working_commit.created_at,
        updated_at: working_commit.updated_at,
        schema_data: working_commit.schema_data,
        instances_data: enhanced_instances,
        status: working_commit.status,
    })
}

/// Expand relationships in working commit changes and create enhanced response
async fn expand_changes_relationships<S: Store>(
    store: &S,
    changes: crate::model::WorkingCommitChanges,
    database_id: &Id,
    branch_name: &str,
) -> anyhow::Result<WorkingCommitChangesResponse> {
    
    // Helper function to convert Instance to WorkingCommitInstance with expanded relationships
    async fn expand_instance<S: Store>(
        store: &S,
        instance: &crate::model::Instance,
        database_id: &Id,
        branch_name: &str,
    ) -> anyhow::Result<WorkingCommitInstance> {
        // Expand properties (literal and conditional)
        let mut expanded_props = std::collections::HashMap::new();
        for (key, prop_value) in &instance.properties {
            match prop_value {
                PropertyValue::Literal(typed_value) => {
                    expanded_props.insert(key.clone(), typed_value.value.clone());
                }
                PropertyValue::Conditional(rule_set) => {
                    let value = crate::logic::SimpleEvaluator::evaluate_rule_set(rule_set, instance);
                    expanded_props.insert(key.clone(), value);
                }
            }
        }
        
        // Process relationships: preserve original + add resolved data
        let mut enhanced_relationships = std::collections::HashMap::new();
        for (rel_name, original_selection) in &instance.relationships {
            // Resolve the relationship to get materialized IDs
            let resolved_rel = Expander::resolve_selection_enhanced_with_branch(
                store,
                original_selection,
                database_id,
                Some(branch_name),
            ).await?;
            
            enhanced_relationships.insert(rel_name.clone(), WorkingCommitRelationship {
                original: original_selection.clone(),
                resolved: resolved_rel,
            });
        }
        
        // Also add schema default relationships that aren't explicitly set
        let schema_resolved_rels = Expander::resolve_all_relationships_from_schema(
            store, instance, database_id, branch_name
        ).await?;
        
        for (schema_rel_name, schema_resolved_rel) in schema_resolved_rels {
            if !enhanced_relationships.contains_key(&schema_rel_name) {
                // Create a "default" RelationshipSelection to represent schema defaults
                let default_selection = RelationshipSelection::All; // Represents schema default behavior
                enhanced_relationships.insert(schema_rel_name, WorkingCommitRelationship {
                    original: default_selection,
                    resolved: schema_resolved_rel,
                });
            }
        }
        
        Ok(WorkingCommitInstance {
            id: instance.id.clone(),
            class: instance.class_id.clone(),
            properties: expanded_props,
            relationships: enhanced_relationships,
            created_by: instance.created_by.clone(),
            created_at: instance.created_at,
            updated_by: instance.updated_by.clone(),
            updated_at: instance.updated_at,
        })
    }
    
    // Expand added instances
    let mut enhanced_added = Vec::new();
    for instance in &changes.instance_changes.added {
        enhanced_added.push(expand_instance(store, instance, database_id, branch_name).await?);
    }
    
    // Expand modified instances
    let mut enhanced_modified = Vec::new();
    for instance in &changes.instance_changes.modified {
        enhanced_modified.push(expand_instance(store, instance, database_id, branch_name).await?);
    }
    
    Ok(WorkingCommitChangesResponse {
        id: changes.id,
        database_id: changes.database_id,
        branch_name: changes.branch_name,
        based_on_hash: changes.based_on_hash,
        author: changes.author,
        created_at: changes.created_at,
        updated_at: changes.updated_at,
        status: changes.status,
        schema_changes: changes.schema_changes,
        instance_changes: EnhancedInstanceChanges {
            added: enhanced_added,
            modified: enhanced_modified,
            deleted: changes.instance_changes.deleted,
        },
    })
}

/// Resolve relationship selection using working commit context (includes working commit instances)
async fn resolve_selection_with_working_commit_context<S: Store>(
    store: &S,
    selection: &RelationshipSelection,
    database_id: &Id,
    branch_name: &str,
    working_commit: &WorkingCommit,
) -> anyhow::Result<crate::model::ResolvedRelationship> {
    use std::time::Instant;
    
    let start_time = Instant::now();
    
    match selection {
        RelationshipSelection::SimpleIds(ids) => {
            // For simple IDs, just return them as-is
            Ok(crate::model::ResolvedRelationship {
                materialized_ids: ids.clone(),
                resolution_method: crate::model::ResolutionMethod::ExplicitIds,
                resolution_details: Some(crate::model::ResolutionDetails {
                    original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                    resolved_from: Some("simple_ids".to_string()),
                    filter_description: None,
                    total_pool_size: Some(ids.len()),
                    filtered_out_count: Some(0),
                    resolution_time_us: Some(start_time.elapsed().as_micros() as u64),
                    notes: vec!["Explicitly set instance IDs".to_string()],
                }),
            })
        }
        RelationshipSelection::Ids { ids } => {
            // For explicit IDs, just return them as-is
            Ok(crate::model::ResolvedRelationship {
                materialized_ids: ids.clone(),
                resolution_method: crate::model::ResolutionMethod::ExplicitIds,
                resolution_details: Some(crate::model::ResolutionDetails {
                    original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                    resolved_from: Some("explicit_ids".to_string()),
                    filter_description: None,
                    total_pool_size: Some(ids.len()),
                    filtered_out_count: Some(0),
                    resolution_time_us: Some(start_time.elapsed().as_micros() as u64),
                    notes: vec!["Explicitly set instance IDs".to_string()],
                }),
            })
        }
        RelationshipSelection::PoolBased { pool, selection: _ } => {
            if let Some(pool_filter) = pool {
                // Resolve pool using working commit instances instead of branch instances
                let pool_instances = resolve_pool_filter_with_working_commit(
                    pool_filter,
                    working_commit,
                )?;
                
                let pool_size = pool_instances.len();
                
                Ok(crate::model::ResolvedRelationship {
                    materialized_ids: pool_instances.clone(),
                    resolution_method: crate::model::ResolutionMethod::PoolFilterResolved,
                    resolution_details: Some(crate::model::ResolutionDetails {
                        original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                        resolved_from: Some("working_commit_pool_filter".to_string()),
                        filter_description: Some(format!("Pool filter using working commit data: {:?}", pool_filter)),
                        total_pool_size: Some(pool_size),
                        filtered_out_count: Some(0),
                        resolution_time_us: Some(start_time.elapsed().as_micros() as u64),
                        notes: vec![format!("Resolved {} instances from working commit pool", pool_size)],
                    }),
                })
            } else {
                // No pool filter - return empty
                Ok(crate::model::ResolvedRelationship {
                    materialized_ids: Vec::new(),
                    resolution_method: crate::model::ResolutionMethod::EmptyResolution,
                    resolution_details: Some(crate::model::ResolutionDetails {
                        original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                        resolved_from: Some("no_pool_filter".to_string()),
                        filter_description: Some("No pool filter specified".to_string()),
                        total_pool_size: Some(0),
                        filtered_out_count: Some(0),
                        resolution_time_us: Some(start_time.elapsed().as_micros() as u64),
                        notes: vec!["No pool filter to resolve".to_string()],
                    }),
                })
            }
        }
        _ => {
            // For other types, fall back to the standard resolution
            Expander::resolve_selection_enhanced_with_branch(store, selection, database_id, Some(branch_name)).await
        }
    }
}

/// Resolve pool filter using working commit instances instead of branch instances
fn resolve_pool_filter_with_working_commit(
    filter: &crate::model::InstanceFilter,
    working_commit: &WorkingCommit,
) -> anyhow::Result<Vec<Id>> {
    if let Some(types) = &filter.types {
        let mut matching_instances = Vec::new();
        
        // Search through working commit instances instead of branch instances
        for target_type in types {
            for instance in &working_commit.instances_data {
                if instance.class_id == *target_type {
                    matching_instances.push(instance.clone());
                }
            }
        }
        
        // Apply where_clause filters if present (basic implementation for now)
        if let Some(_where_clause) = &filter.where_clause {
            matching_instances.retain(|instance| {
                // For now, implement a basic price filter
                // TODO: Implement proper where clause evaluation
                if let Some(where_obj) = _where_clause.as_object() {
                    for (prop_key, condition) in where_obj {
                        if let Some(prop_value) = instance.properties.get(prop_key) {
                            if let crate::model::PropertyValue::Literal(typed_value) = prop_value {
                                if let Some(condition_obj) = condition.as_object() {
                                    for (op, target_value) in condition_obj {
                                        match op.as_str() {
                                            "lt" => {
                                                if let (serde_json::Value::Number(prop_num), Some(target_str)) = (&typed_value.value, target_value.as_str()) {
                                                    if let (Some(prop_val), Ok(target_val)) = (prop_num.as_f64(), target_str.parse::<f64>()) {
                                                        if prop_val >= target_val {
                                                            return false; // Doesn't match, exclude
                                                        }
                                                    } else {
                                                        return false; // Can't parse, exclude
                                                    }
                                                } else {
                                                    return false; // Not a number or can't parse, exclude
                                                }
                                            }
                                            "gt" => {
                                                if let (serde_json::Value::Number(prop_num), Some(target_str)) = (&typed_value.value, target_value.as_str()) {
                                                    if let (Some(prop_val), Ok(target_val)) = (prop_num.as_f64(), target_str.parse::<f64>()) {
                                                        if prop_val <= target_val {
                                                            return false; // Doesn't match, exclude
                                                        }
                                                    } else {
                                                        return false; // Can't parse, exclude
                                                    }
                                                } else {
                                                    return false; // Not a number or can't parse, exclude
                                                }
                                            }
                                            "eq" => {
                                                if &typed_value.value != target_value {
                                                    return false; // Doesn't match, exclude
                                                }
                                            }
                                            _ => {
                                                return false; // Unknown operator, exclude
                                            }
                                        }
                                    }
                                } else {
                                    return false; // Not an object condition, exclude
                                }
                            } else {
                                return false; // Not a literal property, exclude
                            }
                        } else {
                            return false; // Property doesn't exist, exclude
                        }
                    }
                    true // All conditions matched, include
                } else {
                    true // No where clause, include all
                }
            });
        }
        
        // Apply sorting if present (similar to expand.rs implementation)
        if let Some(sort_field) = &filter.sort {
            if let Some(order) = sort_field.strip_suffix(" DESC") {
                let field_name = order.trim();
                matching_instances.sort_by(|a, b| {
                    compare_instances_by_field_basic(b, a, field_name).unwrap_or(std::cmp::Ordering::Equal)
                });
            } else if let Some(field_name) = sort_field.strip_suffix(" ASC") {
                let field_name = field_name.trim();
                matching_instances.sort_by(|a, b| {
                    compare_instances_by_field_basic(a, b, field_name).unwrap_or(std::cmp::Ordering::Equal)
                });
            } else {
                // Default to ASC if no order specified
                let field_name = sort_field.trim();
                matching_instances.sort_by(|a, b| {
                    compare_instances_by_field_basic(a, b, field_name).unwrap_or(std::cmp::Ordering::Equal)
                });
            }
        }
        
        // Apply limit if present
        if let Some(limit) = filter.limit {
            matching_instances.truncate(limit);
        }
        
        Ok(matching_instances.into_iter().map(|i| i.id).collect())
    } else {
        Ok(Vec::new())
    }
}

/// Basic field comparison for working commit instances
fn compare_instances_by_field_basic(
    a: &crate::model::Instance, 
    b: &crate::model::Instance, 
    field_name: &str
) -> anyhow::Result<std::cmp::Ordering> {
    let a_value = a.properties.get(field_name);
    let b_value = b.properties.get(field_name);
    
    match (a_value, b_value) {
        (Some(crate::model::PropertyValue::Literal(a_typed)), Some(crate::model::PropertyValue::Literal(b_typed))) => {
            match (&a_typed.value, &b_typed.value) {
                (serde_json::Value::Number(a_num), serde_json::Value::Number(b_num)) => {
                    if let (Some(a_f64), Some(b_f64)) = (a_num.as_f64(), b_num.as_f64()) {
                        Ok(a_f64.partial_cmp(&b_f64).unwrap_or(std::cmp::Ordering::Equal))
                    } else {
                        Ok(std::cmp::Ordering::Equal)
                    }
                }
                (serde_json::Value::String(a_str), serde_json::Value::String(b_str)) => {
                    Ok(a_str.cmp(b_str))
                }
                _ => Ok(std::cmp::Ordering::Equal)
            }
        }
        (Some(_), None) => Ok(std::cmp::Ordering::Greater),
        (None, Some(_)) => Ok(std::cmp::Ordering::Less),
        (None, None) => Ok(std::cmp::Ordering::Equal),
        _ => Ok(std::cmp::Ordering::Equal),
    }
}
