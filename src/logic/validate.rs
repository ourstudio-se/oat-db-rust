use anyhow::{Result, anyhow};
use std::collections::{HashMap, HashSet};

use crate::model::{Schema, Instance, Quantifier, RelationshipSelection, Id};
use crate::store::traits::Store;

pub struct Validator;

impl Validator {
    pub async fn validate_instance<S: Store>(
        store: &S,
        instance: &Instance,
        schema: &Schema,
    ) -> Result<()> {
        Self::validate_properties(instance, schema)?;
        Self::validate_relationships(store, instance, schema).await?;
        Ok(())
    }

    fn validate_properties(instance: &Instance, schema: &Schema) -> Result<()> {
        let schema_props: HashMap<String, &crate::model::PropertyDef> = 
            schema.properties.iter().map(|p| (p.id.clone(), p)).collect();

        for (prop_id, _) in &instance.properties {
            if !schema_props.contains_key(prop_id) {
                return Err(anyhow!("Property '{}' not defined in schema", prop_id));
            }
        }

        for prop_def in &schema.properties {
            if prop_def.required.unwrap_or(false) && !instance.properties.contains_key(&prop_def.id) {
                return Err(anyhow!("Required property '{}' is missing", prop_def.id));
            }
        }

        Ok(())
    }

    async fn validate_relationships<S: Store>(
        store: &S,
        instance: &Instance,
        schema: &Schema,
    ) -> Result<()> {
        let schema_rels: HashMap<String, &crate::model::RelationshipDef> = 
            schema.relationships.iter().map(|r| (r.id.clone(), r)).collect();

        for (rel_id, selection) in &instance.relationships {
            let rel_def = schema_rels.get(rel_id)
                .ok_or_else(|| anyhow!("Relationship '{}' not defined in schema", rel_id))?;

            let target_ids = Self::resolve_selection(store, selection).await?;
            Self::validate_relationship_targets(store, rel_def, &target_ids).await?;
            Self::validate_quantifier(store, rel_def, &target_ids).await?;
        }

        Ok(())
    }

    async fn resolve_selection<S: Store>(
        store: &S,
        selection: &RelationshipSelection,
    ) -> Result<Vec<Id>> {
        match selection {
            RelationshipSelection::Ids { ids } => Ok(ids.clone()),
            RelationshipSelection::Filter { filter } => {
                let instances = store.list_instances(Some(filter.clone())).await?;
                Ok(instances.into_iter().map(|i| i.id).collect())
            }
            RelationshipSelection::All => {
                let instances = store.list_instances(None).await?;
                Ok(instances.into_iter().map(|i| i.id).collect())
            }
        }
    }

    async fn validate_relationship_targets<S: Store>(
        store: &S,
        rel_def: &crate::model::RelationshipDef,
        target_ids: &[Id],
    ) -> Result<()> {
        // Allow duplicates for relationships - a bed can have 4 legs of the same type

        for id in target_ids {
            if let Some(instance) = store.get_instance(id).await? {
                if !rel_def.targets.contains(&instance.class_id) {
                    return Err(anyhow!(
                        "Instance '{}' of type '{}' is not a valid target for this relationship",
                        id, instance.class_id
                    ));
                }
            } else {
                return Err(anyhow!("Referenced instance '{}' does not exist", id));
            }
        }

        Ok(())
    }

    async fn validate_quantifier<S: Store>(
        store: &S,
        rel_def: &crate::model::RelationshipDef,
        target_ids: &[Id],
    ) -> Result<()> {
        let count = target_ids.len();

        match &rel_def.quantifier {
            Quantifier::Exactly(n) => {
                if count != *n {
                    return Err(anyhow!(
                        "Relationship requires exactly {} targets, found {}", n, count
                    ));
                }
            }
            Quantifier::AtLeast(n) => {
                if count < *n {
                    return Err(anyhow!(
                        "Relationship requires at least {} targets, found {}", n, count
                    ));
                }
            }
            Quantifier::AtMost(n) => {
                if count > *n {
                    return Err(anyhow!(
                        "Relationship allows at most {} targets, found {}", n, count
                    ));
                }
            }
            Quantifier::Range(min, max) => {
                if count < *min || count > *max {
                    return Err(anyhow!(
                        "Relationship requires {}-{} targets, found {}", min, max, count
                    ));
                }
            }
            Quantifier::Optional => {
                if count > 1 {
                    return Err(anyhow!(
                        "Optional relationship allows 0 or 1 targets, found {}", count
                    ));
                }
            }
            Quantifier::Any => {
            }
            Quantifier::All => {
                if let Some(universe_type) = &rel_def.universe {
                    let universe_instances = store.find_by_type(universe_type).await?;
                    let universe_ids: HashSet<_> = universe_instances.iter().map(|i| &i.id).collect();
                    let target_ids_set: HashSet<_> = target_ids.iter().collect();
                    
                    if universe_ids != target_ids_set {
                        return Err(anyhow!(
                            "ALL quantifier requires selection to equal universe set"
                        ));
                    }
                }
            }
        }

        Ok(())
    }
}