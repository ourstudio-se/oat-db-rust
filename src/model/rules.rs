use crate::model::BoolExpr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RuleSet {
    // New simple format: {"rules": [...]} - matches user's desired JSON structure
    Simple { 
        rules: Vec<RuleBranch>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<serde_json::Value>,
    },
    // Original format for backward compatibility
    Complex {
        branches: Vec<RuleBranch>,
        #[serde(skip_serializing_if = "Option::is_none")]
        default: Option<serde_json::Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuleBranch {
    pub when: BoolExpr,
    pub then: serde_json::Value,
}
