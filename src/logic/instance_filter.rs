use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::{Instance, PropertyValue};

/// Complex filter expression that can be deserialized from JSON
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum FilterExpr {
    /// Logical AND - all conditions must be true
    All { all: Vec<FilterExpr> },
    /// Logical OR - any condition must be true
    Any { any: Vec<FilterExpr> },
    /// Logical NOT - condition must be false
    Not { not: Box<FilterExpr> },
    /// Equality check
    Eq { eq: (JsonPath, Value) },
    /// Not equal check
    Ne { ne: (JsonPath, Value) },
    /// Greater than check
    Gt { gt: (JsonPath, Value) },
    /// Greater than or equal check
    Gte { gte: (JsonPath, Value) },
    /// Less than check
    Lt { lt: (JsonPath, Value) },
    /// Less than or equal check
    Lte { lte: (JsonPath, Value) },
    /// Check if value is in a list
    In { r#in: (JsonPath, Vec<Value>) },
    /// Check if value is not in a list
    NotIn { not_in: (JsonPath, Vec<Value>) },
    /// Check if string contains substring
    Contains { contains: (JsonPath, String) },
    /// Check if property exists
    Exists { exists: JsonPath },
    /// Check if property does not exist
    NotExists { not_exists: JsonPath },
}

/// JSON path for accessing instance properties
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JsonPath(pub String);

impl JsonPath {
    /// Extract value from instance using the path
    pub fn extract(&self, instance: &Instance) -> Result<Option<Value>> {
        let path = &self.0;
        
        // Handle special paths
        if path == "$.__id" || path == "$.id" {
            return Ok(Some(Value::String(instance.id.clone())));
        }
        if path == "$.__type" || path == "$.class_id" {
            return Ok(Some(Value::String(instance.class_id.clone())));
        }
        
        // Handle property paths like $.price or $.material
        if let Some(prop_name) = path.strip_prefix("$.") {
            if let Some(prop_value) = instance.properties.get(prop_name) {
                match prop_value {
                    PropertyValue::Literal(typed_val) => Ok(Some(typed_val.value.clone())),
                    PropertyValue::Conditional(_) => {
                        // For filtering, we don't evaluate conditional properties
                        // They would need evaluation context which we don't have here
                        Ok(None)
                    }
                }
            } else {
                Ok(None)
            }
        } else {
            Err(anyhow!("Invalid JSON path: {}", path))
        }
    }
}

/// Instance filter evaluator
pub struct InstanceFilterEvaluator;

impl InstanceFilterEvaluator {
    /// Filter a list of instances based on the filter expression
    pub fn filter_instances(instances: Vec<Instance>, filter: &FilterExpr) -> Vec<Instance> {
        instances
            .into_iter()
            .filter(|instance| Self::evaluate_filter(instance, filter).unwrap_or(false))
            .collect()
    }
    
    /// Evaluate filter expression against a single instance
    pub fn evaluate_filter(instance: &Instance, filter: &FilterExpr) -> Result<bool> {
        match filter {
            FilterExpr::All { all } => {
                for expr in all {
                    if !Self::evaluate_filter(instance, expr)? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            
            FilterExpr::Any { any } => {
                for expr in any {
                    if Self::evaluate_filter(instance, expr)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            
            FilterExpr::Not { not } => {
                Ok(!Self::evaluate_filter(instance, not)?)
            }
            
            FilterExpr::Eq { eq: (path, value) } => {
                let extracted = path.extract(instance)?;
                Ok(extracted.as_ref() == Some(value))
            }
            
            FilterExpr::Ne { ne: (path, value) } => {
                let extracted = path.extract(instance)?;
                Ok(extracted.as_ref() != Some(value))
            }
            
            FilterExpr::Gt { gt: (path, value) } => {
                let extracted = path.extract(instance)?;
                Self::compare_values(extracted.as_ref(), value, |a, b| a > b)
            }
            
            FilterExpr::Gte { gte: (path, value) } => {
                let extracted = path.extract(instance)?;
                Self::compare_values(extracted.as_ref(), value, |a, b| a >= b)
            }
            
            FilterExpr::Lt { lt: (path, value) } => {
                let extracted = path.extract(instance)?;
                Self::compare_values(extracted.as_ref(), value, |a, b| a < b)
            }
            
            FilterExpr::Lte { lte: (path, value) } => {
                let extracted = path.extract(instance)?;
                Self::compare_values(extracted.as_ref(), value, |a, b| a <= b)
            }
            
            FilterExpr::In { r#in: (path, values) } => {
                let extracted = path.extract(instance)?;
                match extracted {
                    Some(val) => Ok(values.contains(&val)),
                    None => Ok(false),
                }
            }
            
            FilterExpr::NotIn { not_in: (path, values) } => {
                let extracted = path.extract(instance)?;
                match extracted {
                    Some(val) => Ok(!values.contains(&val)),
                    None => Ok(true), // If property doesn't exist, it's not in the list
                }
            }
            
            FilterExpr::Contains { contains: (path, substring) } => {
                let extracted = path.extract(instance)?;
                match extracted {
                    Some(Value::String(s)) => Ok(s.contains(substring)),
                    _ => Ok(false),
                }
            }
            
            FilterExpr::Exists { exists: path } => {
                let extracted = path.extract(instance)?;
                Ok(extracted.is_some())
            }
            
            FilterExpr::NotExists { not_exists: path } => {
                let extracted = path.extract(instance)?;
                Ok(extracted.is_none())
            }
        }
    }
    
    /// Compare two JSON values using a comparison function
    fn compare_values<F>(left: Option<&Value>, right: &Value, cmp: F) -> Result<bool>
    where
        F: Fn(f64, f64) -> bool,
    {
        match (left, right) {
            // Both numbers - direct comparison
            (Some(Value::Number(l)), Value::Number(r)) => {
                match (l.as_f64(), r.as_f64()) {
                    (Some(lf), Some(rf)) => Ok(cmp(lf, rf)),
                    _ => Ok(false),
                }
            }
            // Both strings - try numeric comparison first, fall back to lexicographic
            (Some(Value::String(l)), Value::String(r)) => {
                // Try to parse both as numbers first
                match (l.parse::<f64>(), r.parse::<f64>()) {
                    (Ok(lf), Ok(rf)) => Ok(cmp(lf, rf)),
                    _ => {
                        // Fall back to lexicographic string comparison  
                        Ok(cmp(
                            l.chars().map(|c| c as u32 as f64).sum::<f64>(),
                            r.chars().map(|c| c as u32 as f64).sum::<f64>()
                        ))
                    }
                }
            }
            // Mixed number and string - try to convert string to number
            (Some(Value::Number(l)), Value::String(r)) => {
                match (l.as_f64(), r.parse::<f64>()) {
                    (Some(lf), Ok(rf)) => Ok(cmp(lf, rf)),
                    _ => Ok(false),
                }
            }
            // Mixed string and number - try to convert string to number  
            (Some(Value::String(l)), Value::Number(r)) => {
                match (l.parse::<f64>(), r.as_f64()) {
                    (Ok(lf), Some(rf)) => Ok(cmp(lf, rf)),
                    _ => Ok(false),
                }
            }
            _ => Ok(false),
        }
    }
}

/// Convert from generic JSON value to typed FilterExpr
pub fn parse_filter_expr(value: Value) -> Result<FilterExpr> {
    serde_json::from_value(value)
        .map_err(|e| anyhow!("Failed to parse filter expression: {}", e))
}

/// Filter instances using a strongly-typed filter expression
/// This is the primary API for filtering instances in memory
pub fn filter_instances(instances: Vec<Instance>, filter: &FilterExpr) -> Vec<Instance> {
    InstanceFilterEvaluator::filter_instances(instances, filter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::model::{TypedValue, DataType};
    
    fn create_test_instance(id: &str, class_id: &str, props: Vec<(&str, Value)>) -> Instance {
        let mut properties = HashMap::new();
        for (key, value) in props {
            let typed_value = match &value {
                Value::String(_) => TypedValue {
                    value: value.clone(),
                    data_type: DataType::String,
                },
                Value::Number(_) => TypedValue {
                    value: value.clone(),
                    data_type: DataType::Number,
                },
                Value::Bool(_) => TypedValue {
                    value: value.clone(),
                    data_type: DataType::Boolean,
                },
                _ => TypedValue {
                    value: value.clone(),
                    data_type: DataType::Object,
                },
            };
            properties.insert(key.to_string(), PropertyValue::Literal(typed_value));
        }
        
        Instance {
            id: id.to_string(),
            class_id: class_id.to_string(),
            domain: None,
            properties,
            relationships: HashMap::new(),
            created_by: "test".to_string(),
            created_at: chrono::Utc::now(),
            updated_by: "test".to_string(),
            updated_at: chrono::Utc::now(),
        }
    }
    
    #[test]
    fn test_eq_filter() {
        let instance = create_test_instance("inst1", "Furniture", vec![
            ("price", Value::Number(serde_json::Number::from(150))),
        ]);
        
        let filter = FilterExpr::Eq {
            eq: (JsonPath("$.price".to_string()), Value::Number(serde_json::Number::from(150))),
        };
        
        assert!(InstanceFilterEvaluator::evaluate_filter(&instance, &filter).unwrap());
        
        // Also test the convenience function
        let instances = vec![instance];
        let filtered = filter_instances(instances, &filter);
        assert_eq!(filtered.len(), 1);
    }
    
    #[test]
    fn test_complex_filter() {
        let instance = create_test_instance("inst1", "Furniture", vec![
            ("price", Value::Number(serde_json::Number::from(150))),
            ("material", Value::String("wood".to_string())),
        ]);
        
        let filter = FilterExpr::All {
            all: vec![
                FilterExpr::Eq {
                    eq: (JsonPath("$.__type".to_string()), Value::String("Furniture".to_string())),
                },
                FilterExpr::Gt {
                    gt: (JsonPath("$.price".to_string()), Value::Number(serde_json::Number::from(100))),
                },
                FilterExpr::In {
                    r#in: (
                        JsonPath("$.material".to_string()),
                        vec![
                            Value::String("wood".to_string()),
                            Value::String("metal".to_string()),
                        ],
                    ),
                },
            ],
        };
        
        assert!(InstanceFilterEvaluator::evaluate_filter(&instance, &filter).unwrap());
    }
    
    #[test]
    fn test_parse_from_json() {
        let json = serde_json::json!({
            "all": [
                {"eq": ["$.__type", "Furniture"]},
                {"gt": ["$.price", 100]},
                {"in": ["$.material", ["wood", "metal"]]}
            ]
        });
        
        let filter = parse_filter_expr(json).unwrap();
        
        match filter {
            FilterExpr::All { all } => assert_eq!(all.len(), 3),
            _ => panic!("Expected All expression"),
        }
    }
    
    #[test]
    fn test_direct_json_deserialization() {
        let json_str = r#"{
            "all": [
                {"eq": ["$.__type", "Furniture"]},
                {"gt": ["$.price", 100]}
            ]
        }"#;
        
        let filter: FilterExpr = serde_json::from_str(json_str).unwrap();
        
        let instance = create_test_instance("inst1", "Furniture", vec![
            ("price", Value::Number(serde_json::Number::from(150))),
        ]);
        
        let instances = vec![instance];
        let filtered = filter_instances(instances, &filter);
        assert_eq!(filtered.len(), 1);
    }
}