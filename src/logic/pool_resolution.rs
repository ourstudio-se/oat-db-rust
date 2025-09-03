use anyhow::{anyhow, Result};
use crate::model::{
    DefaultPool, InstanceFilter, RelationshipDef, RelationshipSelection, 
    SelectionSpec, Id
};
use crate::store::traits::Store;
use std::collections::HashSet;

/// Pool and selection resolver for combinatorial optimization
pub struct PoolResolver;

impl PoolResolver {
    /// Resolve the effective pool for a relationship
    /// Step 1: Determine what instances are available for selection
    pub async fn resolve_effective_pool<S: Store>(
        store: &S,
        relationship_def: &RelationshipDef,
        instance_override: Option<&InstanceFilter>,
        database_id: &Id,
        branch_id: &Id,
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

        // Execute the filter to get pool instances
        let pool_instances = store.list_instances_for_branch(database_id, branch_id, Some(pool_filter)).await?;
        Ok(pool_instances.into_iter().map(|inst| inst.id).collect())
    }

    /// Resolve the final selection from the effective pool
    /// Step 2: Determine which specific instances are selected
    pub async fn resolve_selection<S: Store>(
        _store: &S,
        relationship_def: &RelationshipDef,
        effective_pool: &[Id],
        selection_spec: Option<&SelectionSpec>,
        _branch_id: &Id,
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
            Some(SelectionSpec::Filter(_filter)) => {
                // Filter-based selection from pool
                // For now, this is simplified - in a full implementation you'd apply the filter
                // to the pool instances
                Ok(SelectionResult::Resolved(effective_pool.to_vec()))
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
    pub async fn resolve_relationship<S: Store>(
        store: &S,
        relationship_def: &RelationshipDef,
        relationship_selection: &RelationshipSelection,
        database_id: &Id,
        branch_id: &Id,
    ) -> Result<SelectionResult> {
        match relationship_selection {
            RelationshipSelection::PoolBased { pool, selection } => {
                // New pool-based resolution
                let effective_pool = Self::resolve_effective_pool(
                    store,
                    relationship_def,
                    pool.as_ref(),
                    database_id,
                    branch_id,
                ).await?;

                Self::resolve_selection(
                    store,
                    relationship_def,
                    &effective_pool,
                    selection.as_ref(),
                    branch_id,
                ).await
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
                    store,
                    relationship_def,
                    None,
                    database_id,
                    branch_id,
                ).await?;
                Ok(SelectionResult::Resolved(effective_pool))
            }
            RelationshipSelection::Filter { filter } => {
                // Apply filter directly
                let instances = store.list_instances_for_branch(database_id, branch_id, Some(filter.clone())).await?;
                let ids = instances.into_iter().map(|inst| inst.id).collect();
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