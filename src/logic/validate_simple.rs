use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::model::{ClassDef, DataType, Id, Instance, PropertyValue, Schema};
use crate::store::traits::Store;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub instance_count: usize,
    pub validated_instances: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub instance_id: String,
    pub error_type: ValidationErrorType,
    pub message: String,
    pub property_name: Option<String>,
    pub expected: Option<String>,
    pub actual: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationWarning {
    pub instance_id: String,
    pub warning_type: ValidationWarningType,
    pub message: String,
    pub property_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationErrorType {
    TypeMismatch,
    MissingRequiredProperty,
    UndefinedProperty,
    InvalidValue,
    ClassNotFound,
    RelationshipError,
    ValueTypeInconsistency,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationWarningType {
    UnusedProperty,
    ConditionalPropertySkipped,
    RelationshipNotValidated,
}

pub struct SimpleValidator;

impl SimpleValidator {
    /// Validate all instances in a branch against the schema
    pub async fn validate_branch<S: Store>(
        store: &S,
        database_id: &Id,
        branch_name: &str,
    ) -> Result<ValidationResult> {
        let mut result = ValidationResult {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            instance_count: 0,
            validated_instances: Vec::new(),
        };

        // Get the schema for this branch
        let schema = match store.get_schema(database_id, branch_name).await? {
            Some(s) => s,
            None => {
                result.valid = false;
                result.errors.push(ValidationError {
                    instance_id: "N/A".to_string(),
                    error_type: ValidationErrorType::ClassNotFound,
                    message: format!("No schema found for branch '{}'", branch_name),
                    property_name: None,
                    expected: None,
                    actual: None,
                });
                return Ok(result);
            }
        };

        // Get all instances for this branch
        let instances = store
            .list_instances_for_branch(database_id, branch_name, None)
            .await?;
        result.instance_count = instances.len();

        // Validate each instance
        for instance in &instances {
            let instance_result = Self::validate_instance(store, instance, &schema).await;
            result.validated_instances.push(instance.id.clone());

            match instance_result {
                Ok(mut inst_result) => {
                    if !inst_result.valid {
                        result.valid = false;
                    }
                    result.errors.append(&mut inst_result.errors);
                    result.warnings.append(&mut inst_result.warnings);
                }
                Err(e) => {
                    result.valid = false;
                    result.errors.push(ValidationError {
                        instance_id: instance.id.clone(),
                        error_type: ValidationErrorType::InvalidValue,
                        message: format!("Validation failed: {}", e),
                        property_name: None,
                        expected: None,
                        actual: None,
                    });
                }
            }
        }

        // Additional validation: Check that all relationships resolve to at least one instance
        for instance in &instances {
            if let Some(class_def) = schema.get_class_by_id(&instance.class_id) {
                Self::validate_relationship_resolution(
                    instance,
                    class_def,
                    &instances,
                    &mut result,
                );
            }
        }

        Ok(result)
    }

    /// Validate a single instance against the schema
    pub async fn validate_instance<S: Store>(
        store: &S,
        instance: &Instance,
        schema: &Schema,
    ) -> Result<ValidationResult> {
        let mut result = ValidationResult {
            valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            instance_count: 1,
            validated_instances: vec![instance.id.clone()],
        };

        // Find the class definition
        let class_def = match schema.get_class_by_id(&instance.class_id) {
            Some(c) => c,
            None => {
                result.valid = false;
                result.errors.push(ValidationError {
                    instance_id: instance.id.clone(),
                    error_type: ValidationErrorType::ClassNotFound,
                    message: format!(
                        "No class definition found for class ID '{}'",
                        instance.class_id
                    ),
                    property_name: None,
                    expected: Some(instance.class_id.clone()),
                    actual: None,
                });
                return Ok(result);
            }
        };

        // Validate properties
        Self::validate_instance_properties(instance, class_def, &mut result);

        // Validate relationships
        Self::validate_instance_relationships(store, instance, class_def, schema, &mut result)
            .await;

        Ok(result)
    }

    /// Legacy method for backward compatibility
    pub async fn validate_instance_basic<S: Store>(
        store: &S,
        instance: &Instance,
        schema: &Schema,
    ) -> Result<()> {
        let result = Self::validate_instance(store, instance, schema).await?;
        if !result.valid {
            let error_messages: Vec<String> =
                result.errors.iter().map(|e| e.message.clone()).collect();
            return Err(anyhow!("Validation failed: {}", error_messages.join(", ")));
        }
        Ok(())
    }

    fn validate_instance_properties(
        instance: &Instance,
        class_def: &ClassDef,
        result: &mut ValidationResult,
    ) {
        // Create lookup maps for both property IDs and names (for backward compatibility)
        let schema_props_by_id: HashMap<String, &crate::model::PropertyDef> = class_def
            .properties
            .iter()
            .map(|p| (p.id.clone(), p))
            .collect();
        let schema_props_by_name: HashMap<String, &crate::model::PropertyDef> = class_def
            .properties
            .iter()
            .map(|p| (p.name.clone(), p))
            .collect();

        // Check for undefined properties (check both ID and name for backward compatibility)
        for prop_key in instance.properties.keys() {
            if !schema_props_by_id.contains_key(prop_key)
                && !schema_props_by_name.contains_key(prop_key)
            {
                result.valid = false;
                result.errors.push(ValidationError {
                    instance_id: instance.id.clone(),
                    error_type: ValidationErrorType::UndefinedProperty,
                    message: format!(
                        "Property '{}' is not defined in class '{}' (checked both ID and name)",
                        prop_key, class_def.name
                    ),
                    property_name: Some(prop_key.clone()),
                    expected: None,
                    actual: Some(prop_key.clone()),
                });
            }
        }

        // Check for missing required properties
        for prop_def in &class_def.properties {
            if prop_def.required.unwrap_or(false) {
                let has_by_id = instance.properties.contains_key(&prop_def.id);
                let has_by_name = instance.properties.contains_key(&prop_def.name);

                if !has_by_id && !has_by_name {
                    result.valid = false;
                    result.errors.push(ValidationError {
                        instance_id: instance.id.clone(),
                        error_type: ValidationErrorType::MissingRequiredProperty,
                        message: format!(
                            "Required property '{}' (ID: {}) is missing",
                            prop_def.name, prop_def.id
                        ),
                        property_name: Some(prop_def.id.clone()),
                        expected: Some(format!("{:?}", prop_def.data_type)),
                        actual: None,
                    });
                }
            }
        }

        // Validate property types and values
        for (prop_key, prop_value) in &instance.properties {
            // Try to find property definition by ID first, then by name
            let prop_def = schema_props_by_id
                .get(prop_key)
                .or_else(|| schema_props_by_name.get(prop_key));

            if let Some(prop_def) = prop_def {
                match prop_value {
                    PropertyValue::Literal(typed_value) => {
                        // Check type compatibility
                        if typed_value.data_type != prop_def.data_type {
                            result.valid = false;
                            result.errors.push(ValidationError {
                                instance_id: instance.id.clone(),
                                error_type: ValidationErrorType::TypeMismatch,
                                message: format!(
                                    "Type mismatch for property '{}' (ID: {}): expected {:?}, found {:?}",
                                    prop_def.name, prop_def.id, prop_def.data_type, typed_value.data_type
                                ),
                                property_name: Some(prop_key.clone()),
                                expected: Some(format!("{:?}", prop_def.data_type)),
                                actual: Some(format!("{:?}", typed_value.data_type)),
                            });
                        }

                        // Check value-type consistency
                        // Skip validation if property is optional (required=false) and value is null
                        let is_optional = prop_def.required.unwrap_or(false) == false;
                        let is_null = typed_value.value.is_null();

                        if !(is_optional && is_null) {
                            if let Err(msg) = Self::validate_value_type_consistency_detailed(
                                &typed_value.value,
                                &typed_value.data_type,
                            ) {
                                result.valid = false;
                                result.errors.push(ValidationError {
                                    instance_id: instance.id.clone(),
                                    error_type: ValidationErrorType::ValueTypeInconsistency,
                                    message: format!(
                                        "Value type inconsistency for property '{}' (ID: {}): {}",
                                        prop_def.name, prop_def.id, msg
                                    ),
                                    property_name: Some(prop_key.clone()),
                                    expected: Some(format!("{:?}", typed_value.data_type)),
                                    actual: Some(format!("{:?}", typed_value.value)),
                                });
                            }
                        }
                    }
                    PropertyValue::Conditional(rule_set) => {
                        // Validate that all relationships referenced in conditional rules exist in the class definition
                        Self::validate_conditional_property_relationships(
                            instance, class_def, prop_key, rule_set, result,
                        );
                    }
                }
            }
        }
    }

    async fn validate_instance_relationships<S: Store>(
        store: &S,
        instance: &Instance,
        class_def: &ClassDef,
        schema: &Schema,
        result: &mut ValidationResult,
    ) {
        // Build maps for both relationship names and IDs for flexible lookup
        let schema_rels_by_name: HashMap<String, &crate::model::RelationshipDef> = class_def
            .relationships
            .iter()
            .map(|r| (r.name.clone(), r))
            .collect();
        let schema_rels_by_id: HashMap<String, &crate::model::RelationshipDef> = class_def
            .relationships
            .iter()
            .map(|r| (r.id.clone(), r))
            .collect();

        // Check for undefined relationships (check both by name and by ID)
        for rel_key in instance.relationships.keys() {
            if !schema_rels_by_name.contains_key(rel_key)
                && !schema_rels_by_id.contains_key(rel_key)
            {
                result.valid = false;
                result.errors.push(ValidationError {
                    instance_id: instance.id.clone(),
                    error_type: ValidationErrorType::RelationshipError,
                    message: format!(
                        "Relationship '{}' is not defined in class '{}' (checked both ID and name)",
                        rel_key, class_def.name
                    ),
                    property_name: Some(rel_key.clone()),
                    expected: None,
                    actual: Some(rel_key.clone()),
                });
            }
        }

        // Validate that relationship target class IDs exist in schema
        for rel_def in &class_def.relationships {
            for target_class_id in &rel_def.targets {
                if schema.get_class_by_id(target_class_id).is_none() {
                    result.valid = false;
                    result.errors.push(ValidationError {
                        instance_id: instance.id.clone(),
                        error_type: ValidationErrorType::ClassNotFound,
                        message: format!(
                            "Relationship '{}' references non-existent class ID '{}'",
                            rel_def.name, target_class_id
                        ),
                        property_name: Some(rel_def.name.clone()),
                        expected: Some("Valid class ID".to_string()),
                        actual: Some(target_class_id.clone()),
                    });
                }
            }
        }

        // Note: Relationship resolution validation is now handled by validate_relationship_resolution()
        // which is called separately with access to all instances. This allows proper pool resolution
        // and filter evaluation. The old warnings about "complex validation not yet implemented" have
        // been removed since we now have full relationship resolution validation.
    }

    /// Validate that all relationships resolve to at least one instance
    /// This is called separately after all instances are validated
    pub fn validate_relationship_resolution(
        instance: &Instance,
        class_def: &ClassDef,
        all_instances: &[Instance],
        result: &mut ValidationResult,
    ) {
        use crate::logic::pool_resolution::{PoolResolver, SelectionResult};

        // Build relationship definition lookup
        let schema_rels_by_name: HashMap<String, &crate::model::RelationshipDef> = class_def
            .relationships
            .iter()
            .map(|r| (r.name.clone(), r))
            .collect();
        let schema_rels_by_id: HashMap<String, &crate::model::RelationshipDef> = class_def
            .relationships
            .iter()
            .map(|r| (r.id.clone(), r))
            .collect();

        // Check each relationship in the instance
        for (rel_key, relationship_selection) in &instance.relationships {
            // Get the relationship definition
            let rel_def = schema_rels_by_name
                .get(rel_key)
                .or_else(|| schema_rels_by_id.get(rel_key));

            if let Some(rel_def) = rel_def {
                // Try to resolve the relationship to see if it produces any instances
                match PoolResolver::resolve_relationship(
                    all_instances,
                    rel_def,
                    relationship_selection,
                ) {
                    Ok(selection_result) => {
                        // Check if the resolved relationship is empty
                        let is_empty = match &selection_result {
                            SelectionResult::Resolved(ids) => ids.is_empty(),
                            SelectionResult::Unresolved(pool) => {
                                // Unresolved with an empty pool is a problem
                                pool.is_empty()
                            }
                        };

                        if is_empty {
                            // Check if the relationship has a minimum quantifier > 0
                            let min_required = match &rel_def.quantifier {
                                crate::model::Quantifier::Range(lower, _) => *lower,
                                crate::model::Quantifier::Exactly(n) => *n,
                                crate::model::Quantifier::AtLeast(n) => *n,
                                crate::model::Quantifier::One => 1,
                                crate::model::Quantifier::Optional => 0,
                                crate::model::Quantifier::AtMost(_) => 0,
                                crate::model::Quantifier::Any => 0,
                                crate::model::Quantifier::All => 0,
                            };

                            if min_required > 0 {
                                // This is an error - relationship requires instances but resolves to none
                                result.valid = false;
                                result.errors.push(ValidationError {
                                    instance_id: instance.id.clone(),
                                    error_type: ValidationErrorType::RelationshipError,
                                    message: format!(
                                        "Relationship '{}' resolves to an empty set but requires at least {} instance(s). Check your relationship filters and available instances.",
                                        rel_key, min_required
                                    ),
                                    property_name: Some(rel_key.clone()),
                                    expected: Some(format!("At least {} instance(s)", min_required)),
                                    actual: Some("0 instances".to_string()),
                                });
                            } else {
                                // Just a warning if not required
                                result.warnings.push(ValidationWarning {
                                    instance_id: instance.id.clone(),
                                    warning_type: ValidationWarningType::RelationshipNotValidated,
                                    message: format!(
                                        "Relationship '{}' resolves to an empty set. This may be intentional if the relationship is optional.",
                                        rel_key
                                    ),
                                    property_name: Some(rel_key.clone()),
                                });
                            }
                        }
                    }
                    Err(e) => {
                        // Failed to resolve - add error
                        result.valid = false;
                        result.errors.push(ValidationError {
                            instance_id: instance.id.clone(),
                            error_type: ValidationErrorType::RelationshipError,
                            message: format!(
                                "Failed to resolve relationship '{}': {}",
                                rel_key, e
                            ),
                            property_name: Some(rel_key.clone()),
                            expected: None,
                            actual: None,
                        });
                    }
                }
            }
        }
    }

    fn validate_value_type_consistency_detailed(
        value: &serde_json::Value,
        declared_type: &DataType,
    ) -> Result<(), String> {
        let is_valid = match (value, declared_type) {
            (serde_json::Value::String(_), DataType::String) => true,
            (serde_json::Value::Number(_), DataType::Number) => true,
            (serde_json::Value::Bool(_), DataType::Boolean) => true,
            (serde_json::Value::Object(_), DataType::Object) => true,
            (serde_json::Value::Array(_), DataType::Array) => true,
            (serde_json::Value::Array(arr), DataType::StringList) => arr
                .iter()
                .all(|v| matches!(v, serde_json::Value::String(_))),
            _ => false,
        };

        if !is_valid {
            return Err(format!(
                "declared as {:?} but JSON value type is {}",
                declared_type,
                match value {
                    serde_json::Value::String(_) => "string",
                    serde_json::Value::Number(_) => "number",
                    serde_json::Value::Bool(_) => "boolean",
                    serde_json::Value::Object(_) => "object",
                    serde_json::Value::Array(_) => "array",
                    serde_json::Value::Null => "null",
                }
            ));
        }

        Ok(())
    }

    fn validate_conditional_property_relationships(
        instance: &Instance,
        class_def: &ClassDef,
        property_id: &str,
        rule_set: &crate::model::RuleSet,
        result: &mut ValidationResult,
    ) {
        let schema_rels: HashMap<String, &crate::model::RelationshipDef> = class_def
            .relationships
            .iter()
            .map(|r| (r.name.clone(), r))
            .collect();

        let branches = match rule_set {
            crate::model::RuleSet::Simple { rules, .. } => rules,
            crate::model::RuleSet::Complex { branches, .. } => branches,
        };

        // Validate each rule branch
        for (branch_index, rule_branch) in branches.iter().enumerate() {
            Self::validate_bool_expr_relationships(
                instance,
                &schema_rels,
                property_id,
                branch_index,
                &rule_branch.when,
                result,
            );
        }
    }

    fn validate_bool_expr_relationships(
        instance: &Instance,
        schema_rels: &HashMap<String, &crate::model::RelationshipDef>,
        property_id: &str,
        branch_index: usize,
        bool_expr: &crate::model::BoolExpr,
        result: &mut ValidationResult,
    ) {
        match bool_expr {
            crate::model::BoolExpr::SimpleAll { all } => {
                // Validate that all referenced relationships exist in the class definition
                for rel_name in all {
                    if !schema_rels.contains_key(rel_name) {
                        result.valid = false;
                        result.errors.push(ValidationError {
                            instance_id: instance.id.clone(),
                            error_type: ValidationErrorType::RelationshipError,
                            message: format!(
                                "Conditional property '{}' rule {} references undefined relationship '{}'",
                                property_id, branch_index + 1, rel_name
                            ),
                            property_name: Some(property_id.to_string()),
                            expected: Some("Defined relationship".to_string()),
                            actual: Some(rel_name.clone()),
                        });
                    }
                }
            }
            crate::model::BoolExpr::All { predicates }
            | crate::model::BoolExpr::Any { predicates }
            | crate::model::BoolExpr::None { predicates } => {
                // Validate predicates that reference relationships
                for predicate in predicates {
                    Self::validate_predicate_relationships(
                        instance,
                        schema_rels,
                        property_id,
                        branch_index,
                        predicate,
                        result,
                    );
                }
            }
        }
    }

    fn validate_predicate_relationships(
        instance: &Instance,
        schema_rels: &HashMap<String, &crate::model::RelationshipDef>,
        property_id: &str,
        branch_index: usize,
        predicate: &crate::model::Predicate,
        result: &mut ValidationResult,
    ) {
        match predicate {
            crate::model::Predicate::Has { rel, .. }
            | crate::model::Predicate::Count { rel, .. }
            | crate::model::Predicate::HasTargets { rel, .. }
            | crate::model::Predicate::IncludesUniverse { rel } => {
                if !schema_rels.contains_key(rel) {
                    result.valid = false;
                    result.errors.push(ValidationError {
                        instance_id: instance.id.clone(),
                        error_type: ValidationErrorType::RelationshipError,
                        message: format!(
                            "Conditional property '{}' rule {} references undefined relationship '{}'",
                            property_id, branch_index + 1, rel
                        ),
                        property_name: Some(property_id.to_string()),
                        expected: Some("Defined relationship".to_string()),
                        actual: Some(rel.clone()),
                    });
                }
            }
            _ => {
                // Other predicate types don't reference relationships
            }
        }
    }
}
