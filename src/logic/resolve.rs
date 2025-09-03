use anyhow::Result;
use crate::model::{BoolExpr, InstanceFilter, Instance, Id};
use crate::store::traits::Store;

pub struct Resolver;

impl Resolver {
    pub async fn resolve_filter<S: Store>(
        store: &S,
        filter: &InstanceFilter,
    ) -> Result<Vec<Id>> {
        let mut instances = store.list_instances(None).await?;

        if let Some(types) = &filter.types {
            instances.retain(|instance| types.contains(&instance.class_id));
        }

        if let Some(where_clause) = &filter.where_clause {
            let mut filtered_instances = Vec::new();
            for instance in instances {
                if Self::evaluate_where_clause(store, where_clause, &instance).await? {
                    filtered_instances.push(instance);
                }
            }
            instances = filtered_instances;
        }

        if let Some(limit) = filter.limit {
            instances.truncate(limit);
        }

        Ok(instances.into_iter().map(|i| i.id).collect())
    }

    async fn evaluate_where_clause<S: Store>(
        store: &S,
        expr: &BoolExpr,
        instance: &Instance,
    ) -> Result<bool> {
        crate::logic::Evaluator::evaluate_bool_expr(store, expr, instance).await
    }
}