use crate::model::{Id, InstanceFilter};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Resolution mode for selectors - how instances are determined
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMode {
    /// Static selector with pre-materialized instance IDs
    Static,
    /// Dynamic selector using filters to determine instances at resolve time
    Dynamic,
}

/// Selector describes WHAT to select without branch/commit context
/// Replaces the current pool/selection system with a cleaner abstraction
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Selector {
    /// How instances are resolved for this selector
    pub resolution_mode: ResolutionMode,

    /// Optional filter for dynamic selectors (ignored for static selectors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<InstanceFilter>,

    /// Pre-materialized instance IDs for static selectors (ignored for dynamic selectors)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub materialized_ids: Option<Vec<Id>>,

    /// Optional metadata about this selector
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<SelectorMetadata>,
}

/// Optional metadata for selectors
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SelectorMetadata {
    /// Human-readable description of what this selector represents
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags for categorizing selectors
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// Custom properties for extensions
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

impl Selector {
    /// Create a new static selector with materialized instance IDs
    pub fn static_with_ids(ids: Vec<Id>) -> Self {
        Self {
            resolution_mode: ResolutionMode::Static,
            filter: None,
            materialized_ids: Some(ids),
            metadata: None,
        }
    }

    /// Create a new dynamic selector with a filter
    pub fn dynamic_with_filter(filter: InstanceFilter) -> Self {
        Self {
            resolution_mode: ResolutionMode::Dynamic,
            filter: Some(filter),
            materialized_ids: None,
            metadata: None,
        }
    }

    /// Create an empty static selector (selects nothing)
    pub fn empty() -> Self {
        Self {
            resolution_mode: ResolutionMode::Static,
            filter: None,
            materialized_ids: Some(Vec::new()),
            metadata: None,
        }
    }

    /// Add metadata to this selector
    pub fn with_metadata(mut self, metadata: SelectorMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Add a description to this selector's metadata
    pub fn with_description(mut self, description: String) -> Self {
        let metadata = self.metadata.get_or_insert_with(|| SelectorMetadata {
            description: None,
            tags: Vec::new(),
            custom: HashMap::new(),
        });
        metadata.description = Some(description);
        self
    }

    /// Check if this selector is empty (selects no instances)
    pub fn is_empty(&self) -> bool {
        match &self.resolution_mode {
            ResolutionMode::Static => self
                .materialized_ids
                .as_ref()
                .map_or(true, |ids| ids.is_empty()),
            ResolutionMode::Dynamic => false, // Dynamic selectors may resolve to instances
        }
    }

    /// Get the number of materialized IDs for static selectors
    pub fn materialized_count(&self) -> Option<usize> {
        match &self.resolution_mode {
            ResolutionMode::Static => self.materialized_ids.as_ref().map(|ids| ids.len()),
            ResolutionMode::Dynamic => None,
        }
    }

    /// Validate the selector structure
    pub fn validate(&self) -> Result<(), String> {
        match &self.resolution_mode {
            ResolutionMode::Static => {
                if self.materialized_ids.is_none() {
                    return Err("Static selectors must have materialized_ids".to_string());
                }
                if self.filter.is_some() {
                    return Err("Static selectors should not have filters".to_string());
                }
            }
            ResolutionMode::Dynamic => {
                if self.filter.is_none() {
                    return Err("Dynamic selectors must have a filter".to_string());
                }
                if self.materialized_ids.is_some() {
                    return Err("Dynamic selectors should not have materialized_ids".to_string());
                }
            }
        }
        Ok(())
    }
}

impl Default for Selector {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_static_selector_creation() {
        let ids = vec!["id1".to_string(), "id2".to_string()];
        let selector = Selector::static_with_ids(ids.clone());

        assert_eq!(selector.resolution_mode, ResolutionMode::Static);
        assert_eq!(selector.materialized_ids, Some(ids));
        assert!(selector.filter.is_none());
        assert!(selector.validate().is_ok());
    }

    #[test]
    fn test_dynamic_selector_creation() {
        let filter = InstanceFilter {
            types: Some(vec!["Color".to_string()]),
            where_clause: Some(crate::logic::FilterExpr::All {
                all: vec![
                    crate::logic::FilterExpr::Exists {
                        exists: crate::logic::JsonPath("$.price_low".to_string()),
                    }
                ],
            }),
            sort: None,
            limit: None,
        };

        let selector = Selector::dynamic_with_filter(filter.clone());

        assert_eq!(selector.resolution_mode, ResolutionMode::Dynamic);
        assert_eq!(selector.filter, Some(filter));
        assert!(selector.materialized_ids.is_none());
        assert!(selector.validate().is_ok());
    }

    #[test]
    fn test_empty_selector() {
        let selector = Selector::empty();

        assert_eq!(selector.resolution_mode, ResolutionMode::Static);
        assert_eq!(selector.materialized_ids, Some(Vec::new()));
        assert!(selector.is_empty());
        assert!(selector.validate().is_ok());
    }

    #[test]
    fn test_selector_with_metadata() {
        let selector = Selector::empty().with_description("Test selector".to_string());

        assert!(selector.metadata.is_some());
        assert_eq!(
            selector.metadata.unwrap().description,
            Some("Test selector".to_string())
        );
    }

    #[test]
    fn test_selector_validation() {
        // Invalid static selector without materialized_ids
        let invalid_static = Selector {
            resolution_mode: ResolutionMode::Static,
            filter: None,
            materialized_ids: None,
            metadata: None,
        };
        assert!(invalid_static.validate().is_err());

        // Invalid dynamic selector without filter
        let invalid_dynamic = Selector {
            resolution_mode: ResolutionMode::Dynamic,
            filter: None,
            materialized_ids: None,
            metadata: None,
        };
        assert!(invalid_dynamic.validate().is_err());
    }
}
