use crate::model::Id;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Resolution context provides the WHERE/WHEN scope for selector evaluation
/// This separates concerns: Selectors describe WHAT, ResolutionContext provides WHERE/WHEN
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionContext {
    /// The database being resolved against
    pub database_id: Id,
    
    /// The specific branch/commit to resolve selectors against
    pub branch_id: Id,
    
    /// Optional commit hash for point-in-time resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
    
    /// Resolution policies that affect how selectors are evaluated
    pub policies: ResolutionPolicies,
    
    /// Optional metadata about this resolution context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<ResolutionContextMetadata>,
}

/// Policies that control how selectors are resolved
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionPolicies {
    /// How to handle cross-branch references in relationships
    pub cross_branch_policy: CrossBranchPolicy,
    
    /// How to handle missing instances referenced by static selectors
    pub missing_instance_policy: MissingInstancePolicy,
    
    /// How to handle failed dynamic selector filters (e.g., no matches)
    pub empty_selection_policy: EmptySelectionPolicy,
    
    /// Maximum number of instances a dynamic selector can resolve to (prevents runaway selections)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_selection_size: Option<usize>,
    
    /// Custom policies for extensions
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Default for ResolutionPolicies {
    fn default() -> Self {
        Self {
            cross_branch_policy: CrossBranchPolicy::Reject,
            missing_instance_policy: MissingInstancePolicy::Skip,
            empty_selection_policy: EmptySelectionPolicy::Allow,
            max_selection_size: Some(1000),
            custom: HashMap::new(),
        }
    }
}

/// How to handle cross-branch references
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CrossBranchPolicy {
    /// Reject any cross-branch references (strict branch isolation)
    Reject,
    /// Allow cross-branch references with warnings
    AllowWithWarnings,
    /// Allow cross-branch references silently
    Allow,
}

/// How to handle missing instances in static selectors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingInstancePolicy {
    /// Fail resolution if any referenced instance is missing
    Fail,
    /// Skip missing instances but continue with available ones
    Skip,
    /// Replace missing instances with placeholders
    Placeholder,
}

/// How to handle empty selections from dynamic selectors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptySelectionPolicy {
    /// Fail resolution if a dynamic selector returns no matches
    Fail,
    /// Allow empty selections (valid for optional relationships)
    Allow,
    /// Use fallback selector if available
    Fallback,
}

/// Optional metadata for resolution contexts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionContextMetadata {
    /// Human-readable description of this resolution context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    
    /// Tags for categorizing resolution contexts
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    
    /// Timestamp when this context was created
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    
    /// User or system that created this context
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,
    
    /// Custom properties for extensions
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

impl ResolutionContext {
    /// Create a new resolution context with strict policies
    pub fn new_strict(database_id: Id, branch_id: Id) -> Self {
        Self {
            database_id,
            branch_id,
            commit_hash: None,
            policies: ResolutionPolicies {
                cross_branch_policy: CrossBranchPolicy::Reject,
                missing_instance_policy: MissingInstancePolicy::Fail,
                empty_selection_policy: EmptySelectionPolicy::Fail,
                max_selection_size: Some(1000), // Reasonable default limit
                custom: HashMap::new(),
            },
            metadata: None,
        }
    }
    
    /// Create a new resolution context with permissive policies
    pub fn new_permissive(database_id: Id, branch_id: Id) -> Self {
        Self {
            database_id,
            branch_id,
            commit_hash: None,
            policies: ResolutionPolicies {
                cross_branch_policy: CrossBranchPolicy::Allow,
                missing_instance_policy: MissingInstancePolicy::Skip,
                empty_selection_policy: EmptySelectionPolicy::Allow,
                max_selection_size: None, // No limit
                custom: HashMap::new(),
            },
            metadata: None,
        }
    }
    
    /// Create a new resolution context at a specific commit
    pub fn at_commit(database_id: Id, branch_id: Id, commit_hash: String) -> Self {
        let mut context = Self::new_strict(database_id, branch_id);
        context.commit_hash = Some(commit_hash);
        context
    }
    
    /// Add metadata to this resolution context
    pub fn with_metadata(mut self, metadata: ResolutionContextMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
    
    /// Add a description to this resolution context
    pub fn with_description(mut self, description: String) -> Self {
        let metadata = self.metadata.get_or_insert_with(|| ResolutionContextMetadata {
            description: None,
            tags: Vec::new(),
            created_at: Some(chrono::Utc::now()),
            created_by: None,
            custom: HashMap::new(),
        });
        metadata.description = Some(description);
        self
    }
    
    /// Set the cross-branch policy
    pub fn with_cross_branch_policy(mut self, policy: CrossBranchPolicy) -> Self {
        self.policies.cross_branch_policy = policy;
        self
    }
    
    /// Set the missing instance policy
    pub fn with_missing_instance_policy(mut self, policy: MissingInstancePolicy) -> Self {
        self.policies.missing_instance_policy = policy;
        self
    }
    
    /// Set the empty selection policy
    pub fn with_empty_selection_policy(mut self, policy: EmptySelectionPolicy) -> Self {
        self.policies.empty_selection_policy = policy;
        self
    }
    
    /// Set the maximum selection size
    pub fn with_max_selection_size(mut self, max_size: Option<usize>) -> Self {
        self.policies.max_selection_size = max_size;
        self
    }
    
    /// Check if this context represents a point-in-time resolution
    pub fn is_point_in_time(&self) -> bool {
        self.commit_hash.is_some()
    }
    
    /// Get the resolution scope as a human-readable string
    pub fn scope_description(&self) -> String {
        match &self.commit_hash {
            Some(commit) => format!("{}@{}@{}", self.database_id, self.branch_id, &commit[..8]),
            None => format!("{}@{}", self.database_id, self.branch_id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strict_resolution_context() {
        let context = ResolutionContext::new_strict("db1".to_string(), "branch1".to_string());
        
        assert_eq!(context.database_id, "db1");
        assert_eq!(context.branch_id, "branch1");
        assert!(context.commit_hash.is_none());
        assert_eq!(context.policies.cross_branch_policy, CrossBranchPolicy::Reject);
        assert_eq!(context.policies.missing_instance_policy, MissingInstancePolicy::Fail);
        assert_eq!(context.policies.empty_selection_policy, EmptySelectionPolicy::Fail);
        assert_eq!(context.policies.max_selection_size, Some(1000));
        assert!(!context.is_point_in_time());
    }
    
    #[test]
    fn test_permissive_resolution_context() {
        let context = ResolutionContext::new_permissive("db1".to_string(), "branch1".to_string());
        
        assert_eq!(context.policies.cross_branch_policy, CrossBranchPolicy::Allow);
        assert_eq!(context.policies.missing_instance_policy, MissingInstancePolicy::Skip);
        assert_eq!(context.policies.empty_selection_policy, EmptySelectionPolicy::Allow);
        assert_eq!(context.policies.max_selection_size, None);
    }
    
    #[test]
    fn test_point_in_time_context() {
        let commit_hash = "abc123def456".to_string();
        let context = ResolutionContext::at_commit(
            "db1".to_string(), 
            "branch1".to_string(), 
            commit_hash.clone()
        );
        
        assert_eq!(context.commit_hash, Some(commit_hash));
        assert!(context.is_point_in_time());
        assert_eq!(context.scope_description(), "db1@branch1@abc123de");
    }
    
    #[test]
    fn test_context_with_metadata() {
        let context = ResolutionContext::new_strict("db1".to_string(), "branch1".to_string())
            .with_description("Test context".to_string());
            
        assert!(context.metadata.is_some());
        assert_eq!(
            context.metadata.unwrap().description,
            Some("Test context".to_string())
        );
    }
    
    #[test]
    fn test_policy_builders() {
        let context = ResolutionContext::new_strict("db1".to_string(), "branch1".to_string())
            .with_cross_branch_policy(CrossBranchPolicy::Allow)
            .with_missing_instance_policy(MissingInstancePolicy::Skip)
            .with_empty_selection_policy(EmptySelectionPolicy::Allow)
            .with_max_selection_size(Some(500));
            
        assert_eq!(context.policies.cross_branch_policy, CrossBranchPolicy::Allow);
        assert_eq!(context.policies.missing_instance_policy, MissingInstancePolicy::Skip);
        assert_eq!(context.policies.empty_selection_policy, EmptySelectionPolicy::Allow);
        assert_eq!(context.policies.max_selection_size, Some(500));
    }
    
    #[test]
    fn test_scope_description() {
        let context1 = ResolutionContext::new_strict("db1".to_string(), "main".to_string());
        assert_eq!(context1.scope_description(), "db1@main");
        
        let context2 = ResolutionContext::at_commit(
            "db1".to_string(),
            "feature".to_string(), 
            "abcdef1234567890".to_string()
        );
        assert_eq!(context2.scope_description(), "db1@feature@abcdef12");
    }
}