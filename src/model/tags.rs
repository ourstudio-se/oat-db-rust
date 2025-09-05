use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::model::Id;

/// Types of tags that can be applied to commits
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TagType {
    /// Semantic version tags (v1.0.0, v2.1.3, etc.) - version info stored in metadata
    Version,
    /// Release tags (prod-release, staging-deploy, etc.)
    Release,
    /// Milestone markers (feature-complete, beta-ready, etc.)
    Milestone,
    /// Custom user-defined tags
    Custom,
}

impl std::fmt::Display for TagType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TagType::Version => write!(f, "version"),
            TagType::Release => write!(f, "release"),
            TagType::Milestone => write!(f, "milestone"),
            TagType::Custom => write!(f, "custom"),
        }
    }
}

impl std::str::FromStr for TagType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "version" => Ok(TagType::Version),
            "release" => Ok(TagType::Release),
            "milestone" => Ok(TagType::Milestone),
            "custom" => Ok(TagType::Custom),
            _ => Err(format!("Unknown tag type: {}", s)),
        }
    }
}

/// A tag applied to a specific commit for labeling and organization
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommitTag {
    /// Unique tag ID
    pub id: i32,
    /// The commit this tag is applied to
    pub commit_hash: String,
    /// Type of tag
    pub tag_type: TagType,
    /// Human-readable tag name
    pub tag_name: String,
    /// Optional description of what this tag represents
    pub tag_description: Option<String>,
    /// When the tag was created
    pub created_at: String, // ISO 8601 string
    /// Who created the tag
    pub created_by: Option<String>,
    /// Additional flexible metadata
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Version information stored in commit tag metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct VersionInfo {
    /// Major version number (breaking changes)
    pub major: i32,
    /// Minor version number (new features)
    pub minor: i32,
    /// Patch version number (bug fixes)
    pub patch: i32,
    /// Pre-release identifier (alpha, beta, rc1, etc.)
    pub pre_release: Option<String>,
    /// Build metadata (build number, date, etc.)
    pub build_metadata: Option<String>,
    /// Whether this is the latest version for the database
    pub is_latest: Option<bool>,
    /// Optional release notes
    pub release_notes: Option<String>,
}

impl VersionInfo {
    /// Generate semantic version string from components
    pub fn version_string(&self) -> String {
        let mut version = format!("v{}.{}.{}", self.major, self.minor, self.patch);
        
        if let Some(pre_release) = &self.pre_release {
            version.push_str(&format!("-{}", pre_release));
        }
        
        if let Some(build_metadata) = &self.build_metadata {
            version.push_str(&format!("+{}", build_metadata));
        }
        
        version
    }
}

/// Request to create a new commit tag
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewCommitTag {
    /// The commit to tag
    pub commit_hash: String,
    /// Type of tag
    pub tag_type: TagType,
    /// Tag name
    pub tag_name: String,
    /// Optional description
    pub tag_description: Option<String>,
    /// Who is creating the tag
    pub created_by: Option<String>,
    /// Additional metadata
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Combined view of commit with its tags
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaggedCommit {
    /// Commit hash
    pub commit_hash: String,
    /// Database ID
    pub database_id: Id,
    /// Commit message
    pub commit_message: Option<String>,
    /// Commit author
    pub commit_author: Option<String>,
    /// When commit was created
    pub commit_created_at: String,
    /// All tags applied to this commit
    pub tags: Vec<CommitTag>,
}

/// Query parameters for filtering tagged commits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TagQuery {
    /// Filter by tag type
    pub tag_type: Option<TagType>,
    /// Filter by tag name (partial match)
    pub tag_name: Option<String>,
    /// Limit number of results
    pub limit: Option<i32>,
}

impl NewCommitTag {
    /// Convert to CommitTag (requires generated ID and timestamps)
    pub fn to_commit_tag(&self, id: i32) -> CommitTag {
        CommitTag {
            id,
            commit_hash: self.commit_hash.clone(),
            tag_type: self.tag_type.clone(),
            tag_name: self.tag_name.clone(),
            tag_description: self.tag_description.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            created_by: self.created_by.clone(),
            metadata: self.metadata.clone().unwrap_or_default(),
        }
    }
}

impl CommitTag {
    /// Get version information from metadata if this is a version tag
    pub fn version_info(&self) -> Option<VersionInfo> {
        if self.tag_type == TagType::Version {
            serde_json::from_value(serde_json::Value::Object(
                self.metadata.clone().into_iter().collect()
            )).ok()
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_generation() {
        let version_info = VersionInfo {
            major: 1,
            minor: 2,
            patch: 3,
            pre_release: Some("beta".to_string()),
            build_metadata: Some("20241201".to_string()),
            is_latest: Some(true),
            release_notes: Some("Test release".to_string()),
        };
        
        assert_eq!(version_info.version_string(), "v1.2.3-beta+20241201");
    }
    
    #[test]
    fn test_commit_tag_version_info() {
        let mut metadata = HashMap::new();
        metadata.insert("major".to_string(), serde_json::Value::Number(serde_json::Number::from(2)));
        metadata.insert("minor".to_string(), serde_json::Value::Number(serde_json::Number::from(1)));
        metadata.insert("patch".to_string(), serde_json::Value::Number(serde_json::Number::from(0)));
        
        let commit_tag = CommitTag {
            id: 1,
            commit_hash: "abc123".to_string(),
            tag_type: TagType::Version,
            tag_name: "v2.1.0".to_string(),
            tag_description: Some("Version 2.1.0".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            created_by: Some("tester".to_string()),
            metadata,
        };
        
        let version_info = commit_tag.version_info();
        assert!(version_info.is_some());
        
        // Non-version tag should return None
        let milestone_tag = CommitTag {
            id: 2,
            commit_hash: "def456".to_string(),
            tag_type: TagType::Milestone,
            tag_name: "feature-complete".to_string(),
            tag_description: None,
            created_at: "2024-01-01T00:00:00Z".to_string(),
            created_by: None,
            metadata: HashMap::new(),
        };
        
        assert!(milestone_tag.version_info().is_none());
    }
}