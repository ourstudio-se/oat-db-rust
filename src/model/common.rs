use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type Id = String;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum DataType {
    String,
    Number,
    Boolean,
    Object,
    Array,
    #[serde(rename = "StringList")] // Keep for backward compatibility
    StringList,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Quantifier {
    Exactly(usize),
    AtLeast(usize),
    AtMost(usize),
    Range(usize, usize),
    Optional,
    Any,
    All,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SelectionType {
    ExplicitOrFilter,
    FilterAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ComparisonOp {
    Eq,
    Ne,
    Gt,
    Lt,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Domain {
    pub lower: i32,
    pub upper: i32,
}

impl Domain {
    pub fn new(lower: i32, upper: i32) -> Self {
        Self { lower, upper }
    }

    pub fn constant(value: i32) -> Self {
        Self {
            lower: value,
            upper: value,
        }
    }

    pub fn binary() -> Self {
        Self { lower: 0, upper: 1 }
    }

    pub fn is_constant(&self) -> bool {
        self.lower == self.upper
    }

    pub fn is_binary(&self) -> bool {
        self.lower == 0 && self.upper == 1
    }

    pub fn contains(&self, value: i32) -> bool {
        value >= self.lower && value <= self.upper
    }
}

pub fn generate_id() -> Id {
    Uuid::new_v4().to_string()
}

/// Generate a deterministic configuration artifact ID based on commit hash and objective content
pub fn generate_configuration_id(
    commit_hash: Option<&String>, 
    objectives: &HashMap<String, f64>, 
    instance_id: &str
) -> Id {
    use std::collections::BTreeMap;
    use std::hash::{Hash, Hasher};
    
    // Create a deterministic string representation of objectives
    // Using BTreeMap to ensure consistent ordering
    let mut sorted_objectives = BTreeMap::new();
    for (k, v) in objectives {
        sorted_objectives.insert(k.clone(), v.to_bits()); // Use to_bits for exact f64 representation
    }
    
    // Create a string to hash
    let hash_input = format!(
        "{}-{}-{:?}", 
        commit_hash.as_ref().map(|h| h.as_str()).unwrap_or("no-commit"),
        instance_id,
        sorted_objectives
    );
    
    // Use std::hash::DefaultHasher for deterministic hashing
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    hash_input.hash(&mut hasher);
    let hash_value = hasher.finish();
    
    // Format as config-{first 8 chars of commit}-{first 8 chars of objectives hash}
    match commit_hash {
        Some(commit) => {
            let commit_prefix = if commit.len() >= 8 { &commit[..8] } else { commit };
            format!("config-{}-{:016x}", commit_prefix, hash_value)
        }
        None => {
            format!("config-uncommitted-{:016x}", hash_value)
        }
    }
}
