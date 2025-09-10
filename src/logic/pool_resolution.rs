use anyhow::{anyhow, Result};
use crate::model::{
    DefaultPool, Instance, InstanceFilter, RelationshipDef, RelationshipSelection, 
    SelectionSpec, Id
};
use std::collections::HashSet;

/// Pool and selection resolver for combinatorial optimization
pub struct PoolResolver;

impl PoolResolver {
    /// Resolve the effective pool for a relationship
    /// Step 1: Determine what instances are available for selection
    pub fn resolve_effective_pool(
        instances: &[Instance],
        relationship_def: &RelationshipDef,
        instance_override: Option<&InstanceFilter>,
    ) -> Result<Vec<Id>> {
        let pool_filter = if let Some(override_filter) = instance_override {
            // Use instance-level pool override
            override_filter.clone()
        } else {
            // Use schema's default_pool
            match &relationship_def.default_pool {
                DefaultPool::None => {
                    // Empty pool
                    return Ok(Vec::new());
                }
                DefaultPool::All => {
                    // All instances of the target types
                    InstanceFilter {
                        types: Some(relationship_def.targets.clone()),
                        where_clause: None,
                        sort: None,
                        limit: None,
                    }
                }
                DefaultPool::Filter { types, filter } => {
                    // Filtered subset
                    InstanceFilter {
                        types: types.clone().or_else(|| Some(relationship_def.targets.clone())),
                        where_clause: filter.as_ref().and_then(|f| f.where_clause.clone()),
                        sort: filter.as_ref().and_then(|f| f.sort.clone()),
                        limit: filter.as_ref().and_then(|f| f.limit),
                    }
                }
            }
        };

        // Filter instances to get pool instances
        let mut pool_instances: Vec<Instance> = instances.to_vec();
        
        // Apply type filter if specified
        if let Some(types) = &pool_filter.types {
            pool_instances.retain(|inst| types.contains(&inst.class_id));
        }
        
        // Apply where clause filter if specified
        if let Some(filter_expr) = &pool_filter.where_clause {
            pool_instances = crate::logic::filter_instances(pool_instances, filter_expr);
        }
        
        // Apply sort if specified
        if let Some(sort_field) = &pool_filter.sort {
            pool_instances.sort_by(|a, b| {
                use std::cmp::Ordering;
                let a_val = a.properties.get(sort_field).map(|pv| match pv {
                    crate::model::PropertyValue::Literal(tv) => &tv.value,
                    crate::model::PropertyValue::Conditional(_) => &serde_json::Value::Null,
                });
                let b_val = b.properties.get(sort_field).map(|pv| match pv {
                    crate::model::PropertyValue::Literal(tv) => &tv.value,
                    crate::model::PropertyValue::Conditional(_) => &serde_json::Value::Null,
                });
                match (a_val, b_val) {
                    (Some(a), Some(b)) => {
                        let a_str = serde_json::to_string(a).unwrap_or_default();
                        let b_str = serde_json::to_string(b).unwrap_or_default();
                        a_str.cmp(&b_str)
                    },
                    (Some(_), None) => Ordering::Greater,
                    (None, Some(_)) => Ordering::Less,
                    (None, None) => Ordering::Equal,
                }
            });
        }
        
        // Apply limit if specified
        if let Some(limit) = pool_filter.limit {
            pool_instances.truncate(limit);
        }
        
        Ok(pool_instances.into_iter().map(|inst| inst.id).collect())
    }

    /// Resolve the final selection from the effective pool
    /// Step 2: Determine which specific instances are selected
    pub fn resolve_selection(
        instances: &[Instance],
        relationship_def: &RelationshipDef,
        effective_pool: &[Id],
        selection_spec: Option<&SelectionSpec>,
    ) -> Result<SelectionResult> {
        let pool_set: HashSet<&Id> = effective_pool.iter().collect();

        match selection_spec {
            Some(SelectionSpec::Ids(ids)) => {
                // Explicit selection - must be subset of pool
                for id in ids {
                    if !pool_set.contains(id) {
                        return Err(anyhow!(
                            "Selection instance '{}' is not in the effective pool for relationship '{}'",
                            id, relationship_def.name
                        ));
                    }
                }
                Ok(SelectionResult::Resolved(ids.clone()))
            }
            Some(SelectionSpec::Filter(filter)) => {
                // Filter-based selection from pool
                // Filter to only pool instances from the provided instances
                let pool_instances: Vec<Instance> = instances.iter()
                    .filter(|inst| effective_pool.contains(&inst.id))
                    .cloned()
                    .collect();
                
                // Apply the selection filter to the pool instances
                let mut filtered_instances = pool_instances;
                
                // Apply type filter if specified
                if let Some(types) = &filter.types {
                    filtered_instances.retain(|inst| types.contains(&inst.class_id));
                }
                
                // Apply where clause filter if specified
                if let Some(filter_expr) = &filter.where_clause {
                    filtered_instances = crate::logic::filter_instances(filtered_instances, filter_expr);
                }
                
                // Apply sort if specified
                if let Some(sort_field) = &filter.sort {
                    filtered_instances.sort_by(|a, b| {
                        use std::cmp::Ordering;
                        let a_val = a.properties.get(sort_field).map(|pv| match pv {
                            crate::model::PropertyValue::Literal(tv) => &tv.value,
                            crate::model::PropertyValue::Conditional(_) => &serde_json::Value::Null,
                        });
                        let b_val = b.properties.get(sort_field).map(|pv| match pv {
                            crate::model::PropertyValue::Literal(tv) => &tv.value,
                            crate::model::PropertyValue::Conditional(_) => &serde_json::Value::Null,
                        });
                        match (a_val, b_val) {
                            (Some(a), Some(b)) => {
                                let a_str = serde_json::to_string(a).unwrap_or_default();
                                let b_str = serde_json::to_string(b).unwrap_or_default();
                                a_str.cmp(&b_str)
                            },
                            (Some(_), None) => Ordering::Greater,
                            (None, Some(_)) => Ordering::Less,
                            (None, None) => Ordering::Equal,
                        }
                    });
                }
                
                // Apply limit if specified
                if let Some(limit) = filter.limit {
                    filtered_instances.truncate(limit);
                }
                
                let filtered_ids: Vec<Id> = filtered_instances.into_iter().map(|inst| inst.id).collect();
                Ok(SelectionResult::Resolved(filtered_ids))
            }
            Some(SelectionSpec::All) => {
                // Select all from pool
                Ok(SelectionResult::Resolved(effective_pool.to_vec()))
            }
            Some(SelectionSpec::Unresolved) | None => {
                // Check quantifier to determine behavior
                match &relationship_def.quantifier {
                    crate::model::Quantifier::All => {
                        // If quantifier is ALL, selection equals pool
                        Ok(SelectionResult::Resolved(effective_pool.to_vec()))
                    }
                    _ => {
                        // Selection is unresolved - needs solver/user input
                        Ok(SelectionResult::Unresolved(effective_pool.to_vec()))
                    }
                }
            }
        }
    }

    /// Full resolution: pool + selection
    pub fn resolve_relationship(
        instances: &[Instance],
        relationship_def: &RelationshipDef,
        relationship_selection: &RelationshipSelection,
    ) -> Result<SelectionResult> {
        match relationship_selection {
            RelationshipSelection::PoolBased { pool, selection } => {
                // New pool-based resolution
                let effective_pool = Self::resolve_effective_pool(
                    instances,
                    relationship_def,
                    pool.as_ref(),
                )?;

                Self::resolve_selection(
                    instances,
                    relationship_def,
                    &effective_pool,
                    selection.as_ref(),
                )
            }
            // Legacy formats - convert to resolved selections
            RelationshipSelection::SimpleIds(ids) => {
                Ok(SelectionResult::Resolved(ids.clone()))
            }
            RelationshipSelection::Ids { ids } => {
                Ok(SelectionResult::Resolved(ids.clone()))
            }
            RelationshipSelection::All => {
                // Resolve as "all from default pool"
                let effective_pool = Self::resolve_effective_pool(
                    instances,
                    relationship_def,
                    None,
                )?;
                Ok(SelectionResult::Resolved(effective_pool))
            }
            RelationshipSelection::Filter { filter } => {
                // Apply filter directly to the provided instances
                let mut filtered_instances: Vec<Instance> = instances.to_vec();
                
                // Apply type filter if specified
                if let Some(types) = &filter.types {
                    filtered_instances.retain(|inst| types.contains(&inst.class_id));
                }
                
                // Apply where clause filter if specified
                if let Some(filter_expr) = &filter.where_clause {
                    filtered_instances = crate::logic::filter_instances(filtered_instances, filter_expr);
                }
                
                // Apply sort if specified
                if let Some(sort_field) = &filter.sort {
                    filtered_instances.sort_by(|a, b| {
                        use std::cmp::Ordering;
                        let a_val = a.properties.get(sort_field).map(|pv| match pv {
                            crate::model::PropertyValue::Literal(tv) => &tv.value,
                            crate::model::PropertyValue::Conditional(_) => &serde_json::Value::Null,
                        });
                        let b_val = b.properties.get(sort_field).map(|pv| match pv {
                            crate::model::PropertyValue::Literal(tv) => &tv.value,
                            crate::model::PropertyValue::Conditional(_) => &serde_json::Value::Null,
                        });
                        match (a_val, b_val) {
                            (Some(a), Some(b)) => {
                                let a_str = serde_json::to_string(a).unwrap_or_default();
                                let b_str = serde_json::to_string(b).unwrap_or_default();
                                a_str.cmp(&b_str)
                            },
                            (Some(_), None) => Ordering::Greater,
                            (None, Some(_)) => Ordering::Less,
                            (None, None) => Ordering::Equal,
                        }
                    });
                }
                
                // Apply limit if specified
                if let Some(limit) = filter.limit {
                    filtered_instances.truncate(limit);
                }
                
                let ids = filtered_instances.into_iter().map(|inst| inst.id).collect();
                Ok(SelectionResult::Resolved(ids))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SelectionResult {
    /// Selection is fully resolved to specific instance IDs
    Resolved(Vec<Id>),
    /// Selection is unresolved - contains the available pool for solver/user choice
    Unresolved(Vec<Id>),
}