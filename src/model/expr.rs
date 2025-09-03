use crate::model::Id;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Expr {
    Add {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Sub {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Mul {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Div {
        left: Box<Expr>,
        right: Box<Expr>,
    },
    LitNumber {
        value: f64,
    },
    LitBool {
        value: bool,
    },
    LitString {
        value: String,
    },
    Prop {
        prop: String,
    },
    RelProp {
        rel: String,
        prop: String,
    },
    Sum {
        over: String,
        prop: String,
        r#where: Option<BoolExpr>,
    },
    Count {
        over: String,
        r#where: Option<BoolExpr>,
    },
    If {
        cond: BoolExpr,
        then: Box<Expr>,
        r#else: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolExpr {
    // New simple format: {"all": ["a", "b"]} - checks if relationships 'a' and 'b' exist
    SimpleAll { all: Vec<String> },
    // Original complex format with predicates
    All { predicates: Vec<Predicate> },
    Any { predicates: Vec<Predicate> },
    None { predicates: Vec<Predicate> },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Predicate {
    Has {
        rel: String,
        ids: Option<Vec<Id>>,
        any: Option<bool>,
    },
    PropEq {
        prop: String,
        value: serde_json::Value,
    },
    PropNe {
        prop: String,
        value: serde_json::Value,
    },
    PropGt {
        prop: String,
        value: serde_json::Value,
    },
    PropLt {
        prop: String,
        value: serde_json::Value,
    },
    PropContains {
        prop: String,
        value: String,
    },
    Count {
        rel: String,
        op: crate::model::ComparisonOp,
        value: usize,
    },
    HasTargets {
        rel: String,
        types: Vec<String>,
    },
    IncludesUniverse {
        rel: String,
    },
}
