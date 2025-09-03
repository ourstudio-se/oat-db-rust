use crate::model::{generate_id, Id};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Database {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,            // ISO 8601 timestamp
    pub default_branch_name: String, // Name of the main/default branch
}

impl Database {
    pub fn new(name: String, description: Option<String>) -> Self {
        Self {
            id: generate_id(),
            name,
            description,
            created_at: chrono::Utc::now().to_rfc3339(),
            default_branch_name: "main".to_string(), // Default to main branch
        }
    }

    pub fn new_with_id(id: Id, name: String, description: Option<String>) -> Self {
        Self {
            id,
            name,
            description,
            created_at: chrono::Utc::now().to_rfc3339(),
            default_branch_name: "main".to_string(), // Default to main branch
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BranchStatus {
    Active,   // Branch is actively being worked on
    Merged,   // Branch has been merged to parent
    Archived, // Branch is archived but kept for history
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Branch {
    pub database_id: Id,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,             // ISO 8601 timestamp
    pub parent_branch_name: Option<String>,   // Which branch this was created from (within same database)
    pub current_commit_hash: String,            // Current state identifier
    pub commit_message: Option<String>, // Latest commit message
    pub author: Option<String>,         // Who made the latest commit
    pub status: BranchStatus,
}

impl Branch {
    pub fn new_main_branch(database_id: Id, author: Option<String>) -> Self {
        Self {
            database_id,
            name: "main".to_string(),
            description: Some("Default main branch".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_branch_name: None,     // Main branch has no parent
            current_commit_hash: "".to_string(), // No commits yet
            commit_message: Some("Initial commit".to_string()),
            author,
            status: BranchStatus::Active,
        }
    }

    pub fn new(
        database_id: Id,
        name: String,
        description: Option<String>,
        author: Option<String>,
    ) -> Self {
        Self {
            database_id,
            name: name.clone(),
            description,
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_branch_name: None,     // No parent for new branches
            current_commit_hash: generate_id(), // New branch gets fresh commit
            commit_message: Some(format!("Created branch '{}'", name)),
            author,
            status: BranchStatus::Active,
        }
    }

    pub fn new_from_branch(
        database_id: Id,
        parent_branch_name: String,
        name: String,
        description: Option<String>,
        author: Option<String>,
    ) -> Self {
        Self {
            database_id,
            name: name.clone(),
            description,
            created_at: chrono::Utc::now().to_rfc3339(),
            parent_branch_name: Some(parent_branch_name),
            current_commit_hash: generate_id(), // New branch gets fresh commit
            commit_message: Some(format!("Created branch '{}'", name)),
            author,
            status: BranchStatus::Active,
        }
    }

    pub fn mark_as_merged(&mut self, commit_message: Option<String>) {
        self.status = BranchStatus::Merged;
        if let Some(message) = commit_message {
            self.commit_message = Some(message);
            self.current_commit_hash = generate_id();
        }
    }

    pub fn mark_as_archived(&mut self) {
        self.status = BranchStatus::Archived;
    }

    pub fn can_be_merged(&self) -> bool {
        self.status == BranchStatus::Active
    }

    pub fn can_be_deleted(&self) -> bool {
        matches!(self.status, BranchStatus::Merged | BranchStatus::Archived)
    }
}

/// Input model for creating a new database
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewDatabase {
    pub id: Id,
    pub name: String,
    pub description: Option<String>,
}

impl NewDatabase {
    /// Convert to a full Database with server-generated fields
    pub fn into_database(self) -> Database {
        Database::new_with_id(self.id, self.name, self.description)
    }
}

// Keep Version as an alias for backward compatibility during migration
pub type Version = Branch;
