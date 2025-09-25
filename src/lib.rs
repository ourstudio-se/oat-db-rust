#![recursion_limit = "512"]

pub mod api;
pub mod config;
pub mod logic;
pub mod model;
pub mod seed;
pub mod store;

// Export API types
pub use api::handlers;
pub use api::routes;

// Export logic types (excluding conflicting merge types)
pub use logic::{
    filter_instances, BranchOperationsV2, Expander, MergeValidationResult, PoolResolver,
    SelectionResult, SimpleEvaluator, SimpleValidator, SolvePipeline, SolvePipelineWithStore,
    ValidationError, ValidationErrorType, ValidationResult, ValidationWarning,
    ValidationWarningType,
};

// Export all model types
pub use model::*;

// Export seed module
pub use seed::*;

// Export store types
pub use store::{PostgresStore, Store};

// Function for integration testing
pub async fn run_server() -> anyhow::Result<()> {
    use axum::serve;
    use std::sync::Arc;
    use tokio::net::TcpListener;

    // Load environment variables from .env file if it exists
    dotenvy::dotenv().ok();

    // Initialize logging with INFO level only (suppress DEBUG logs)
    let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .try_init();

    // Load configuration
    let config = crate::config::AppConfig::load()?;

    // Connect to PostgreSQL
    let database_url = config.database_url()?;
    let postgres_store = crate::store::PostgresStore::new(&database_url).await?;

    // Run migrations
    postgres_store.migrate().await?;

    let store = Arc::new(postgres_store);

    // Create router with state
    let app = crate::api::routes::create_router().with_state(store);

    let bind_address = config.server_address();
    let listener = TcpListener::bind(&bind_address).await?;

    serve(listener, app).await?;

    Ok(())
}

#[cfg(test)]
mod tests {

    #[tokio::test]
    async fn test_relationship_deserialization_issue() {
        use crate::model::RelationshipSelection;
        use serde_json;

        // Test various relationship formats to ensure fix doesn't break other variants

        // 1. Test the user's JSON format (Filter variant)
        let json = r#"{"filter": {"type": ["Color"], "where": {"lt": ["$.price", "50"]}}}"#;
        match serde_json::from_str::<RelationshipSelection>(json) {
            Ok(RelationshipSelection::Filter { .. }) => println!("✓ Filter variant works"),
            Ok(other) => panic!("✗ Filter JSON incorrectly matched: {:?}", other),
            Err(e) => panic!("✗ Filter JSON failed: {}", e),
        }

        // 2. Test SimpleIds variant
        let json = r#"["id1", "id2"]"#;
        match serde_json::from_str::<RelationshipSelection>(json) {
            Ok(RelationshipSelection::SimpleIds(ids)) => {
                assert_eq!(ids, vec!["id1", "id2"]);
                println!("✓ SimpleIds variant works");
            }
            Ok(other) => panic!("✗ SimpleIds JSON incorrectly matched: {:?}", other),
            Err(e) => panic!("✗ SimpleIds JSON failed: {}", e),
        }

        // 3. Test Ids variant
        let json = r#"{"ids": ["id1", "id2"]}"#;
        match serde_json::from_str::<RelationshipSelection>(json) {
            Ok(RelationshipSelection::Ids { ids }) => {
                assert_eq!(ids, vec!["id1", "id2"]);
                println!("✓ Ids variant works");
            }
            Ok(other) => panic!("✗ Ids JSON incorrectly matched: {:?}", other),
            Err(e) => panic!("✗ Ids JSON failed: {}", e),
        }

        // 4. Test PoolBased variant (should still work when explicitly using pool field)
        let json = r#"{"pool": {"type": ["Color"]}, "selection": null}"#;
        match serde_json::from_str::<RelationshipSelection>(json) {
            Ok(RelationshipSelection::PoolBased {
                pool: Some(_),
                selection: None,
            }) => {
                println!("✓ PoolBased variant works");
            }
            Ok(other) => panic!("✗ PoolBased JSON incorrectly matched: {:?}", other),
            Err(e) => panic!("✗ PoolBased JSON failed: {}", e),
        }

        println!("✅ All relationship variants deserialize correctly after fix");
    }

    #[tokio::test]
    async fn test_filter_expr_number_vs_string_issue() {
        use crate::logic::instance_filter::{FilterExpr, InstanceFilterEvaluator, JsonPath};
        use crate::model::Instance;
        use crate::model::{DataType, PropertyValue, TypedValue};
        use serde_json;
        use std::collections::HashMap;

        // Create test instance with numeric price property
        let mut properties = HashMap::new();
        properties.insert(
            "price".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::Value::Number(serde_json::Number::from(75)), // Number: 75
                data_type: DataType::Number,
            }),
        );

        let instance = Instance {
            id: "test-instance".to_string(),
            class_id: "Color".to_string(),
            domain: None,
            properties,
            relationships: HashMap::new(),
            created_by: "test".to_string(),
            created_at: chrono::Utc::now(),
            updated_by: "test".to_string(),
            updated_at: chrono::Utc::now(),
        };

        // Test 1: Number vs Number (should work)
        let filter_number = FilterExpr::Lt {
            lt: (
                JsonPath("$.price".to_string()),
                serde_json::Value::Number(serde_json::Number::from(100)),
            ),
        };
        let result = InstanceFilterEvaluator::evaluate_filter(&instance, &filter_number).unwrap();
        assert!(result, "Number 75 < Number 100 should be true");
        println!("✓ Number vs Number comparison works");

        // Test 2: Number vs String (should now work with fixed comparison)
        let filter_string = FilterExpr::Lt {
            lt: (
                JsonPath("$.price".to_string()),
                serde_json::Value::String("100".to_string()),
            ),
        };
        let result = InstanceFilterEvaluator::evaluate_filter(&instance, &filter_string).unwrap();

        // Should return true because 75 < 100 (numeric comparison)
        assert!(result, "Number 75 should be < String '100' after parsing");
        println!("✓ Number vs String comparison works (75 < '100' = true)");

        // Test 3: User's original case - should work numerically
        let user_filter = FilterExpr::Lt {
            lt: (
                JsonPath("$.price".to_string()),
                serde_json::Value::String("50".to_string()),
            ),
        };
        let result = InstanceFilterEvaluator::evaluate_filter(&instance, &user_filter).unwrap();

        // Should return false because 75 is not < 50 (numeric comparison)
        assert!(
            !result,
            "Number 75 should NOT be < String '50' after parsing"
        );
        println!("✓ User case works correctly (75 < '50' = false, comparison succeeded)");

        // Test 4: String vs Number (reverse case)
        let mut properties2 = HashMap::new();
        properties2.insert(
            "price".to_string(),
            PropertyValue::Literal(TypedValue {
                value: serde_json::Value::String("25".to_string()), // String: "25"
                data_type: DataType::String,
            }),
        );

        let instance2 = Instance {
            id: "test-instance2".to_string(),
            class_id: "Color".to_string(),
            domain: None,
            properties: properties2,
            relationships: HashMap::new(),
            created_by: "test".to_string(),
            created_at: chrono::Utc::now(),
            updated_by: "test".to_string(),
            updated_at: chrono::Utc::now(),
        };

        let filter_mixed = FilterExpr::Lt {
            lt: (
                JsonPath("$.price".to_string()),
                serde_json::Value::Number(serde_json::Number::from(50)),
            ),
        };
        let result = InstanceFilterEvaluator::evaluate_filter(&instance2, &filter_mixed).unwrap();

        // Should return true because "25" parsed as 25 < 50
        assert!(result, "String '25' should be < Number 50 after parsing");
        println!("✓ String vs Number comparison works ('25' < 50 = true)");
    }

    #[tokio::test]
    async fn test_parent_branch_name_model() {
        // This test verifies that parent_branch_name is properly set in the Branch model
        use crate::model::{Branch, Database};

        // Create a test database
        let db = Database::new(
            "test-parent-branch-db".to_string(),
            Some("Test database for parent branch".to_string()),
        );
        let db_id = db.id.clone();

        // Test 1: Main branch should have no parent
        let main_branch = Branch::new_main_branch(db_id.clone(), Some("System".to_string()));
        assert_eq!(
            main_branch.parent_branch_name, None,
            "Main branch should have no parent"
        );

        // Test 2: Feature branch created with new_from_branch should have parent
        let feature_branch = Branch::new_from_branch(
            db_id.clone(),
            "main".to_string(),
            "feature-test".to_string(),
            Some("Test feature branch".to_string()),
            Some("test-user".to_string()),
        );
        assert_eq!(
            feature_branch.parent_branch_name,
            Some("main".to_string()),
            "Feature branch should have main as parent"
        );

        // Test 3: Regular new() branch should have no parent by default
        let regular_branch = Branch::new(
            db_id.clone(),
            "another-branch".to_string(),
            Some("Another branch".to_string()),
            Some("test-user".to_string()),
        );
        assert_eq!(
            regular_branch.parent_branch_name, None,
            "Regular new() branch should have no parent by default"
        );

        println!("✓ Main branch correctly has no parent");
        println!("✓ Feature branch created with new_from_branch has parent");
        println!("✓ Regular branch created with new() has no parent");
    }

    #[tokio::test]
    async fn test_property_default_value_field() {
        // This test verifies that PropertyDef now has a value field for default/constant values
        use crate::model::{DataType, PropertyDef};
        use serde_json;

        // Test 1: PropertyDef with no default value (None)
        let prop_no_default = PropertyDef {
            id: "prop-name".to_string(),
            name: "name".to_string(),
            data_type: DataType::String,
            required: Some(true),
            value: None,
        };
        assert_eq!(
            prop_no_default.value, None,
            "Property should have no default value"
        );

        // Test 2: PropertyDef with string default value
        let prop_string_default = PropertyDef {
            id: "prop-status".to_string(),
            name: "status".to_string(),
            data_type: DataType::String,
            required: Some(false),
            value: Some(serde_json::json!("active")),
        };
        assert_eq!(
            prop_string_default.value,
            Some(serde_json::json!("active")),
            "Property should have string default value"
        );

        // Test 3: PropertyDef with number default value
        let prop_number_default = PropertyDef {
            id: "prop-count".to_string(),
            name: "count".to_string(),
            data_type: DataType::Number,
            required: Some(false),
            value: Some(serde_json::json!(0)),
        };
        assert_eq!(
            prop_number_default.value,
            Some(serde_json::json!(0)),
            "Property should have number default value"
        );

        // Test 4: PropertyDef with boolean default value
        let prop_boolean_default = PropertyDef {
            id: "prop-enabled".to_string(),
            name: "enabled".to_string(),
            data_type: DataType::Boolean,
            required: Some(false),
            value: Some(serde_json::json!(true)),
        };
        assert_eq!(
            prop_boolean_default.value,
            Some(serde_json::json!(true)),
            "Property should have boolean default value"
        );

        // Test 5: Test JSON serialization/deserialization
        let json_str = serde_json::to_string(&prop_string_default).unwrap();
        let parsed: PropertyDef = serde_json::from_str(&json_str).unwrap();
        assert_eq!(
            parsed.value,
            Some(serde_json::json!("active")),
            "Property default value should survive JSON round-trip"
        );

        // Test 6: Test that value field is skipped when None (check JSON doesn't contain "value":null)
        let json_str_no_default = serde_json::to_string(&prop_no_default).unwrap();
        assert!(
            !json_str_no_default.contains("\"value\""),
            "JSON should not contain value field when it's None"
        );

        println!("✓ PropertyDef correctly supports value field");
        println!("✓ Default values work for String, Number, and Boolean types");
        println!("✓ JSON serialization properly handles None values (skip_serializing_if)");
    }

    #[tokio::test]
    async fn test_property_value_field_usage_example() {
        // This test shows how the new value field can be used for constant/default values
        use crate::model::{ClassDef, DataType, PropertyDef};

        // Example: A Product class with a status property that defaults to "active"
        let product_class = ClassDef {
            id: "product".to_string(),
            name: "Product".to_string(),
            description: Some("A product in the catalog".to_string()),
            properties: vec![
                PropertyDef {
                    id: "prop-name".to_string(),
                    name: "name".to_string(),
                    data_type: DataType::String,
                    required: Some(true),
                    value: None, // No default value - must be provided
                },
                PropertyDef {
                    id: "prop-status".to_string(),
                    name: "status".to_string(),
                    data_type: DataType::String,
                    required: Some(false),
                    value: Some(serde_json::json!("active")), // Constant/default value
                },
                PropertyDef {
                    id: "prop-priority".to_string(),
                    name: "priority".to_string(),
                    data_type: DataType::Number,
                    required: Some(false),
                    value: Some(serde_json::json!(1)), // Default priority
                },
                PropertyDef {
                    id: "prop-featured".to_string(),
                    name: "featured".to_string(),
                    data_type: DataType::Boolean,
                    required: Some(false),
                    value: Some(serde_json::json!(false)), // Default to not featured
                },
            ],
            relationships: vec![],
            derived: vec![],
            domain_constraint: crate::model::Domain::binary(),
            base: crate::model::Base::default(),
            created_by: "test".to_string(),
            created_at: chrono::Utc::now(),
            updated_by: "test".to_string(),
            updated_at: chrono::Utc::now(),
        };

        // Test that we can access the default values
        let status_prop = &product_class.properties[1];
        assert_eq!(status_prop.value, Some(serde_json::json!("active")));

        let priority_prop = &product_class.properties[2];
        assert_eq!(priority_prop.value, Some(serde_json::json!(1)));

        let featured_prop = &product_class.properties[3];
        assert_eq!(featured_prop.value, Some(serde_json::json!(false)));

        // Application logic could use these values to provide defaults when creating instances
        println!("✓ Properties can have constant/default values for application use");
        println!("✓ Values are strongly typed using serde_json::Value");
    }

    #[tokio::test]
    async fn test_property_value_field_migration() {
        // This test verifies that existing schemas without the value field are properly normalized
        use crate::model::{DataType, PropertyDef, Schema};

        // Simulate old JSON data without the "value" field (as it would exist in PostgreSQL)
        let old_property_json = r#"{
        "id": "prop-name",
        "name": "name", 
        "data_type": "string",
        "required": true
    }"#;

        // Test that serde can deserialize old data without the value field
        let old_property: PropertyDef = serde_json::from_str(old_property_json).unwrap();
        assert_eq!(
            old_property.value, None,
            "Old property should default to value: None"
        );

        // Create an old-style schema JSON (without value fields)
        let old_schema_json = r#"{
        "id": "old-schema",
        "description": "Schema without value fields",
        "classes": [
            {
                "id": "class-product",
                "name": "Product",
                "description": "Old product class",
                "properties": [
                    {
                        "id": "prop-name",
                        "name": "name",
                        "data_type": "string", 
                        "required": true
                    },
                    {
                        "id": "prop-price",
                        "name": "price",
                        "data_type": "number",
                        "required": false
                    }
                ],
                "relationships": [],
                "derived": [],
                "domain_constraint": {"lower": 0, "upper": 1},
                "created_by": "system",
                "created_at": "2024-01-01T00:00:00Z", 
                "updated_by": "system",
                "updated_at": "2024-01-01T00:00:00Z"
            }
        ]
    }"#;

        // Test deserialization and normalization
        let mut old_schema: Schema = serde_json::from_str(old_schema_json).unwrap();

        // Before normalization - check that value fields are None
        let product_class = &old_schema.classes[0];
        assert_eq!(product_class.properties[0].value, None);
        assert_eq!(product_class.properties[1].value, None);

        // Apply normalization (this should be called automatically in commit.get_data())
        old_schema.normalize();

        // After normalization - values should still be None but explicitly set
        let product_class = &old_schema.classes[0];
        assert_eq!(product_class.properties[0].value, None);
        assert_eq!(product_class.properties[1].value, None);

        // Test that new schemas with value fields work alongside old ones
        let new_property = PropertyDef {
            id: "prop-status".to_string(),
            name: "status".to_string(),
            data_type: DataType::String,
            required: Some(false),
            value: Some(serde_json::json!("active")),
        };

        // Verify the new property retains its value
        assert_eq!(new_property.value, Some(serde_json::json!("active")));

        println!("✓ Old schemas without value field deserialize correctly");
        println!("✓ Schema normalization ensures consistent value field presence");
        println!("✓ Migration from old to new schema format works seamlessly");
    }

    #[tokio::test]
    async fn test_relationship_poolbased_serialization() {
        // This test verifies the serialization behavior of PoolBased relationships
        use crate::model::{Instance, RelationshipSelection};
        use std::collections::HashMap;
        use serde_json;
        
        // Test 1: PoolBased with null pool and selection
        let pool_based = RelationshipSelection::PoolBased {
            pool: None,
            selection: None,
        };
        
        let json = serde_json::to_string(&pool_based).unwrap();
        println!("PoolBased with null pool and selection serializes to: {}", json);
        
        // Due to skip_serializing_if, selection field won't be included when None
        assert_eq!(json, r#"{"pool":null}"#, "Only pool field should be serialized");
        
        // Test 2: Create an instance with this relationship
        let mut relationships = HashMap::new();
        relationships.insert("wheels".to_string(), pool_based);
        
        let instance = Instance {
            id: "bike-001".to_string(),
            class_id: "Bicycle".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships: relationships.clone(),
            created_by: "test".to_string(),
            created_at: chrono::Utc::now(),
            updated_by: "test".to_string(), 
            updated_at: chrono::Utc::now(),
        };
        
        // Serialize the instance
        let instance_json = serde_json::to_value(&instance).unwrap();
        let relationships_json = instance_json.get("relationships").unwrap();
        let wheels_json = relationships_json.get("wheels").unwrap();
        
        println!("Instance relationships field contains: {}", serde_json::to_string_pretty(relationships_json).unwrap());
        
        // Verify the relationship is preserved
        assert!(wheels_json.is_object(), "Relationship should be an object");
        assert!(wheels_json.get("pool").is_some(), "Pool field should be present");
        
        // Test 3: Deserialize back and verify
        let deserialized: Instance = serde_json::from_value(instance_json).unwrap();
        assert_eq!(deserialized.relationships.len(), 1, "Should have one relationship");
        assert!(deserialized.relationships.contains_key("wheels"), "Should have wheels relationship");
        
        match &deserialized.relationships["wheels"] {
            RelationshipSelection::PoolBased { pool, selection } => {
                assert_eq!(pool, &None, "Pool should be None");
                assert_eq!(selection, &None, "Selection should be None");
                println!("✓ PoolBased relationship correctly deserializes back");
            }
            _ => panic!("Expected PoolBased variant"),
        }
        
        println!("✅ PoolBased relationships are preserved through serialization");
    }
    
    #[tokio::test]
    async fn test_relationship_api_deserialization() {
        // This test simulates what happens when the API receives the user's JSON
        use crate::model::{Instance, RelationshipSelection};
        use serde_json;
        
        // Test the exact JSON the user is sending
        let user_json = r#"{
            "id": "bike-001",
            "class": "Bicycle",
            "properties": {
                "name": {"value": "Mountain Explorer", "type": "string"},
                "gear_count": {"value": 21, "type": "number"}
            },
            "relationships": {
                "wheels": {
                    "pool": null,
                    "selection": null
                }
            }
        }"#;
        
        // Try to deserialize as Instance
        let instance_result = serde_json::from_str::<Instance>(user_json);
        
        match instance_result {
            Ok(instance) => {
                println!("✓ User JSON deserializes successfully");
                assert_eq!(instance.id, "bike-001");
                assert_eq!(instance.class_id, "Bicycle");
                assert_eq!(instance.relationships.len(), 1, "Should have one relationship");
                
                // Check the wheels relationship
                match instance.relationships.get("wheels") {
                    Some(RelationshipSelection::PoolBased { pool, selection }) => {
                        assert_eq!(pool, &None);
                        assert_eq!(selection, &None);
                        println!("✓ Wheels relationship is PoolBased with null pool and selection");
                    }
                    Some(other) => panic!("Unexpected relationship variant: {:?}", other),
                    None => panic!("Wheels relationship is missing!"),
                }
            }
            Err(e) => panic!("Failed to deserialize user JSON: {}", e),
        }
        
        // Test another variant - the Filter format the user mentioned
        let filter_json = r#"{
            "id": "car-001",
            "class": "Car", 
            "properties": {},
            "relationships": {
                "color": {
                    "filter": {
                        "type": ["Color"],
                        "where": {
                            "lt": ["$.price", "50"]
                        }
                    }
                }
            }
        }"#;
        
        let filter_result = serde_json::from_str::<Instance>(filter_json);
        match filter_result {
            Ok(instance) => {
                match instance.relationships.get("color") {
                    Some(RelationshipSelection::Filter { filter }) => {
                        assert_eq!(filter.types, Some(vec!["Color".to_string()]));
                        println!("✓ Filter relationship variant works correctly");
                    }
                    Some(other) => panic!("Expected Filter variant, got: {:?}", other),
                    None => panic!("Color relationship is missing!"),
                }
            }
            Err(e) => panic!("Failed to deserialize filter JSON: {}", e),
        }
    }

    #[tokio::test]
    async fn test_working_commit_instance_persistence() {
        // This test simulates the exact flow of creating an instance in working commit
        use crate::model::{Instance, RelationshipSelection, WorkingCommit, Schema, WorkingCommitStatus};
        use std::collections::HashMap;
        use serde_json;
        
        // Create a working commit
        let mut working_commit = WorkingCommit {
            id: "wc-001".to_string(),
            database_id: "db-001".to_string(),
            branch_name: Some("main".to_string()),
            based_on_hash: "hash-001".to_string(),
            author: Some("test".to_string()),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            schema_data: Schema {
                id: "schema-001".to_string(),
                description: Some("Test schema".to_string()),
                classes: vec![],
            },
            instances_data: vec![],
            status: WorkingCommitStatus::Active,
            merge_state: None,
        };
        
        // Create an instance with PoolBased relationship
        let mut relationships = HashMap::new();
        relationships.insert("wheels".to_string(), RelationshipSelection::PoolBased {
            pool: None,
            selection: None,
        });
        
        let instance = Instance {
            id: "bike-001".to_string(),
            class_id: "Bicycle".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships,
            created_by: "test".to_string(),
            created_at: chrono::Utc::now(),
            updated_by: "test".to_string(),
            updated_at: chrono::Utc::now(),
        };
        
        // Add instance to working commit (simulating the handler)
        working_commit.instances_data.push(instance.clone());
        
        // Serialize the working commit (simulating what PostgresStore does)
        let instances_json = serde_json::to_value(&working_commit.instances_data).unwrap();
        println!("Serialized instances: {}", serde_json::to_string_pretty(&instances_json).unwrap());
        
        // Check that the relationship is preserved in JSON
        let first_instance = instances_json.get(0).unwrap();
        let relationships = first_instance.get("relationships").unwrap();
        assert!(relationships.get("wheels").is_some(), "Wheels relationship should exist in JSON");
        
        // Deserialize back (simulating reading from DB)
        let deserialized_instances: Vec<Instance> = serde_json::from_value(instances_json).unwrap();
        assert_eq!(deserialized_instances.len(), 1);
        
        let deserialized_instance = &deserialized_instances[0];
        assert_eq!(deserialized_instance.relationships.len(), 1, "Should have one relationship after deserialization");
        
        match deserialized_instance.relationships.get("wheels") {
            Some(RelationshipSelection::PoolBased { pool, selection }) => {
                assert_eq!(pool, &None);
                assert_eq!(selection, &None);
                println!("✓ PoolBased relationship survives working commit serialization");
            }
            Some(other) => panic!("Unexpected relationship variant after deserialization: {:?}", other),
            None => panic!("Wheels relationship lost during serialization!"),
        }
    }

    #[tokio::test]
    async fn test_classdef_base_backward_compatibility() {
        // This test verifies that ClassDef can deserialize old data without the base field
        use crate::model::{Base, BaseOp, ClassDef};
        
        // Test 1: Old ClassDef JSON without base field should deserialize with default base
        let old_classdef_json = r#"{
            "id": "test-class",
            "name": "TestClass",
            "description": "Test class for backward compatibility",
            "properties": [],
            "relationships": [],
            "derived": [],
            "domain_constraint": {"lower": 0, "upper": 1},
            "created_by": "system",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_by": "system", 
            "updated_at": "2024-01-01T00:00:00Z"
        }"#;
        
        let classdef: ClassDef = serde_json::from_str(old_classdef_json).unwrap();
        assert_eq!(classdef.base.op, BaseOp::All);
        assert_eq!(classdef.base.val, None);
        println!("✓ Old ClassDef without base field deserializes with default base");
        
        // Test 2: New ClassDef JSON with explicit base field
        let new_classdef_json = r#"{
            "id": "test-class-2",
            "name": "TestClass2", 
            "description": "Test class with explicit base",
            "properties": [],
            "relationships": [],
            "derived": [],
            "domain_constraint": {"lower": 0, "upper": 1},
            "base": {"op": "atleast", "val": 3},
            "created_by": "system",
            "created_at": "2024-01-01T00:00:00Z",
            "updated_by": "system",
            "updated_at": "2024-01-01T00:00:00Z"
        }"#;
        
        let classdef2: ClassDef = serde_json::from_str(new_classdef_json).unwrap();
        assert_eq!(classdef2.base.op, BaseOp::AtLeast);
        assert_eq!(classdef2.base.val, Some(3));
        println!("✓ New ClassDef with explicit base field deserializes correctly");
        
        // Test 3: Test serialization includes base field
        let serialized = serde_json::to_string(&classdef).unwrap();
        assert!(serialized.contains("\"base\""));
        assert!(serialized.contains("\"op\":\"all\""));
        println!("✓ Serialized ClassDef includes base field");
        
        // Test 4: Test all BaseOp enum variants serialize correctly
        let test_base_ops = vec![
            ("all", BaseOp::All),
            ("any", BaseOp::Any),
            ("atleast", BaseOp::AtLeast),
            ("atmost", BaseOp::AtMost),
            ("exactly", BaseOp::Exactly),
            ("imply", BaseOp::Imply),
            ("equiv", BaseOp::Equiv),
        ];
        
        for (json_str, expected_op) in test_base_ops {
            let json = format!(r#"{{"op": "{}", "val": 2}}"#, json_str);
            let base: Base = serde_json::from_str(&json).unwrap();
            assert_eq!(base.op, expected_op);
            assert_eq!(base.val, Some(2));
        }
        println!("✓ All BaseOp enum variants deserialize correctly");
        
        // Test 5: Test that val is omitted when None
        let base_no_val = Base {
            op: BaseOp::All,
            val: None,
        };
        let json = serde_json::to_string(&base_no_val).unwrap();
        assert!(!json.contains("\"val\""));
        println!("✓ Base serialization omits val when None");
    }

}
