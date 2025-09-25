use anyhow::{anyhow, Result};

use crate::model::{Expr, Instance, PropertyValue, RuleSet, Schema};

pub struct SimpleEvaluator;

impl SimpleEvaluator {
    pub fn evaluate_rule_set(rule_set: &RuleSet, context: &Instance) -> serde_json::Value {
        let (branches, default) = match rule_set {
            RuleSet::Simple { rules, default } => (rules, default),
            RuleSet::Complex { branches, default } => (branches, default),
        };

        // Evaluate each rule branch in order
        for rule_branch in branches {
            if Self::evaluate_bool_expr(&rule_branch.when, context) {
                return rule_branch.then.clone();
            }
        }

        // Return default value if no rules match, or 0 if no default
        default
            .clone()
            .unwrap_or(serde_json::Value::Number(serde_json::Number::from(0)))
    }

    /// Evaluate a boolean expression against the instance context
    pub fn evaluate_bool_expr(expr: &crate::model::BoolExpr, context: &Instance) -> bool {
        match expr {
            crate::model::BoolExpr::SimpleAll { all } => {
                // Check if all specified relationships exist in the instance
                all.iter().all(|rel_name| {
                    context.relationships.contains_key(rel_name)
                        && !Self::is_relationship_empty(&context.relationships[rel_name])
                })
            }
            crate::model::BoolExpr::All { predicates } => {
                // Original complex predicate evaluation - keep existing behavior for backward compatibility
                predicates
                    .iter()
                    .all(|predicate| Self::evaluate_predicate(predicate, context))
            }
            crate::model::BoolExpr::Any { predicates } => predicates
                .iter()
                .any(|predicate| Self::evaluate_predicate(predicate, context)),
            crate::model::BoolExpr::None { predicates } => !predicates
                .iter()
                .any(|predicate| Self::evaluate_predicate(predicate, context)),
        }
    }

    /// Check if a relationship selection is empty (no targets)
    fn is_relationship_empty(selection: &crate::model::RelationshipSelection) -> bool {
        match selection {
            crate::model::RelationshipSelection::SimpleIds(ids) => ids.is_empty(),
            crate::model::RelationshipSelection::Ids { ids } => ids.is_empty(),
            crate::model::RelationshipSelection::PoolBased { pool: _, selection } => {
                match selection {
                    Some(crate::model::SelectionSpec::Ids(ids)) => ids.is_empty(),
                    Some(crate::model::SelectionSpec::All) => false,
                    Some(crate::model::SelectionSpec::Filter(_)) => false, // Assume filters are non-empty
                    Some(crate::model::SelectionSpec::Unresolved) => true, // Unresolved is considered empty
                    None => true, // No selection means empty
                }
            }
            crate::model::RelationshipSelection::Filter { .. } => false, // Assume filters are non-empty
            crate::model::RelationshipSelection::All => false, // All means non-empty by definition
        }
    }

    /// Evaluate a predicate against the instance context
    fn evaluate_predicate(predicate: &crate::model::Predicate, context: &Instance) -> bool {
        match predicate {
            crate::model::Predicate::Has { rel, ids, any: _ } => {
                if let Some(relationship) = context.relationships.get(rel) {
                    if let Some(required_ids) = ids {
                        // Check if the relationship contains the required IDs
                        match relationship {
                            crate::model::RelationshipSelection::SimpleIds(actual_ids) => {
                                required_ids.iter().all(|id| actual_ids.contains(id))
                            }
                            crate::model::RelationshipSelection::Ids { ids: actual_ids } => {
                                required_ids.iter().all(|id| actual_ids.contains(id))
                            }
                            _ => false, // For filters and All, we'd need more complex evaluation
                        }
                    } else {
                        // Just check if the relationship exists and is non-empty
                        !Self::is_relationship_empty(relationship)
                    }
                } else {
                    false // Relationship doesn't exist
                }
            }
            _ => {
                // TODO: Implement other predicate types as needed
                false
            }
        }
    }

    pub fn get_property_value(instance: &Instance, prop: &str) -> Result<serde_json::Value> {
        match instance.properties.get(prop) {
            Some(PropertyValue::Literal(typed_value)) => Ok(typed_value.value.clone()),
            Some(PropertyValue::Conditional(rule_set)) => {
                Ok(Self::evaluate_rule_set(rule_set, instance))
            }
            None => Err(anyhow!("Property '{}' not found", prop)),
        }
    }

    /// Evaluate derived properties for an instance based on schema definitions
    pub fn evaluate_derived_properties(
        instance: &Instance,
        schema: &Schema,
        requested_properties: &[String],
        configuration: &[Instance], // Pass the full configuration to check domains
    ) -> Result<std::collections::HashMap<String, serde_json::Value>> {
        let mut derived_values = std::collections::HashMap::new();

        // Evaluating derived properties for instance

        // Find the class definition for this instance
        if let Some(class_def) = schema.get_class_by_id(&instance.class_id) {
            // Evaluate each requested derived property
            for derived_prop_name in requested_properties {
                if let Some(derived_def) = class_def
                    .derived
                    .iter()
                    .find(|d| d.name == *derived_prop_name)
                {
                    // Get the expression using the new method that handles fn_short
                    if let Some(expr) = derived_def.get_expr(class_def) {
                        match Self::evaluate_derived_expr(&expr, instance, configuration) {
                            Ok(value) => {
                                derived_values.insert(derived_prop_name.clone(), value);
                            }
                            Err(_e) => {
                                // Log error but continue with other properties
                                // Failed to evaluate derived property
                            }
                        }
                    }
                }
            }
        }

        Ok(derived_values)
    }

    /// Simple derived expression evaluator
    fn evaluate_derived_expr(
        expr: &Expr,
        context: &Instance,
        configuration: &[Instance],
    ) -> Result<serde_json::Value> {
        match expr {
            Expr::LitNumber { value } => Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*value).unwrap(),
            )),
            Expr::Prop { prop } => Self::get_property_value(context, prop),
            Expr::Add { left, right } => {
                let left_val = Self::evaluate_derived_expr(left, context, configuration)?;
                let right_val = Self::evaluate_derived_expr(right, context, configuration)?;

                let left_num = Self::json_to_number(&left_val)?;
                let right_num = Self::json_to_number(&right_val)?;
                let result = left_num + right_num;

                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result).unwrap(),
                ))
            }
            Expr::Sub { left, right } => {
                let left_val = Self::evaluate_derived_expr(left, context, configuration)?;
                let right_val = Self::evaluate_derived_expr(right, context, configuration)?;

                let left_num = Self::json_to_number(&left_val)?;
                let right_num = Self::json_to_number(&right_val)?;
                let result = left_num - right_num;

                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result).unwrap(),
                ))
            }
            Expr::Sum {
                over,
                prop,
                r#where: _,
            } => {
                // Get the relationship
                if let Some(relationship) = context.relationships.get(over) {
                    let instance_ids = match relationship {
                        crate::model::RelationshipSelection::SimpleIds(ids) => ids.clone(),
                        crate::model::RelationshipSelection::Ids { ids } => ids.clone(),
                        _ => {
                            return Ok(serde_json::Value::Null);
                        }
                    };

                    // First, collect all values to determine type
                    let mut values = Vec::new();
                    
                    // Add the instance's own property value
                    if let Ok(own_value) = Self::get_property_value(context, prop) {
                        values.push(own_value);
                    }
                    
                    // Add values from related instances
                    for instance_id in &instance_ids {
                        // Find the instance in the configuration to check its domain
                        if let Some(config_instance) =
                            configuration.iter().find(|inst| inst.id == *instance_id)
                        {
                            // Only include instances that are selected (domain.lower >= 1)
                            if let Some(domain) = &config_instance.domain {
                                if domain.lower >= 1 {
                                    // Instance is selected, include it in the sum
                                    if let Ok(prop_value) =
                                        Self::get_property_value(config_instance, prop)
                                    {
                                        values.push(prop_value);
                                    }
                                }
                            }
                        }
                    }

                    if values.is_empty() {
                        return Ok(serde_json::Value::Null);
                    }

                    // Determine if we should do string concatenation or numeric addition
                    let has_string = values.iter().any(|v| v.is_string());

                    if has_string {
                        // String concatenation mode
                        let concatenated = values
                            .into_iter()
                            .filter_map(|v| match v {
                                serde_json::Value::String(s) => Some(s),
                                serde_json::Value::Number(n) => Some(n.to_string()),
                                serde_json::Value::Bool(b) => Some(b.to_string()),
                                serde_json::Value::Null => None,  // Filter out null values
                                _ => Some(serde_json::to_string(&v).unwrap_or_default()),
                            })
                            .collect::<Vec<String>>()
                            .join("");
                        
                        Ok(serde_json::Value::String(concatenated))
                    } else {
                        // Numeric addition mode
                        let sum = values
                            .into_iter()
                            .map(|v| match v {
                                serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                                serde_json::Value::Bool(true) => 1.0,
                                serde_json::Value::Bool(false) => 0.0,
                                _ => 0.0,
                            })
                            .sum::<f64>();

                        Ok(serde_json::Value::Number(
                            serde_json::Number::from_f64(sum).unwrap_or(serde_json::Number::from(0)),
                        ))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Number(serde_json::Number::from(0))),
        }
    }

    /// Convert JSON value to number
    fn json_to_number(value: &serde_json::Value) -> Result<f64> {
        match value {
            serde_json::Value::Number(n) => {
                n.as_f64().ok_or_else(|| anyhow!("Number is not finite"))
            }
            serde_json::Value::Bool(true) => Ok(1.0),
            serde_json::Value::Bool(false) => Ok(0.0),
            _ => Err(anyhow!("Value is not a number: {:?}", value)),
        }
    }
}
