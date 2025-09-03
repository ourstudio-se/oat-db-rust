use serde::{Deserialize, Serialize};
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
