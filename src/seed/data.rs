use crate::model::{
    Branch, ClassDef, DataType, Database, DefaultPool, DerivedDef, Domain, Expr, Id, Instance,
    PropertyDef, PropertyValue, Quantifier, RelationshipDef, RelationshipSelection, Schema,
    SelectionType,
};
use crate::store::traits::Store;
use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;

/// Helper function to create ClassDef with system audit info
fn create_system_class(
    id: Id,
    name: String,
    description: Option<String>,
    properties: Vec<PropertyDef>,
    relationships: Vec<RelationshipDef>,
    derived: Vec<DerivedDef>,
    domain_constraint: Option<Domain>,
) -> ClassDef {
    let now = Utc::now();
    let system_user = "system".to_string();

    ClassDef {
        id,
        name,
        description,
        properties,
        relationships,
        derived,
        domain_constraint,
        created_by: system_user.clone(),
        created_at: now,
        updated_by: system_user,
        updated_at: now,
    }
}

/// Helper function to create Instance with system audit info
fn create_system_instance(
    id: Id,
    class_id: Id,
    domain: Option<Domain>,
    properties: HashMap<String, PropertyValue>,
    relationships: HashMap<String, RelationshipSelection>,
) -> Instance {
    let now = Utc::now();
    let system_user = "system".to_string();

    Instance {
        id,
        class_id,
        domain,
        properties,
        relationships,
        created_by: system_user.clone(),
        created_at: now,
        updated_by: system_user,
        updated_at: now,
    }
}

/// Helper function to complete ClassDef with system audit info
fn create_system_class_full(mut class_def: ClassDef) -> ClassDef {
    let now = Utc::now();
    let system_user = "system".to_string();

    class_def.created_by = system_user.clone();
    class_def.created_at = now;
    class_def.updated_by = system_user;
    class_def.updated_at = now;

    class_def
}

/// Helper function to complete Instance with system audit info  
fn create_system_instance_full(mut instance: Instance) -> Instance {
    let now = Utc::now();
    let system_user = "system".to_string();

    instance.created_by = system_user.clone();
    instance.created_at = now;
    instance.updated_by = system_user;
    instance.updated_at = now;

    instance
}

pub async fn load_seed_data<S: Store>(store: &S) -> Result<()> {
    // Load original furniture catalog data
    let (database_id, main_branch_id) = create_database_and_main_branch(store).await?;

    // Load main branch data
    load_schema(store, &main_branch_id).await?;
    // Temporarily disabled due to audit field migration
    // load_instances(store, &main_branch_id).await?;

    // Create feature branch with some schema changes
    // Temporarily disabled due to audit field migration
    // let feature_branch_id = create_feature_branch(store, &database_id, &main_branch_id).await?;
    // load_feature_schema(store, &feature_branch_id).await?;
    // load_feature_instances(store, &feature_branch_id).await?;

    // Load furniture workflow data from Postman collection
    // Temporarily disabled due to audit field migration
    // load_furniture_workflow_data(store).await?;

    Ok(())
}

async fn create_database_and_main_branch<S: Store>(store: &S) -> Result<(Id, Id)> {
    // Create database without default branch initially
    let mut database = Database::new_with_id(
        "furniture_catalog".to_string(),
        "Furniture Catalog".to_string(),
        Some("Sample furniture database with beds, fabrics, and components".to_string()),
    );

    let database_id = database.id.clone();

    // Create main branch
    let main_branch = Branch::new_main_branch(
        database_id.clone(),
        Some("System".to_string()), // System as the author of initial setup
    );

    let branch_name = main_branch.name.clone();

    // Set the main branch as default
    database.default_branch_name = branch_name.clone();

    // Save both database and branch
    store.upsert_database(database).await?;
    store.upsert_branch(main_branch).await?;

    Ok((database_id, branch_name))
}

async fn load_schema<S: Store>(store: &S, branch_id: &Id) -> Result<()> {
    // Schema with multiple class definitions
    let furniture_schema = Schema {
        id: "FurnitureCatalogSchema".to_string(),
        // branch_id removed in commit-based architecture
        description: Some(
            "Furniture catalog schema with Underbed, Size, Fabric, and Leg classes".to_string(),
        ),
        classes: vec![
            // Underbed class definition
            ClassDef {
                id: "class-underbed".to_string(),
                name: "Underbed".to_string(),
                description: Some("Under-bed storage furniture".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-underbed-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-underbed-basePrice".to_string(),
                        name: "basePrice".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-underbed-price".to_string(),
                        name: "price".to_string(),
                        data_type: DataType::Number,
                        required: Some(false),
                    },
                ],
                relationships: vec![
                    RelationshipDef {
                        id: "rel-underbed-size".to_string(),
                        name: "size".to_string(),
                        targets: vec!["class-size".to_string()],
                        quantifier: Quantifier::Exactly(1),
                        universe: None,
                        selection: SelectionType::ExplicitOrFilter,
                        default_pool: DefaultPool::All,
                    },
                    RelationshipDef {
                        id: "rel-underbed-fabric".to_string(),
                        name: "fabric".to_string(),
                        targets: vec!["class-fabric".to_string()],
                        quantifier: Quantifier::AtLeast(1),
                        universe: None,
                        selection: SelectionType::FilterAllowed,
                        default_pool: DefaultPool::All,
                    },
                    RelationshipDef {
                        id: "rel-underbed-leg".to_string(),
                        name: "leg".to_string(),
                        targets: vec!["class-leg".to_string()],
                        quantifier: Quantifier::Range(0, 4),
                        universe: None,
                        selection: SelectionType::ExplicitOrFilter,
                        default_pool: DefaultPool::All,
                    },
                ],
                derived: vec![DerivedDef {
                    id: "der-underbed-totalPrice".to_string(),
                    name: "totalPrice".to_string(),
                    data_type: DataType::Number,
                    expr: Expr::Add {
                        left: Box::new(Expr::Prop {
                            prop: "basePrice".to_string(),
                        }),
                        right: Box::new(Expr::Sum {
                            over: "leg".to_string(),
                            prop: "price".to_string(),
                            r#where: None,
                        }),
                    },
                }],
                domain_constraint: Some(Domain::binary()), // Each Underbed instance defaults to domain [0,1]
                created_by: "seed-data".to_string(),
                created_at: chrono::Utc::now(),
                updated_by: "seed-data".to_string(),
                updated_at: chrono::Utc::now(),
            },
            // Size class definition
            ClassDef {
                id: "class-size".to_string(),
                name: "Size".to_string(),
                description: Some("Furniture size dimensions".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-size-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-size-width".to_string(),
                        name: "width".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-size-length".to_string(),
                        name: "length".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                ],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::constant(1)), // Each Size instance defaults to domain [1,1] (always selected)
                created_by: "seed-data".to_string(),
                created_at: chrono::Utc::now(),
                updated_by: "seed-data".to_string(),
                updated_at: chrono::Utc::now(),
            },
            // Fabric class definition
            ClassDef {
                id: "class-fabric".to_string(),
                name: "Fabric".to_string(),
                description: Some("Fabric materials and colors".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-fabric-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-fabric-color".to_string(),
                        name: "color".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-fabric-material".to_string(),
                        name: "material".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                ],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::new(0, 10)), // Each Fabric instance defaults to domain [0,10]
                created_by: "seed-data".to_string(),
                created_at: chrono::Utc::now(),
                updated_by: "seed-data".to_string(),
                updated_at: chrono::Utc::now(),
            },
            // Leg class definition
            ClassDef {
                id: "class-leg".to_string(),
                name: "Leg".to_string(),
                description: Some("Furniture legs and supports".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-leg-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-leg-material".to_string(),
                        name: "material".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-leg-price".to_string(),
                        name: "price".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                ],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::new(0, 4)), // Each Leg instance defaults to domain [0,4]
                created_by: "seed-data".to_string(),
                created_at: chrono::Utc::now(),
                updated_by: "seed-data".to_string(),
                updated_at: chrono::Utc::now(),
            },
        ],
    };

    // TODO: Schema updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!(
        "Seed data must be updated to use working commit system"
    ));
    // store.upsert_schema(furniture_schema).await?;

    // Add Component and Painting classes to the schema
    // let mut extended_schema = store.get_schema(database_id, branch_id).await?.unwrap();

    /*
    // Add Component class
    extended_schema.classes.push(ClassDef {
        id: "class-component".to_string(),
        name: "Component".to_string(),
        description: Some("Generic component for paintings".to_string()),
        properties: vec![
            PropertyDef {
                id: "prop-component-name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "prop-component-type".to_string(),
                name: "componentType".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
        ],
        relationships: vec![],
        derived: vec![],
        domain_constraint: Some(Domain::binary()), // Each Component instance defaults to domain [0,1]
    });

    // Add Painting class
    extended_schema.classes.push(ClassDef {
        id: "class-painting".to_string(),
        name: "Painting".to_string(),
        description: Some("Painting with conditional pricing based on components".to_string()),
        properties: vec![
            PropertyDef {
                id: "prop-painting-name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "prop-painting-price".to_string(),
                name: "price".to_string(),
                data_type: DataType::Number,
                required: Some(false),
            },
        ],
        relationships: vec![
            RelationshipDef {
                id: "rel-painting-a".to_string(),
                name: "a".to_string(),
                targets: vec!["class-component".to_string()],
                quantifier: Quantifier::AtLeast(0),
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                default_pool: DefaultPool::All,
            },
            RelationshipDef {
                id: "rel-painting-b".to_string(),
                name: "b".to_string(),
                targets: vec!["class-component".to_string()],
                quantifier: Quantifier::AtLeast(0),
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                default_pool: DefaultPool::All,
            },
            RelationshipDef {
                id: "rel-painting-c".to_string(),
                name: "c".to_string(),
                targets: vec!["class-component".to_string()],
                quantifier: Quantifier::AtLeast(0),
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                default_pool: DefaultPool::All,
            },
        ],
        derived: vec![],
        domain_constraint: Some(Domain::binary()), // Each Painting instance defaults to domain [0,1]
    });

    // Add Car, Color, and Option classes for pool resolution example
    extended_schema.classes.push(ClassDef {
        id: "class-color".to_string(),
        name: "Color".to_string(),
        description: Some("Car color with pricing".to_string()),
        properties: vec![
            PropertyDef {
                id: "prop-color-name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "prop-color-price".to_string(),
                name: "price".to_string(),
                data_type: DataType::Number,
                required: Some(true),
            },
        ],
        relationships: vec![],
        derived: vec![],
        domain_constraint: Some(Domain::constant(1)), // Each Color instance defaults to domain [1,1] (always selected)
    });

    extended_schema.classes.push(ClassDef {
        id: "class-option".to_string(),
        name: "Option".to_string(),
        description: Some("Car option/accessory".to_string()),
        properties: vec![
            PropertyDef {
                id: "prop-option-name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "prop-option-price".to_string(),
                name: "price".to_string(),
                data_type: DataType::Number,
                required: Some(true),
            },
        ],
        relationships: vec![],
        derived: vec![],
        domain_constraint: Some(Domain::binary()), // Each Option instance defaults to domain [0,1]
    });

    extended_schema.classes.push(ClassDef {
        id: "class-car".to_string(),
        name: "Car".to_string(),
        description: Some(
            "Car with configurable color and options - demonstrates pool resolution".to_string(),
        ),
        properties: vec![PropertyDef {
            id: "prop-car-model".to_string(),
            name: "model".to_string(),
            data_type: DataType::String,
            required: Some(true),
        }],
        relationships: vec![
            RelationshipDef {
                id: "rel-car-color".to_string(),
                name: "color".to_string(),
                targets: vec!["class-color".to_string()],
                quantifier: Quantifier::Exactly(1),
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                // Default: all Color instances are in the pool
                default_pool: DefaultPool::All,
            },
            RelationshipDef {
                id: "rel-car-free-options".to_string(),
                name: "freeOptions".to_string(),
                targets: vec!["class-option".to_string()],
                quantifier: Quantifier::AtLeast(0),
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                // Default: no Option instances in pool (must be explicitly selected)
                default_pool: DefaultPool::None,
            },
        ],
        derived: vec![],
        domain_constraint: Some(Domain::binary()), // Each Car instance defaults to domain [0,1]
    });
    */

    // TODO: Schema updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!(
        "Seed data must be updated to use working commit system"
    ));
    // store.upsert_schema(extended_schema).await?;

    Ok(())
}

#[allow(dead_code)]
async fn load_instances<S: Store>(_store: &S, _branch_id: &Id) -> Result<()> {
    return Err(anyhow::anyhow!(
        "Function disabled during audit field migration"
    ));
}

/// Creates a feature branch from the main branch for complex demonstration scenarios
/*
    let small_size = create_system_instance_full(Instance {
        id: "size-small".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-size".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Small".to_string())),
            );
            props.insert(
                "width".to_string(),
                PropertyValue::Literal(TypedValue::number(90)),
            );
            props.insert(
                "length".to_string(),
                PropertyValue::Literal(TypedValue::number(200)),
            );
            props
        },
        relationships: HashMap::new(),
    });

    let medium_size = create_system_instance_full(Instance {
        id: "size-medium".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-size".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Medium".to_string())),
            );
            props.insert(
                "width".to_string(),
                PropertyValue::Literal(TypedValue::number(120)),
            );
            props.insert(
                "length".to_string(),
                PropertyValue::Literal(TypedValue::number(200)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let cotton_fabric = create_system_instance_full(Instance {
        id: "fabric-cotton-white".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-fabric".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Cotton White".to_string())),
            );
            props.insert(
                "color".to_string(),
                PropertyValue::Literal(TypedValue::string("White".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Cotton".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let linen_fabric = create_system_instance_full(Instance {
        id: "fabric-linen-beige".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-fabric".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Linen Beige".to_string())),
            );
            props.insert(
                "color".to_string(),
                PropertyValue::Literal(TypedValue::string("Beige".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Linen".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg = create_system_instance_full(Instance {
        id: "leg-wooden".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg_2 = create_system_instance_full(Instance {
        id: "leg-wooden-2".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg #2".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg_3 = create_system_instance_full(Instance {
        id: "leg-wooden-3".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg #3".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg_4 = create_system_instance_full(Instance {
        id: "leg-wooden-4".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg #4".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let metal_leg = create_system_instance_full(Instance {
        id: "leg-metal".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Metal Leg".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Metal".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(35)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let delux_underbed = Instance {
        id: "delux-underbed".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-underbed".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Delux Underbed".to_string())),
            );
            props.insert(
                "basePrice".to_string(),
                PropertyValue::Literal(TypedValue::number(200)),
            );

            props.insert(
                "price".to_string(),
                PropertyValue::Conditional(RuleSet::Complex {
                    branches: vec![
                        RuleBranch {
                            when: BoolExpr::All {
                                predicates: vec![Predicate::Has {
                                    rel: "size".to_string(),
                                    ids: Some(vec!["size-small".to_string()]),
                                    any: None,
                                }],
                            },
                            then: serde_json::Value::Number(serde_json::Number::from(180)),
                        },
                        RuleBranch {
                            when: BoolExpr::All {
                                predicates: vec![Predicate::Has {
                                    rel: "size".to_string(),
                                    ids: Some(vec!["size-medium".to_string()]),
                                    any: None,
                                }],
                            },
                            then: serde_json::Value::Number(serde_json::Number::from(220)),
                        },
                    ],
                    default: Some(serde_json::Value::Number(serde_json::Number::from(200))),
                }),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();
            rels.insert(
                "size".to_string(),
                RelationshipSelection::Ids {
                    ids: vec!["size-medium".to_string()],
                },
            );
            rels.insert(
                "fabric".to_string(),
                RelationshipSelection::Filter {
                    filter: crate::model::InstanceFilter {
                        types: Some(vec!["Fabric".to_string()]),
                        where_clause: Some(BoolExpr::All {
                            predicates: vec![Predicate::PropEq {
                                prop: "material".to_string(),
                                value: serde_json::Value::String("Cotton".to_string()),
                            }],
                        }),
                        sort: None,
                        limit: None,
                    },
                },
            );
            rels.insert(
                "leg".to_string(),
                RelationshipSelection::Ids {
                    ids: vec![
                        "leg-wooden".to_string(),
                        "leg-wooden-2".to_string(),
                        "leg-wooden-3".to_string(),
                        "leg-wooden-4".to_string(),
                    ],
                },
            );
            rels
        },
    };

    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(small_size).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(medium_size).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(cotton_fabric).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(linen_fabric).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg_2).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg_3).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg_4).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(metal_leg).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(delux_underbed).await?;

    // Create Component instances for painting example
    let comp_a = Instance {
        id: "comp-a".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-component".to_string(),
        domain: Some(Domain::new(1, 5)), // Range domain [1,5] - must have 1-5 instances
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Component A".to_string())),
            );
            props.insert(
                "componentType".to_string(),
                PropertyValue::Literal(TypedValue::string("Primary".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let comp_b = Instance {
        id: "comp-b".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-component".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Component B".to_string())),
            );
            props.insert(
                "componentType".to_string(),
                PropertyValue::Literal(TypedValue::string("Secondary".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let comp_c = Instance {
        id: "comp-c".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-component".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Component C".to_string())),
            );
            props.insert(
                "componentType".to_string(),
                PropertyValue::Literal(TypedValue::string("Accent".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    // Create Painting instance with conditional pricing based on your example
    let painting1 = Instance {
        id: "painting1".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-painting".to_string(),
        domain: Some(Domain::constant(1)), // Constant domain [1,1] - always included
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Abstract Composition".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Conditional(RuleSet::Simple {
                    rules: vec![
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "b".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(100.0).unwrap(),
                            ),
                        },
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "c".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(110.0).unwrap(),
                            ),
                        },
                    ],
                    default: Some(serde_json::Value::Number(serde_json::Number::from(0))),
                }),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();
            rels.insert(
                "a".to_string(),
                RelationshipSelection::SimpleIds(vec!["comp-a".to_string()]),
            );
            rels.insert(
                "b".to_string(),
                RelationshipSelection::SimpleIds(vec!["comp-b".to_string()]),
            );
            rels
        },
    };

    // Create another painting with different relationships (a + c) to test the second rule
    let painting2 = Instance {
        id: "painting2".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-painting".to_string(),
        domain: Some(Domain::new(0, 3)), // Range domain [0,3] - can have 0-3 copies
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Modern Vista".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Conditional(RuleSet::Simple {
                    rules: vec![
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "b".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(100.0).unwrap(),
                            ),
                        },
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "c".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(110.0).unwrap(),
                            ),
                        },
                    ],
                    default: Some(serde_json::Value::Number(serde_json::Number::from(0))),
                }),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();
            rels.insert(
                "a".to_string(),
                RelationshipSelection::SimpleIds(vec!["comp-a".to_string()]),
            );
            rels.insert(
                "c".to_string(),
                RelationshipSelection::SimpleIds(vec!["comp-c".to_string()]),
            );
            rels
        },
    };

    // Create a painting with no matching relationships (should default to 0)
    let painting3 = Instance {
        id: "painting3".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-painting".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Minimalist Study".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Conditional(RuleSet::Simple {
                    rules: vec![
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "b".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(100.0).unwrap(),
                            ),
                        },
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "c".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(110.0).unwrap(),
                            ),
                        },
                    ],
                    default: Some(serde_json::Value::Number(serde_json::Number::from(0))),
                }),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();
            // Only has relationship 'a' - no matching rules, should default to 0
            rels.insert(
                "a".to_string(),
                RelationshipSelection::SimpleIds(vec!["comp-a".to_string()]),
            );
            rels
        },
    };

    // Store all component and painting instances
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(comp_a).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(comp_b).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(comp_c).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(painting1).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(painting2).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(painting3).await?;

    // Create Car/Color/Option example instances for pool resolution testing

    // Color instances
    let red_color = Instance {
        id: "color-red".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-color".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Red".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(50)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let blue_color = Instance {
        id: "color-blue".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-color".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Blue".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(75)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let expensive_color = Instance {
        id: "color-gold".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-color".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Gold".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(150)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    // Option instances
    let gps_option = Instance {
        id: "option-gps".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-option".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("GPS Navigation".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(300)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let sunroof_option = Instance {
        id: "option-sunroof".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-option".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Sunroof".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(800)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    // Car instance showing different pool resolution strategies
    let car_001 = Instance {
        id: "car-001".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-car".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "model".to_string(),
                PropertyValue::Literal(TypedValue::string("Sedan".to_string())),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();
            // Color relationship with pool filter override (only colors under $100)
            rels.insert(
                "color".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Color".to_string()]),
                        where_clause: Some(BoolExpr::All {
                            predicates: vec![Predicate::PropLt {
                                prop: "price".to_string(),
                                value: serde_json::Value::Number(serde_json::Number::from(100)),
                            }],
                        }),
                        sort: None,
                        limit: None,
                    }),
                    selection: None, // Unresolved - to be chosen by solver/user
                },
            );
            // Free options - uses schema default (none), explicitly selects GPS
            rels.insert(
                "freeOptions".to_string(),
                RelationshipSelection::PoolBased {
                    pool: None, // Use schema default (none)
                    selection: Some(SelectionSpec::Ids(vec!["option-gps".to_string()])),
                },
            );
            rels
        },
    };

    // Store Car/Color/Option instances
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(red_color).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(blue_color).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(expensive_color).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(gps_option).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(sunroof_option).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(car_001).await?;

    // Additional pool resolution examples to showcase more complex scenarios

    // Create a luxury car with different pool resolution strategies
    let car_002 = Instance {
        id: "car-002".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-car".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "model".to_string(),
                PropertyValue::Literal(TypedValue::string("Luxury SUV".to_string())),
            );
            props
        },
        relationships: {
            let mut relationships = HashMap::new();

            // Color relationship: Use schema default (All) - no pool override
            relationships.insert(
                "color".to_string(),
                RelationshipSelection::PoolBased {
                    pool: None, // No pool override - use schema default (All)
                    selection: Some(SelectionSpec::Ids(vec!["color-gold".to_string()])), // Luxury car gets gold
                },
            );

            // Free options: Override default (None) with custom pool that includes expensive options
            relationships.insert(
                "freeOptions".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Option".to_string()]),
                        where_clause: Some(BoolExpr::All {
                            predicates: vec![Predicate::PropGt {
                                prop: "price".to_string(),
                                value: serde_json::Value::Number(serde_json::Number::from(500)),
                            }],
                        }),
                        sort: None,
                        limit: None,
                    }),
                    selection: Some(SelectionSpec::Ids(vec!["option-sunroof".to_string()])), // Luxury option
                },
            );

            relationships
        },
    };

    // Create an economy car demonstrating minimal pool usage
    let car_003 = Instance {
        id: "car-003".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-car".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "model".to_string(),
                PropertyValue::Literal(TypedValue::string("Economy Hatchback".to_string())),
            );
            props
        },
        relationships: {
            let mut relationships = HashMap::new();

            // Color: Custom pool with only budget colors (under $100)
            relationships.insert(
                "color".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Color".to_string()]),
                        where_clause: Some(BoolExpr::All {
                            predicates: vec![Predicate::PropLt {
                                prop: "price".to_string(),
                                value: serde_json::Value::Number(serde_json::Number::from(100)),
                            }],
                        }),
                        sort: Some("price ASC".to_string()),
                        limit: Some(2), // Only cheapest 2 colors
                    }),
                    selection: Some(SelectionSpec::Ids(vec!["color-red".to_string()])), // Choose cheapest
                },
            );

            // Free options: Keep default empty pool (None) - no free options for economy car
            relationships.insert(
                "freeOptions".to_string(),
                RelationshipSelection::PoolBased {
                    pool: None,      // Use schema default (None - empty pool)
                    selection: None, // No selection - no free options
                },
            );

            relationships
        },
    };

    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(car_002).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(car_003).await?;

    // Add a painting with NO relationships to clearly show default value behavior
    let painting_minimal = Instance {
        id: "painting-minimal".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-painting".to_string(),
        domain: Some(Domain::binary()), // Binary domain [0,1] - can be included or excluded
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string(
                    "Minimal Painting - No Components".to_string(),
                )),
            );
            // Same conditional property structure as other paintings
            props.insert(
                "price".to_string(),
                PropertyValue::Conditional(RuleSet::Simple {
                    rules: vec![
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "b".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(100.0).unwrap(),
                            ),
                        },
                        RuleBranch {
                            when: BoolExpr::SimpleAll {
                                all: vec!["a".to_string(), "c".to_string()],
                            },
                            then: serde_json::Value::Number(
                                serde_json::Number::from_f64(110.0).unwrap(),
                            ),
                        },
                    ],
                    default: Some(serde_json::Value::Number(
                        serde_json::Number::from_f64(25.0).unwrap(),
                    )),
                }),
            );
            props
        },
        relationships: HashMap::new(), // NO relationships - should use default value
    };

    // Add a car with very explicit pool filtering to demonstrate the feature clearly
    let car_demo = Instance {
        id: "car-demo-pools".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-car".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "model".to_string(),
                PropertyValue::Literal(TypedValue::string(
                    "Demo Car - Shows Pool Filtering".to_string(),
                )),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();

            // Demonstrate DefaultPool::All with NO override - should get all colors
            rels.insert(
                "color".to_string(),
                RelationshipSelection::PoolBased {
                    pool: None, // Use schema default (All colors)
                    selection: Some(SelectionSpec::Ids(vec!["color-blue".to_string()])), // Select blue from all available
                },
            );

            // Demonstrate DefaultPool::None with explicit pool override - normally no options available
            rels.insert(
                "freeOptions".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Option".to_string()]),
                        where_clause: Some(BoolExpr::All {
                            predicates: vec![Predicate::PropGt {
                                prop: "price".to_string(),
                                value: serde_json::Value::Number(serde_json::Number::from(600)), // Only expensive options (>$600)
                            }],
                        }),
                        sort: Some("price DESC".to_string()), // Most expensive first
                        limit: Some(1),                       // Only the most expensive option
                    }),
                    selection: Some(SelectionSpec::Ids(vec!["option-sunroof".to_string()])), // Should select expensive sunroof
                },
            );

            rels
        },
    };

    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(painting_minimal).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(car_demo).await?;

    Ok(())
}

// Create a feature branch for testing merge functionality
async fn create_feature_branch<S: Store>(
    store: &S,
    database_id: &Id,
    main_branch_id: &Id,
) -> Result<Id> {
    let mut feature_branch = Branch::new(
        database_id.clone(),
        "feature-add-materials".to_string(),
        Some("Feature branch to add material property to existing classes".to_string()),
        Some("Developer".to_string()),
    );

    // Set the parent branch name manually since Branch::new doesn't support it directly
    feature_branch.parent_branch_name = Some("main".to_string());

    let feature_branch_name = feature_branch.name.clone();
    store.upsert_branch(feature_branch).await?;

    Ok(feature_branch_name)
}

// Load modified schema for feature branch (adds material property to Underbed class)
async fn load_feature_schema<S: Store>(store: &S, branch_id: &Id) -> Result<()> {
    // Enhanced schema with additional required material property on Underbed
    let furniture_schema = Schema {
        id: "FurnitureCatalogSchema".to_string(),
        // branch_id removed in commit-based architecture
        description: Some("Enhanced furniture catalog schema with material properties".to_string()),
        classes: vec![
            // Enhanced Underbed class with NEW required material property
            ClassDef {
                id: "class-underbed".to_string(),
                name: "Underbed".to_string(),
                description: Some(
                    "Under-bed storage furniture with material specifications".to_string(),
                ),
                properties: vec![
                    PropertyDef {
                        id: "prop-underbed-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-underbed-basePrice".to_string(),
                        name: "basePrice".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-underbed-price".to_string(),
                        name: "price".to_string(),
                        data_type: DataType::Number,
                        required: Some(false),
                    },
                    // NEW REQUIRED PROPERTY - This will cause validation conflicts!
                    PropertyDef {
                        id: "prop-underbed-material".to_string(),
                        name: "material".to_string(),
                        data_type: DataType::String,
                        required: Some(true), // Required! Main branch instances don't have this
                    },
                ],
                relationships: vec![
                    RelationshipDef {
                        id: "rel-underbed-size".to_string(),
                        name: "size".to_string(),
                        targets: vec!["class-size".to_string()],
                        quantifier: Quantifier::Exactly(1),
                        universe: None,
                        selection: SelectionType::ExplicitOrFilter,
                        default_pool: DefaultPool::All,
                    },
                    RelationshipDef {
                        id: "rel-underbed-fabric".to_string(),
                        name: "fabric".to_string(),
                        targets: vec!["class-fabric".to_string()],
                        quantifier: Quantifier::AtLeast(1),
                        universe: None,
                        selection: SelectionType::FilterAllowed,
                        default_pool: DefaultPool::All,
                    },
                    RelationshipDef {
                        id: "rel-underbed-leg".to_string(),
                        name: "leg".to_string(),
                        targets: vec!["class-leg".to_string()],
                        quantifier: Quantifier::Range(0, 4),
                        universe: None,
                        selection: SelectionType::ExplicitOrFilter,
                        default_pool: DefaultPool::All,
                    },
                ],
                derived: vec![DerivedDef {
                    id: "der-underbed-totalPrice".to_string(),
                    name: "totalPrice".to_string(),
                    data_type: DataType::Number,
                    expr: Expr::Add {
                        left: Box::new(Expr::Prop {
                            prop: "basePrice".to_string(),
                        }),
                        right: Box::new(Expr::Sum {
                            over: "leg".to_string(),
                            prop: "price".to_string(),
                            r#where: None,
                        }),
                    },
                }],
                domain_constraint: Some(Domain::binary()), // Each Underbed instance defaults to domain [0,1]
            },
            // Size class unchanged
            ClassDef {
                id: "class-size".to_string(),
                name: "Size".to_string(),
                description: Some("Furniture size dimensions".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-size-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-size-width".to_string(),
                        name: "width".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-size-length".to_string(),
                        name: "length".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                ],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::constant(1)), // Each Size instance defaults to domain [1,1] (always selected)
            },
            // Enhanced Fabric class with additional optional property
            ClassDef {
                id: "class-fabric".to_string(),
                name: "Fabric".to_string(),
                description: Some("Fabric materials and colors with durability rating".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-fabric-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-fabric-color".to_string(),
                        name: "color".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-fabric-material".to_string(),
                        name: "material".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    // NEW OPTIONAL PROPERTY - This won't cause conflicts
                    PropertyDef {
                        id: "prop-fabric-durability".to_string(),
                        name: "durability".to_string(),
                        data_type: DataType::String,
                        required: Some(false), // Optional, so safe
                    },
                ],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::new(0, 10)), // Each Fabric instance defaults to domain [0,10]
            },
            // Leg class unchanged
            ClassDef {
                id: "class-leg".to_string(),
                name: "Leg".to_string(),
                description: Some("Furniture legs and supports".to_string()),
                properties: vec![
                    PropertyDef {
                        id: "prop-leg-name".to_string(),
                        name: "name".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-leg-material".to_string(),
                        name: "material".to_string(),
                        data_type: DataType::String,
                        required: Some(true),
                    },
                    PropertyDef {
                        id: "prop-leg-price".to_string(),
                        name: "price".to_string(),
                        data_type: DataType::Number,
                        required: Some(true),
                    },
                ],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::new(0, 4)), // Each Leg instance defaults to domain [0,4]
                created_by: "seed-data".to_string(),
                created_at: chrono::Utc::now(),
                updated_by: "seed-data".to_string(),
                updated_at: chrono::Utc::now(),
            },
        ],
    };

    // TODO: Schema updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_schema(furniture_schema).await?;
    Ok(())
}
*/
// Load instances for feature branch - includes new instances with material properties
#[allow(dead_code)]
async fn load_feature_instances<S: Store>(_store: &S, _branch_id: &Id) -> Result<()> {
    return Err(anyhow::anyhow!(
        "Function disabled during audit field migration"
    ));
    /*
    // Copy main branch instances but update branch_id
    let small_size = create_system_instance_full(Instance {
        id: "size-small".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-size".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Small".to_string())),
            );
            props.insert(
                "width".to_string(),
                PropertyValue::Literal(TypedValue::number(90)),
            );
            props.insert(
                "length".to_string(),
                PropertyValue::Literal(TypedValue::number(200)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let medium_size = create_system_instance_full(Instance {
        id: "size-medium".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-size".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Medium".to_string())),
            );
            props.insert(
                "width".to_string(),
                PropertyValue::Literal(TypedValue::number(120)),
            );
            props.insert(
                "length".to_string(),
                PropertyValue::Literal(TypedValue::number(200)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    // Enhanced fabric with durability property
    let cotton_fabric = create_system_instance_full(Instance {
        id: "fabric-cotton-white".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-fabric".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Cotton White".to_string())),
            );
            props.insert(
                "color".to_string(),
                PropertyValue::Literal(TypedValue::string("White".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Cotton".to_string())),
            );
            // NEW property in feature branch
            props.insert(
                "durability".to_string(),
                PropertyValue::Literal(TypedValue::string("High".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let linen_fabric = create_system_instance_full(Instance {
        id: "fabric-linen-beige".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-fabric".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Linen Beige".to_string())),
            );
            props.insert(
                "color".to_string(),
                PropertyValue::Literal(TypedValue::string("Beige".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Linen".to_string())),
            );
            // NEW property in feature branch
            props.insert(
                "durability".to_string(),
                PropertyValue::Literal(TypedValue::string("Medium".to_string())),
            );
            props
        },
        relationships: HashMap::new(),
    };

    // Copy leg instances (unchanged)
    let wooden_leg = create_system_instance_full(Instance {
        id: "leg-wooden".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg_2 = create_system_instance_full(Instance {
        id: "leg-wooden-2".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg #2".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg_3 = create_system_instance_full(Instance {
        id: "leg-wooden-3".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg #3".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let wooden_leg_4 = create_system_instance_full(Instance {
        id: "leg-wooden-4".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Wooden Leg #4".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Wood".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(25)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    let metal_leg = create_system_instance_full(Instance {
        id: "leg-metal".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-leg".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Metal Leg".to_string())),
            );
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Metal".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(35)),
            );
            props
        },
        relationships: HashMap::new(),
    };

    // Enhanced Underbed instance WITH the new required material property
    let delux_underbed_enhanced = create_system_instance_full(Instance {
        id: "delux-underbed-enhanced".to_string(), // Different ID to avoid conflicts
        // branch_id removed in commit-based architecture
        class_id: "class-underbed".to_string(),
        domain: None,
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Delux Underbed Enhanced".to_string())),
            );
            props.insert(
                "basePrice".to_string(),
                PropertyValue::Literal(TypedValue::number(200)),
            );
            // NEW REQUIRED property provided
            props.insert(
                "material".to_string(),
                PropertyValue::Literal(TypedValue::string("Engineered Wood".to_string())),
            );

            props.insert(
                "price".to_string(),
                PropertyValue::Conditional(RuleSet::Complex {
                    branches: vec![
                        RuleBranch {
                            when: BoolExpr::All {
                                predicates: vec![Predicate::Has {
                                    rel: "size".to_string(),
                                    ids: Some(vec!["size-small".to_string()]),
                                    any: None,
                                }],
                            },
                            then: serde_json::Value::Number(serde_json::Number::from(180)),
                        },
                        RuleBranch {
                            when: BoolExpr::All {
                                predicates: vec![Predicate::Has {
                                    rel: "size".to_string(),
                                    ids: Some(vec!["size-medium".to_string()]),
                                    any: None,
                                }],
                            },
                            then: serde_json::Value::Number(serde_json::Number::from(220)),
                        },
                    ],
                    default: Some(serde_json::Value::Number(serde_json::Number::from(200))),
                }),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();
            rels.insert(
                "size".to_string(),
                RelationshipSelection::Ids {
                    ids: vec!["size-medium".to_string()],
                },
            );
            rels.insert(
                "fabric".to_string(),
                RelationshipSelection::Filter {
                    filter: crate::model::InstanceFilter {
                        types: Some(vec!["Fabric".to_string()]),
                        where_clause: Some(BoolExpr::All {
                            predicates: vec![Predicate::PropEq {
                                prop: "material".to_string(),
                                value: serde_json::Value::String("Cotton".to_string()),
                            }],
                        }),
                        sort: None,
                        limit: None,
                    },
                },
            );
            rels.insert(
                "leg".to_string(),
                RelationshipSelection::Ids {
                    ids: vec![
                        "leg-wooden".to_string(),
                        "leg-wooden-2".to_string(),
                        "leg-wooden-3".to_string(),
                        "leg-wooden-4".to_string(),
                    ],
                },
            );
            rels
        },
    };

    // Store all instances
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(small_size).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(medium_size).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(cotton_fabric).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(linen_fabric).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg_2).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg_3).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(wooden_leg_4).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(metal_leg).await?;
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(delux_underbed_enhanced).await?;

    Ok(())
    */
}

/// Load furniture workflow data that matches the Postman collection
/// This creates a complete dining table configuration scenario
async fn load_furniture_workflow_data<S: Store>(store: &S) -> Result<()> {
    // Check if furniture-db already exists to avoid overwriting user changes
    if let Ok(Some(_)) = store.get_database(&"furniture-db".to_string()).await {
        println!(
            "⚠️  Furniture workflow database already exists - skipping to preserve user changes"
        );
        return Ok(());
    }

    // Create the "furniture-db" database with main branch
    let mut furniture_db = Database::new_with_id(
        "furniture-db".to_string(),
        "Furniture Database".to_string(),
        Some("Kitchen bundles: tables, chairs, options".to_string()),
    );

    // Create main branch for this database
    let main_branch = Branch::new_main_branch(furniture_db.id.clone(), Some("System".to_string()));

    let branch_name = main_branch.name.clone();
    furniture_db.default_branch_name = branch_name.clone();

    // Save database and branch
    store.upsert_database(furniture_db).await?;
    store.upsert_branch(main_branch).await?;

    // Create the kitchen schema
    let kitchen_schema = create_kitchen_schema(branch_name.clone()).await?;
    // TODO: Schema updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!(
        "Seed data must be updated to use working commit system"
    ));
    // store.upsert_schema(kitchen_schema).await?;

    // Create all instances
    create_kitchen_instances(store, &branch_name).await?;

    println!("✅ Furniture workflow data loaded successfully!");
    println!("   Database: furniture-db");
    println!("   Schema: kitchen-schema with Table, Chair, Color, Option classes");
    println!("   Instances: Oak dining table with 4 chairs, red color, and service options");
    println!("   🎯 Perfect for debugging your ILP solver with real pool-based relationships!");

    Ok(())
}

/// Create the kitchen schema with all classes
async fn create_kitchen_schema(branch_name: String) -> Result<Schema> {
    let mut schema = Schema {
        id: "kitchen-schema".to_string(),
        // branch_id field removed in commit-based architecture
        classes: Vec::new(),
        description: Some("Kitchen furniture bundle schema".to_string()),
    };

    // Table class with complex relationships and derived properties
    let table_class = create_system_class_full(ClassDef {
        id: "class-table".to_string(),
        name: "Table".to_string(),
        description: Some("Dining table with computed total price".to_string()),
        properties: vec![
            PropertyDef {
                id: "base_price".to_string(),
                name: "base_price".to_string(),
                data_type: DataType::Number,
                required: Some(true),
            },
            PropertyDef {
                id: "discount".to_string(),
                name: "discount".to_string(),
                data_type: DataType::Number,
                required: Some(false),
            },
        ],
        relationships: vec![
            RelationshipDef {
                id: "chairs".to_string(),
                name: "chairs".to_string(),
                targets: vec!["class-chair".to_string()],
                quantifier: Quantifier::Exactly(4), // Must have exactly 4 chairs
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                default_pool: DefaultPool::All,
            },
            RelationshipDef {
                id: "color".to_string(),
                name: "color".to_string(),
                targets: vec!["class-color".to_string()],
                quantifier: Quantifier::AtMost(1), // At most 1 color
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                default_pool: DefaultPool::All,
            },
            RelationshipDef {
                id: "options".to_string(),
                name: "options".to_string(),
                targets: vec!["class-option".to_string()],
                quantifier: Quantifier::Any, // Any number of options
                universe: None,
                selection: SelectionType::ExplicitOrFilter,
                default_pool: DefaultPool::All,
            },
        ],
        derived: vec![
            // Complex derived property: total_price = base_price + sum(chairs.price) + sum(color.price) - discount
            DerivedDef {
                id: "total_price".to_string(),
                name: "total_price".to_string(),
                data_type: DataType::Number,
                expr: Expr::Add {
                    left: Box::new(Expr::Add {
                        left: Box::new(Expr::Add {
                            left: Box::new(Expr::Prop {
                                prop: "base_price".to_string(),
                            }),
                            right: Box::new(Expr::Sum {
                                over: "chairs".to_string(),
                                prop: "price".to_string(),
                                r#where: None,
                            }),
                        }),
                        right: Box::new(Expr::Sum {
                            over: "color".to_string(),
                            prop: "price".to_string(),
                            r#where: None,
                        }),
                    }),
                    right: Box::new(Expr::Sub {
                        left: Box::new(Expr::LitNumber { value: 0.0 }),
                        right: Box::new(Expr::Prop {
                            prop: "discount".to_string(),
                        }),
                    }),
                },
            },
        ],
        domain_constraint: Some(Domain::binary()), // Tables can be selected (1) or not (0)
        ..ClassDef::default()                      // This will fill in the audit fields
    });

    // Chair class (simple)
    let chair_class = create_system_class_full(ClassDef {
        id: "class-chair".to_string(),
        name: "Chair".to_string(),
        description: Some("Dining chair".to_string()),
        properties: vec![
            PropertyDef {
                id: "name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "price".to_string(),
                name: "price".to_string(),
                data_type: DataType::Number,
                required: Some(true),
            },
        ],
        relationships: vec![],
        derived: vec![],
        domain_constraint: Some(Domain::binary()),
        ..ClassDef::default()
    });

    // Color class (simple)
    let color_class = create_system_class_full(ClassDef {
        id: "Color".to_string(),
        name: "Color".to_string(),
        description: Some("Table color option".to_string()),
        properties: vec![
            PropertyDef {
                id: "name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "price".to_string(),
                name: "price".to_string(),
                data_type: DataType::Number,
                required: Some(true),
            },
        ],
        relationships: vec![],
        derived: vec![],
        domain_constraint: Some(Domain::binary()),
        ..ClassDef::default()
    });

    // Option class (simple)
    let option_class = create_system_class_full(ClassDef {
        id: "Option".to_string(),
        name: "Option".to_string(),
        description: Some("Additional service options".to_string()),
        properties: vec![
            PropertyDef {
                id: "name".to_string(),
                name: "name".to_string(),
                data_type: DataType::String,
                required: Some(true),
            },
            PropertyDef {
                id: "price".to_string(),
                name: "price".to_string(),
                data_type: DataType::Number,
                required: Some(true),
            },
        ],
        relationships: vec![],
        derived: vec![],
        domain_constraint: Some(Domain::binary()),
        ..ClassDef::default()
    });

    schema.classes = vec![table_class, chair_class, color_class, option_class];

    Ok(schema)
}

/// Create all kitchen instances - DISABLED for audit field migration
#[allow(dead_code)]
async fn create_kitchen_instances<S: Store>(_store: &S, _branch_name: &str) -> Result<()> {
    return Err(anyhow::anyhow!(
        "Function disabled during audit field migration"
    ));
    /*
    // Create 4 identical Oak chairs ($150 each)
    for i in 1..=4 {
        let oak_chair = create_system_instance_full(Instance {
            id: format!("oak-chair-{}", i),
            // branch_id removed in commit-based architecture
            class_id: "class-chair".to_string(),
            domain: Some(Domain::binary()),
            properties: {
                let mut props = HashMap::new();
                props.insert(
                    "name".to_string(),
                    PropertyValue::Literal(TypedValue::string("Oak Chair".to_string())),
                );
                props.insert(
                    "price".to_string(),
                    PropertyValue::Literal(TypedValue::number(150)),
                );
                props
            },
            relationships: HashMap::new(),
            ..Instance::default()
        });
        // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(oak_chair).await?;
    }

    // Create Red color option ($50)
    let red_color = create_system_instance_full(Instance {
        id: "red-color".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-color".to_string(),
        domain: Some(Domain::binary()),
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Red".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(50)),
            );
            props
        },
        relationships: HashMap::new(),
    };
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(red_color).await?;

    // Create Assembly Service option ($100)
    let assembly_service = Instance {
        id: "assembly-service".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-option".to_string(),
        domain: Some(Domain::binary()),
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Assembly Service".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(100)),
            );
            props
        },
        relationships: HashMap::new(),
    };
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(assembly_service).await?;

    // Create Extended Warranty option ($50)
    let extended_warranty = Instance {
        id: "extended-warranty".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-option".to_string(),
        domain: Some(Domain::binary()),
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "name".to_string(),
                PropertyValue::Literal(TypedValue::string("Extended Warranty".to_string())),
            );
            props.insert(
                "price".to_string(),
                PropertyValue::Literal(TypedValue::number(50)),
            );
            props
        },
        relationships: HashMap::new(),
    };
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(extended_warranty).await?;

    // Create Oak Dining Table with pool-based relationships
    let oak_dining_table = Instance {
        id: "oak-dining-table".to_string(),
        // branch_id removed in commit-based architecture
        class_id: "class-table".to_string(),
        domain: Some(Domain::binary()),
        properties: {
            let mut props = HashMap::new();
            props.insert(
                "base_price".to_string(),
                PropertyValue::Literal(TypedValue::number(800)),
            );
            props.insert(
                "discount".to_string(),
                PropertyValue::Literal(TypedValue::number(100)),
            );
            props
        },
        relationships: {
            let mut rels = HashMap::new();

            // Chairs: Pool-based selection from all Chair instances
            rels.insert(
                "chairs".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Chair".to_string()]),
                        where_clause: None,
                        sort: None,
                        limit: None,
                    }),
                    selection: None, // Unresolved - let the ILP solver decide which 4 chairs
                },
            );

            // Color: Pool-based selection from all Color instances
            rels.insert(
                "color".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Color".to_string()]),
                        where_clause: None,
                        sort: None,
                        limit: None,
                    }),
                    selection: None, // Unresolved - let ILP solver decide
                },
            );

            // Options: Pool-based selection from all Option instances
            rels.insert(
                "options".to_string(),
                RelationshipSelection::PoolBased {
                    pool: Some(InstanceFilter {
                        types: Some(vec!["Option".to_string()]),
                        where_clause: None,
                        sort: None,
                        limit: None,
                    }),
                    selection: None, // Unresolved - let ILP solver decide
                },
            );

            rels
        },
    };
    // TODO: Instance updates must be done through working commits in new architecture
    // For now, this seed data function is disabled
    return Err(anyhow::anyhow!("Seed data must be updated to use working commit system"));
    // store.upsert_instance(oak_dining_table).await?;

    Ok(())
    */
}
