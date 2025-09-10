use crate::model::{Id};
use crate::logic::FilterExpr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceFilter {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub types: Option<Vec<String>>,

    #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
    pub where_clause: Option<FilterExpr>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RelationshipSelection {
    // Simple array format (most common) - should be tried first due to untagged
    SimpleIds(Vec<Id>),
    // Tagged formats - these are more specific and should come before PoolBased
    Ids { ids: Vec<Id> },
    Filter { filter: InstanceFilter },
    All,
    // New pool-based selection format for combinatorial optimization
    // IMPORTANT: This must come AFTER the tagged variants because it has optional fields
    // that would match almost any JSON object structure
    PoolBased {
        pool: Option<InstanceFilter>,
        #[serde(skip_serializing_if = "Option::is_none")]
        selection: Option<SelectionSpec>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SelectionSpec {
    /// Explicit list of instance IDs
    Ids(Vec<Id>),
    /// Filter to select from the pool  
    Filter(InstanceFilter),
    /// Select all instances from the pool
    All,
    /// Selection is unresolved - to be determined by solver/user
    Unresolved,
}
