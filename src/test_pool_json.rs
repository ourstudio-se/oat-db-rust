#[cfg(test)]
mod tests {
    use crate::model::{RelationshipSelection, InstanceFilter, SelectionSpec, BoolExpr, Predicate};
    use serde_json;

    #[test]
    fn test_pool_based_json_serialization() {
        // Example 1: PoolBased with custom pool and selection
        let pool_based = RelationshipSelection::PoolBased {
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
            selection: Some(SelectionSpec::Ids(vec!["color-red".to_string()])),
        };
        
        println!("\nPoolBased with custom pool:");
        let json = serde_json::to_string_pretty(&pool_based).unwrap();
        println!("{}", json);
        
        // Example 2: PoolBased with no pool override and explicit selection
        let pool_based_no_pool = RelationshipSelection::PoolBased {
            pool: None,
            selection: Some(SelectionSpec::Ids(vec!["option-gps".to_string()])),
        };
        
        println!("\nPoolBased with no pool override:");
        let json = serde_json::to_string_pretty(&pool_based_no_pool).unwrap();
        println!("{}", json);
        
        // Example 3: PoolBased with pool but no selection (unresolved)
        let pool_based_unresolved = RelationshipSelection::PoolBased {
            pool: Some(InstanceFilter {
                types: Some(vec!["Option".to_string()]),
                where_clause: None,
                sort: None,
                limit: None,
            }),
            selection: None,
        };
        
        println!("\nPoolBased unresolved:");
        let json = serde_json::to_string_pretty(&pool_based_unresolved).unwrap();
        println!("{}", json);
        
        // Test deserialization back
        let json_str = r#"{
            "pool": {
                "type": ["Color"],
                "where": {
                    "all": [
                        {"prop_lt": {"prop": "price", "value": 100}}
                    ]
                }
            },
            "selection": ["color-red"]
        }"#;
        
        println!("\nDeserializing JSON:");
        println!("{}", json_str);
        match serde_json::from_str::<RelationshipSelection>(json_str) {
            Ok(rel) => println!("Deserialized successfully: {:?}", rel),
            Err(e) => println!("Deserialization failed: {}", e)
        }
    }
}