use serde::{Deserialize, Serialize};

/// User context extracted from request headers for audit trail
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UserContext {
    pub user_id: String,
    pub user_email: Option<String>,
    pub user_name: Option<String>,
}

impl UserContext {
    /// Create a new UserContext with just a user ID
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            user_email: None,
            user_name: None,
        }
    }

    /// Create a UserContext with full user information
    pub fn with_details(user_id: String, email: Option<String>, name: Option<String>) -> Self {
        Self {
            user_id,
            user_email: email,
            user_name: name,
        }
    }

    /// Create a system user context for internal operations
    pub fn system() -> Self {
        Self {
            user_id: "system".to_string(),
            user_email: Some("system@oat-db.internal".to_string()),
            user_name: Some("System".to_string()),
        }
    }

    /// Create a default user context for development/testing
    pub fn default_user() -> Self {
        Self {
            user_id: "dev-user".to_string(),
            user_email: Some("dev@localhost".to_string()),
            user_name: Some("Development User".to_string()),
        }
    }
}

impl Default for UserContext {
    fn default() -> Self {
        Self::default_user()
    }
}