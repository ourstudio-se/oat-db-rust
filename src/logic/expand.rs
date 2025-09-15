use crate::model::{
    ExpandedInstance, Id, Instance, PropertyValue, RelationshipSelection, ResolutionDetails,
    ResolutionMethod, ResolvedRelationship, Schema,
};
use crate::store::traits::Store;
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::time::Instant;

pub struct Expander;

impl Expander {
    pub async fn expand_instance(
        instance: &Instance,
        other_instances: &[Instance],
        schema: &Schema,
    ) -> Result<ExpandedInstance> {
        // Simple expansion - just resolve relationships using schema default pools
        Self::expand_simple(instance, schema, other_instances).await
    }

    async fn expand_simple(
        instance: &Instance,
        schema: &Schema,
        other_instances: &[Instance],
    ) -> Result<ExpandedInstance> {
        let mut expanded_props = HashMap::new();

        // Expand properties (literal and conditional)
        for (key, prop_value) in &instance.properties {
            match prop_value {
                PropertyValue::Literal(typed_value) => {
                    expanded_props.insert(key.clone(), typed_value.value.clone());
                }
                PropertyValue::Conditional(rule_set) => {
                    let value =
                        crate::logic::SimpleEvaluator::evaluate_rule_set(rule_set, instance);
                    expanded_props.insert(key.clone(), value);
                }
            }
        }

        // Get schema to resolve relationships with default pools
        let class_def = schema
            .classes
            .iter()
            .find(|c| c.id == instance.class_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Class definition not found for instance: {}",
                    instance.class_id
                )
            })?;

        let mut expanded_rels = HashMap::new();

        // Process each relationship definition from the schema
        for rel_def in &class_def.relationships {
            let relationship_name = &rel_def.id;

            // Check if instance has explicit relationship data
            let instance_relationship = instance.relationships.get(relationship_name);

            let resolved_rel = if let Some(existing_selection) = instance_relationship {
                // Use existing instance relationship selection
                Self::resolve_selection_enhanced_with_branch(other_instances, existing_selection)
                    .await?
            } else {
                // No explicit relationship data - resolve using schema default pool
                let resolved_relationship =
                    Self::resolve_relationship_from_schema(other_instances, rel_def).await?;
                resolved_relationship
            };

            expanded_rels.insert(relationship_name.clone(), resolved_rel);
        }

        Ok(ExpandedInstance {
            id: instance.id.clone(),
            class_id: instance.class_id.clone(),
            domain: instance.domain.clone(),
            properties: expanded_props,
            relationships: expanded_rels,
            included: Vec::new(),
            created_by: instance.created_by.clone(),
            created_at: instance.created_at,
            updated_by: instance.updated_by.clone(),
            updated_at: instance.updated_at,
        })
    }

    /// Resolve all relationships for an instance using schema definitions and default pools
    pub async fn resolve_all_relationships_from_schema(
        instance: &Instance,
        schema: &Schema,
        other_instances: &[Instance],
    ) -> Result<HashMap<String, ResolvedRelationship>> {
        // Get schema to resolve relationships with default pools
        let class_def = schema
            .classes
            .iter()
            .find(|c| c.id == instance.class_id)
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "Class definition not found for instance: {}",
                    instance.class_id
                )
            })?;

        let mut resolved_rels = HashMap::new();

        // Process each relationship definition from the schema
        for rel_def in &class_def.relationships {
            let relationship_id = &rel_def.id;
            let resolved_rel =
                Self::resolve_relationship_from_schema(other_instances, rel_def).await?;
            resolved_rels.insert(relationship_id.clone(), resolved_rel);
        }

        Ok(resolved_rels)
    }

    /// Resolve a relationship using schema definition and default pool
    pub async fn resolve_relationship_from_schema(
        other_instances: &[Instance],
        rel_def: &crate::model::RelationshipDef,
    ) -> Result<ResolvedRelationship> {
        use crate::logic::pool_resolution::{PoolResolver, SelectionResult};
        use crate::model::Quantifier;

        let start_time = Instant::now();

        // Step 1: Get all instances and resolve effective pool
        let instances = other_instances.to_vec();
        let effective_pool = PoolResolver::resolve_effective_pool(
            &instances, rel_def, None, // No instance override
        )?;

        // Step 2: For default pool resolution, show the full pool as unresolved
        // This allows the frontend/user to see all available options and make selections
        // The quantifier constrains what CAN be selected, but doesn't pre-select
        let pool_size = effective_pool.len();
        let selection_result = match &rel_def.quantifier {
            Quantifier::All => {
                // ALL quantifier means select everything from pool automatically
                SelectionResult::Resolved(effective_pool)
            }
            Quantifier::Any => {
                // ANY means 0 or more - show pool but don't pre-select anything
                SelectionResult::Unresolved(effective_pool)
            }
            _ => {
                // For all other quantifiers (EXACTLY, AT_MOST, AT_LEAST, RANGE, OPTIONAL)
                // The quantifier does NOT affect the default pool in any way
                // Pool resolution returns ALL available instances - quantifiers are used later by the solver
                // to determine how many instances to actually select from the materialized_ids
                SelectionResult::Unresolved(effective_pool)
            }
        };

        let (materialized_ids, method, notes) = match selection_result {
            SelectionResult::Resolved(ids) => (
                ids.clone(),
                ResolutionMethod::PoolFilterResolved,
                vec![format!("Resolved {} instances using default pool and quantifier {:?}", ids.len(), rel_def.quantifier)]
            ),
            SelectionResult::Unresolved(pool_ids) => (
                pool_ids.clone(),
                ResolutionMethod::SchemaDefaultResolved,
                vec![format!("Pool resolved from schema default - {} instances available for solver selection", pool_ids.len())]
            ),
        };

        let elapsed = start_time.elapsed();

        Ok(ResolvedRelationship {
            materialized_ids,
            resolution_method: method,
            resolution_details: Some(ResolutionDetails {
                original_definition: Some(serde_json::to_value(rel_def).unwrap_or_default()),
                resolved_from: Some("schema_default_pool".to_string()),
                filter_description: Some(format!(
                    "Default pool mode: {:?}, quantifier: {:?}",
                    rel_def.default_pool, rel_def.quantifier
                )),
                total_pool_size: Some(pool_size),
                filtered_out_count: Some(0),
                resolution_time_us: Some(elapsed.as_micros() as u64),
                notes,
            }),
        })
    }

    pub async fn resolve_selection_enhanced<S: Store>(
        store: &S,
        selection: &RelationshipSelection,
    ) -> Result<ResolvedRelationship> {
        // WARNING: This method has no branch context - it will search ALL databases!
        // Use resolve_selection_enhanced_with_branch for proper isolation
        return Err(anyhow::anyhow!("resolve_selection_enhanced called without database_id - use resolve_selection_enhanced_with_branch instead"));
    }

    pub async fn resolve_selection_enhanced_with_branch(
        other_instances: &[Instance],
        selection: &RelationshipSelection,
    ) -> Result<ResolvedRelationship> {
        let start_time = Instant::now();

        let (ids, method, details) = match selection {
            RelationshipSelection::SimpleIds(ids) => (
                ids.clone(),
                ResolutionMethod::ExplicitIds,
                Some(ResolutionDetails {
                    original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                    resolved_from: Some("simple_ids".to_string()),
                    filter_description: None,
                    total_pool_size: Some(ids.len()),
                    filtered_out_count: Some(0),
                    resolution_time_us: None,
                    notes: vec!["Explicitly set instance IDs".to_string()],
                }),
            ),
            RelationshipSelection::Ids { ids } => (
                ids.clone(),
                ResolutionMethod::ExplicitIds,
                Some(ResolutionDetails {
                    original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                    resolved_from: Some("explicit_ids".to_string()),
                    filter_description: None,
                    total_pool_size: Some(ids.len()),
                    filtered_out_count: Some(0),
                    resolution_time_us: None,
                    notes: vec!["Explicitly set instance IDs".to_string()],
                }),
            ),
            RelationshipSelection::PoolBased { pool, selection } => {
                // Resolve the pool first
                let pool_instances = if let Some(pool_filter) = pool {
                    Self::resolve_pool_filter(other_instances, pool_filter).await?
                } else {
                    Vec::new() // No pool filter means we'd need all instances (branch context needed)
                };

                let pool_size = pool_instances.len();

                // Apply selection to the pool
                let (final_ids, method, resolved_from, filter_desc, notes) = match selection {
                    Some(crate::model::SelectionSpec::Ids(ids)) => {
                        let final_ids = if pool_instances.is_empty() {
                            ids.clone()
                        } else {
                            ids.iter()
                                .filter(|id| pool_instances.contains(id))
                                .cloned()
                                .collect()
                        };
                        let filtered_count = ids.len() - final_ids.len();
                        (
                            final_ids,
                            ResolutionMethod::PoolSelectionResolved,
                            "pool_with_explicit_selection".to_string(),
                            Some(format!(
                                "Explicit selection from pool (filtered {} out)",
                                filtered_count
                            )),
                            vec![format!(
                                "Selected {} IDs from pool of {} instances",
                                ids.len(),
                                pool_size
                            )],
                        )
                    }
                    Some(crate::model::SelectionSpec::Filter(filter)) => {
                        let filtered_ids =
                            Self::resolve_pool_filter(other_instances, filter).await?;
                        (
                            filtered_ids.clone(),
                            ResolutionMethod::PoolFilterResolved,
                            "pool_with_filter_selection".to_string(),
                            Some("Applied selection filter to pool".to_string()),
                            vec![format!("Filtered pool to {} instances", filtered_ids.len())],
                        )
                    }
                    Some(crate::model::SelectionSpec::All) => (
                        pool_instances.clone(),
                        ResolutionMethod::PoolFilterResolved,
                        "pool_select_all".to_string(),
                        Some("Selected all instances from pool".to_string()),
                        vec![format!("Selected all {} instances from pool", pool_size)],
                    ),
                    Some(crate::model::SelectionSpec::Unresolved) | None => (
                        pool_instances.clone(),
                        ResolutionMethod::PoolFilterResolved,
                        "pool_unresolved".to_string(),
                        if let Some(pool_filter) = pool {
                            Some(format!("Pool filter: {:?}", pool_filter))
                        } else {
                            Some("No pool filter - would need all instances".to_string())
                        },
                        vec!["Selection is unresolved - showing available pool".to_string()],
                    ),
                };

                (
                    final_ids,
                    method,
                    Some(ResolutionDetails {
                        original_definition: Some(
                            serde_json::to_value(selection).unwrap_or_default(),
                        ),
                        resolved_from: Some(resolved_from),
                        filter_description: filter_desc,
                        total_pool_size: Some(pool_size),
                        filtered_out_count: Some(pool_size.saturating_sub(pool_instances.len())),
                        resolution_time_us: None,
                        notes,
                    }),
                )
            }
            RelationshipSelection::Filter { filter } => {
                let filtered_ids = Self::resolve_pool_filter(other_instances, filter).await?;
                (
                    filtered_ids.clone(),
                    ResolutionMethod::DynamicSelectorResolved,
                    Some(ResolutionDetails {
                        original_definition: Some(
                            serde_json::to_value(selection).unwrap_or_default(),
                        ),
                        resolved_from: Some("direct_filter".to_string()),
                        filter_description: Some(format!("Applied filter: {:?}", filter)),
                        total_pool_size: None, // Unknown without branch context
                        filtered_out_count: None,
                        resolution_time_us: None,
                        notes: vec![format!(
                            "Resolved {} instances via direct filter",
                            filtered_ids.len()
                        )],
                    }),
                )
            }
            RelationshipSelection::All => (
                Vec::new(),
                ResolutionMethod::EmptyResolution,
                Some(ResolutionDetails {
                    original_definition: Some(serde_json::to_value(selection).unwrap_or_default()),
                    resolved_from: Some("all_instances".to_string()),
                    filter_description: Some("Select all instances of target types".to_string()),
                    total_pool_size: None,
                    filtered_out_count: None,
                    resolution_time_us: None,
                    notes: vec!["Cannot resolve 'All' without branch context".to_string()],
                }),
            ),
        };

        let elapsed = start_time.elapsed();

        // Add timing to details if provided
        let mut final_details = details;
        if let Some(ref mut detail) = final_details {
            detail.resolution_time_us = Some(elapsed.as_micros() as u64);
        }

        Ok(ResolvedRelationship {
            materialized_ids: ids,
            resolution_method: method,
            resolution_details: final_details,
        })
    }

    // Keep the old function for any remaining uses
    async fn resolve_selection<S: Store>(
        store: &S,
        selection: &RelationshipSelection,
    ) -> Result<Vec<Id>> {
        let resolved = Self::resolve_selection_enhanced(store, selection).await?;
        Ok(resolved.materialized_ids)
    }

    async fn resolve_pool_filter(
        other_instances: &[Instance],
        filter: &crate::model::InstanceFilter,
    ) -> Result<Vec<Id>> {
        // Get instances from ONLY the specified branch - NEVER cross database boundaries!

        if let Some(types) = &filter.types {
            let mut matching_instances = Vec::new();

            // FIXED: Only query the specific branch, never cross databases
            for instance_type in types {
                let instances = other_instances
                    .iter()
                    .filter(|i| i.class_id == *instance_type)
                    .cloned()
                    .collect::<Vec<_>>();
                matching_instances.extend(instances);
            }

            // Apply where_clause filters if present using our unified filtering system
            if let Some(where_clause) = &filter.where_clause {
                matching_instances =
                    crate::logic::filter_instances(matching_instances, where_clause);
            }

            // Apply sorting if present
            if let Some(sort_field) = &filter.sort {
                if let Some(order) = sort_field.strip_suffix(" DESC") {
                    let field_name = order.trim();
                    matching_instances.sort_by(|a, b| {
                        Self::compare_instances_by_field(b, a, field_name)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                } else if let Some(field_name) = sort_field.strip_suffix(" ASC") {
                    let field_name = field_name.trim();
                    matching_instances.sort_by(|a, b| {
                        Self::compare_instances_by_field(a, b, field_name)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                } else {
                    // Default to ASC if no order specified
                    let field_name = sort_field.trim();
                    matching_instances.sort_by(|a, b| {
                        Self::compare_instances_by_field(a, b, field_name)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });
                }
            }

            // Apply limit if present
            if let Some(limit) = filter.limit {
                matching_instances.truncate(limit);
            }

            Ok(matching_instances.into_iter().map(|i| i.id).collect())
        } else {
            // No type filter means we can't resolve without more context
            Ok(Vec::new())
        }
    }

    /// Check if an instance matches a where clause (basic implementation)
    fn matches_where_clause(
        _instance: &crate::model::Instance,
        _where_clause: &serde_json::Value,
    ) -> bool {
        // For now, return true (no filtering)
        // TODO: Implement proper where clause evaluation based on the JSON value
        // This would parse property conditions like {"prop-color-price": {"lt": "70"}}
        true
    }

    /// Compare two instances by a field name for sorting
    fn compare_instances_by_field(
        a: &crate::model::Instance,
        b: &crate::model::Instance,
        field_name: &str,
    ) -> Result<std::cmp::Ordering> {
        use crate::model::PropertyValue;

        let a_value = a.properties.get(field_name);
        let b_value = b.properties.get(field_name);

        match (a_value, b_value) {
            (Some(PropertyValue::Literal(a_typed)), Some(PropertyValue::Literal(b_typed))) => {
                match (&a_typed.value, &b_typed.value) {
                    (serde_json::Value::Number(a_num), serde_json::Value::Number(b_num)) => {
                        if let (Some(a_f64), Some(b_f64)) = (a_num.as_f64(), b_num.as_f64()) {
                            Ok(a_f64
                                .partial_cmp(&b_f64)
                                .unwrap_or(std::cmp::Ordering::Equal))
                        } else {
                            Ok(std::cmp::Ordering::Equal)
                        }
                    }
                    (serde_json::Value::String(a_str), serde_json::Value::String(b_str)) => {
                        Ok(a_str.cmp(b_str))
                    }
                    _ => Ok(std::cmp::Ordering::Equal),
                }
            }
            (Some(PropertyValue::Conditional(_)), Some(PropertyValue::Literal(_))) => {
                // Conditional properties are harder to compare, treat as less than literal
                Ok(std::cmp::Ordering::Less)
            }
            (Some(PropertyValue::Literal(_)), Some(PropertyValue::Conditional(_))) => {
                // Literal properties are easier to compare, treat as greater than conditional
                Ok(std::cmp::Ordering::Greater)
            }
            (Some(PropertyValue::Conditional(_)), Some(PropertyValue::Conditional(_))) => {
                // Both conditional, can't meaningfully compare without evaluation
                Ok(std::cmp::Ordering::Equal)
            }
            (Some(_), None) => Ok(std::cmp::Ordering::Greater),
            (None, Some(_)) => Ok(std::cmp::Ordering::Less),
            (None, None) => Ok(std::cmp::Ordering::Equal),
        }
    }

    pub fn deduplicate_included(expanded: &mut ExpandedInstance) {
        let mut seen = HashSet::new();
        expanded
            .included
            .retain(|instance| seen.insert(instance.id.clone()));
    }
}
