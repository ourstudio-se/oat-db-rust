use anyhow::{Result, anyhow};

use crate::model::{
    Instance, Schema, Expr, BoolExpr, Predicate, RuleSet, PropertyValue, 
    RelationshipSelection, ComparisonOp, Id
};
use crate::store::traits::Store;

pub struct Evaluator;

impl Evaluator {
    pub async fn evaluate_expr<S: Store>(
        store: &S,
        expr: &Expr,
        context: &Instance,
        _schema: &Schema,
    ) -> Result<serde_json::Value> {
        match expr {
            Expr::Add { left, right } => {
                let left_val = Self::evaluate_expr_simple(store, left, context).await?;
                let right_val = Self::evaluate_expr_simple(store, right, context).await?;
                Self::add_values(&left_val, &right_val)
            }
            Expr::Sub { left, right } => {
                let left_val = Self::evaluate_expr_simple(store, left, context).await?;
                let right_val = Self::evaluate_expr_simple(store, right, context).await?;
                Self::sub_values(&left_val, &right_val)
            }
            Expr::Mul { left, right } => {
                let left_val = Self::evaluate_expr_simple(store, left, context).await?;
                let right_val = Self::evaluate_expr_simple(store, right, context).await?;
                Self::mul_values(&left_val, &right_val)
            }
            Expr::Div { left, right } => {
                let left_val = Self::evaluate_expr_simple(store, left, context).await?;
                let right_val = Self::evaluate_expr_simple(store, right, context).await?;
                Self::div_values(&left_val, &right_val)
            }
            _ => Self::evaluate_expr_simple(store, expr, context).await,
        }
    }

    async fn evaluate_expr_simple<S: Store>(
        store: &S,
        expr: &Expr,
        context: &Instance,
    ) -> Result<serde_json::Value> {
        match expr {
            Expr::LitNumber { value } => Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(*value).unwrap()
            )),
            Expr::LitBool { value } => Ok(serde_json::Value::Bool(*value)),
            Expr::LitString { value } => Ok(serde_json::Value::String(value.clone())),
            Expr::Prop { prop } => {
                Self::get_property_value(context, prop)
            }
            Expr::RelProp { rel, prop } => {
                Self::evaluate_rel_prop(store, context, rel, prop).await
            }
            Expr::Sum { over, prop, r#where } => {
                Self::evaluate_sum(store, context, over, prop, r#where.as_ref()).await
            }
            Expr::Count { over, r#where } => {
                Self::evaluate_count(store, context, over, r#where.as_ref()).await
            }
            _ => Err(anyhow!("Unsupported expression type")),
        }
    }

    pub async fn evaluate_bool_expr<S: Store>(
        store: &S,
        expr: &BoolExpr,
        context: &Instance,
    ) -> Result<bool> {
        match expr {
            BoolExpr::All { predicates } => {
                for predicate in predicates {
                    if !Self::evaluate_predicate(store, predicate, context).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
            BoolExpr::Any { predicates } => {
                for predicate in predicates {
                    if Self::evaluate_predicate(store, predicate, context).await? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            BoolExpr::None { predicates } => {
                for predicate in predicates {
                    if Self::evaluate_predicate(store, predicate, context).await? {
                        return Ok(false);
                    }
                }
                Ok(true)
            }
        }
    }

    async fn evaluate_predicate<S: Store>(
        store: &S,
        predicate: &Predicate,
        context: &Instance,
    ) -> Result<bool> {
        match predicate {
            Predicate::Has { rel, ids, any } => {
                if let Some(selection) = context.relationships.get(rel) {
                    let target_ids = Self::resolve_selection(store, selection).await?;
                    
                    if let Some(check_ids) = ids {
                        if any.unwrap_or(false) {
                            Ok(check_ids.iter().any(|id| target_ids.contains(id)))
                        } else {
                            Ok(check_ids.iter().all(|id| target_ids.contains(id)))
                        }
                    } else {
                        Ok(!target_ids.is_empty())
                    }
                } else {
                    Ok(false)
                }
            }
            Predicate::PropEq { prop, value } => {
                let prop_value = Self::get_property_value(context, prop)?;
                Ok(prop_value == *value)
            }
            Predicate::PropNe { prop, value } => {
                let prop_value = Self::get_property_value(context, prop)?;
                Ok(prop_value != *value)
            }
            Predicate::PropGt { prop, value } => {
                let prop_value = Self::get_property_value(context, prop)?;
                Self::compare_values(&prop_value, value, &ComparisonOp::Gt)
            }
            Predicate::PropLt { prop, value } => {
                let prop_value = Self::get_property_value(context, prop)?;
                Self::compare_values(&prop_value, value, &ComparisonOp::Lt)
            }
            Predicate::PropContains { prop, value } => {
                let prop_value = Self::get_property_value(context, prop)?;
                if let serde_json::Value::String(s) = prop_value {
                    Ok(s.contains(value))
                } else {
                    Ok(false)
                }
            }
            Predicate::Count { rel, op, value } => {
                if let Some(selection) = context.relationships.get(rel) {
                    let target_ids = Self::resolve_selection(store, selection).await?;
                    let count = target_ids.len();
                    
                    match op {
                        ComparisonOp::Eq => Ok(count == *value),
                        ComparisonOp::Ne => Ok(count != *value),
                        ComparisonOp::Gt => Ok(count > *value),
                        ComparisonOp::Lt => Ok(count < *value),
                    }
                } else {
                    Ok(false)
                }
            }
            Predicate::HasTargets { rel, types } => {
                if let Some(selection) = context.relationships.get(rel) {
                    let target_ids = Self::resolve_selection(store, selection).await?;
                    
                    for id in target_ids {
                        if let Some(instance) = store.get_instance(&id).await? {
                            if !types.contains(&instance.class_id) {
                                return Ok(false);
                            }
                        }
                    }
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            Predicate::IncludesUniverse { rel: _ } => {
                Ok(true)
            }
        }
    }

    pub fn evaluate_rule_set(
        rule_set: &RuleSet,
        _context: &Instance,
    ) -> serde_json::Value {
        for branch in &rule_set.branches {
            if matches!(branch.when, BoolExpr::All { predicates: ref p } if p.is_empty()) {
                return branch.then.clone();
            }
        }
        
        rule_set.default.clone().unwrap_or(serde_json::Value::Null)
    }

    fn get_property_value(instance: &Instance, prop: &str) -> Result<serde_json::Value> {
        match instance.properties.get(prop) {
            Some(PropertyValue::Literal(value)) => Ok(value.clone()),
            Some(PropertyValue::Conditional(rule_set)) => {
                Ok(Self::evaluate_rule_set(rule_set, instance))
            }
            None => Err(anyhow!("Property '{}' not found", prop)),
        }
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

    async fn evaluate_rel_prop<S: Store>(
        store: &S,
        context: &Instance,
        rel: &str,
        prop: &str,
    ) -> Result<serde_json::Value> {
        if let Some(selection) = context.relationships.get(rel) {
            let target_ids = Self::resolve_selection(store, selection).await?;
            
            if let Some(first_id) = target_ids.first() {
                if let Some(target_instance) = store.get_instance(first_id).await? {
                    return Self::get_property_value(&target_instance, prop);
                }
            }
        }
        
        Err(anyhow!("Relationship '{}' not found or empty", rel))
    }

    async fn evaluate_sum<S: Store>(
        store: &S,
        context: &Instance,
        over: &str,
        prop: &str,
        _where_clause: Option<&BoolExpr>,
    ) -> Result<serde_json::Value> {
        if let Some(selection) = context.relationships.get(over) {
            let target_ids = Self::resolve_selection(store, selection).await?;
            let mut sum = 0.0;
            
            for id in target_ids {
                if let Some(instance) = store.get_instance(&id).await? {
                    if let Ok(value) = Self::get_property_value(&instance, prop) {
                        if let Some(num) = value.as_f64() {
                            sum += num;
                        }
                    }
                }
            }
            
            Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(sum).unwrap()
            ))
        } else {
            Ok(serde_json::Value::Number(
                serde_json::Number::from_f64(0.0).unwrap()
            ))
        }
    }

    async fn evaluate_count<S: Store>(
        store: &S,
        context: &Instance,
        over: &str,
        _where_clause: Option<&BoolExpr>,
    ) -> Result<serde_json::Value> {
        if let Some(selection) = context.relationships.get(over) {
            let target_ids = Self::resolve_selection(store, selection).await?;
            Ok(serde_json::Value::Number(
                serde_json::Number::from(target_ids.len())
            ))
        } else {
            Ok(serde_json::Value::Number(serde_json::Number::from(0)))
        }
    }

    fn add_values(left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                let result = l.as_f64().unwrap() + r.as_f64().unwrap();
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result).unwrap()
                ))
            }
            _ => Err(anyhow!("Cannot add non-numeric values")),
        }
    }

    fn sub_values(left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                let result = l.as_f64().unwrap() - r.as_f64().unwrap();
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result).unwrap()
                ))
            }
            _ => Err(anyhow!("Cannot subtract non-numeric values")),
        }
    }

    fn mul_values(left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                let result = l.as_f64().unwrap() * r.as_f64().unwrap();
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result).unwrap()
                ))
            }
            _ => Err(anyhow!("Cannot multiply non-numeric values")),
        }
    }

    fn div_values(left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                let right_val = r.as_f64().unwrap();
                if right_val == 0.0 {
                    return Err(anyhow!("Division by zero"));
                }
                let result = l.as_f64().unwrap() / right_val;
                Ok(serde_json::Value::Number(
                    serde_json::Number::from_f64(result).unwrap()
                ))
            }
            _ => Err(anyhow!("Cannot divide non-numeric values")),
        }
    }

    fn compare_values(
        left: &serde_json::Value,
        right: &serde_json::Value,
        op: &ComparisonOp,
    ) -> Result<bool> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                let left_val = l.as_f64().unwrap();
                let right_val = r.as_f64().unwrap();
                match op {
                    ComparisonOp::Gt => Ok(left_val > right_val),
                    ComparisonOp::Lt => Ok(left_val < right_val),
                    ComparisonOp::Eq => Ok((left_val - right_val).abs() < f64::EPSILON),
                    ComparisonOp::Ne => Ok((left_val - right_val).abs() >= f64::EPSILON),
                }
            }
            _ => Err(anyhow!("Cannot compare non-numeric values")),
        }
    }
}