use crate::class;
use crate::model::{
    generate_configuration_id, CommitData, ConfigurationArtifact, DefaultPool, Domain, Id,
    Instance, InstanceFilter, NewConfigurationArtifact, PipelinePhase, Quantifier, RelationshipDef,
    RelationshipSelection, Schema, SelectionSpec, SolveMetadata, SolveStatistics, SolverInfo,
};
use anyhow::Result;
use itertools::Itertools;
use pldag::Pldag;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::Instant;

/// Error type for solve pipeline failures
#[derive(Debug, thiserror::Error)]
pub enum SolveError {
    /// No solution could be found - constraints are unsatisfiable
    #[error("No solution found for objective(s): {objectives}. The constraints may be unsatisfiable or contradictory. Please review your class definitions, relationship quantifiers, and instance relationships.")]
    UnsatisfiableConstraints { objectives: String },

    /// Other errors (internal errors, data not found, etc.)
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl SolveError {
    /// Check if this is an unsatisfiable constraints error (client error, 422)
    pub fn is_unsatisfiable(&self) -> bool {
        matches!(self, SolveError::UnsatisfiableConstraints { .. })
    }
}

/// Helper function to determine the type string for a JSON value
fn get_json_value_type(value: &serde_json::Value) -> &'static str {
    match value {
        serde_json::Value::String(_) => "string",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Null => "null",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    }
}

/// The solve pipeline orchestrates the complete solution process
/// From CommitData + target instance → ConfigurationArtifact with ILP-ready data
pub struct SolvePipeline<'a> {
    commit_data: &'a CommitData,
}

impl<'a> SolvePipeline<'a> {
    /// Create a new solve pipeline from commit data
    pub fn new(commit_data: &'a CommitData) -> Self {
        Self { commit_data }
    }

    /// Execute the solve pipeline with multiple objective sets and derived properties
    ///
    /// This is a convenience wrapper around solve_instance_with_constraints that doesn't add custom constraints
    pub fn solve_instance_with_multiple_objectives_and_derived_properties(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objective_sets: Vec<(String, HashMap<String, f64>)>,
        derived_properties: Option<Vec<String>>,
    ) -> Result<Vec<(String, ConfigurationArtifact)>, SolveError> {
        // Delegate to solve_instance_with_constraints with a no-op constraint function
        self.solve_instance_with_constraints(
            request,
            target_instance_id,
            objective_sets,
            derived_properties,
            |_model, _mappings| Ok(()),
        )
    }

    /// Execute the solve pipeline with multiple objective sets, derived properties, and custom constraints
    ///
    /// This variant allows you to add custom pldag constraints before solving.
    /// The constraint_fn receives a mutable reference to the Pldag model and the IdMappings,
    /// allowing you to add constraints using methods like set_gelineq.
    pub fn solve_instance_with_constraints<F>(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objective_sets: Vec<(String, HashMap<String, f64>)>,
        derived_properties: Option<Vec<String>>,
        constraint_fn: F,
    ) -> Result<Vec<(String, ConfigurationArtifact)>, SolveError>
    where
        F: FnOnce(&mut Pldag, &IdMappings) -> Result<(), SolveError>,
    {
        let start_time = Instant::now();
        let mut phase_timings = Vec::new();

        // Step 1: Get instances and schema from commit data
        let phase_start = Instant::now();
        let all_instances = self.commit_data.instances.clone();
        let schema = &self.commit_data.schema;

        // Find target instance (just validate it exists)
        let _target_instance = all_instances
            .iter()
            .find(|i| i.id == target_instance_id)
            .ok_or_else(|| {
                anyhow::anyhow!("Target instance {} not found in commit", target_instance_id)
            })?
            .clone();
        phase_timings.push(("fetch_data", phase_start.elapsed()));

        // Step 2: Build dependency tree and filter instances early
        let phase_start = Instant::now();
        let dependencies = self.get_instance_dependencies(&target_instance_id, &all_instances)?;
        let instances: Vec<Instance> = all_instances
            .into_iter()
            .filter(|inst| dependencies.contains(&inst.id))
            .collect();
        phase_timings.push(("filter_dependencies", phase_start.elapsed()));

        // Step 3: Resolve all pool filters and materialize relationships for filtered instances
        let phase_start = Instant::now();
        let resolved_instances = self.resolve_all_relationships(instances, schema)?;
        phase_timings.push(("resolve_relationships", phase_start.elapsed()));

        // Step 4: Setup Pldag model
        let phase_start = Instant::now();
        let (mut model, id_mappings) =
            self.setup_pldag_model(&target_instance_id, &resolved_instances, schema)?;
        phase_timings.push(("setup_pldag", phase_start.elapsed()));

        // Step 4.5: Apply custom constraints
        let phase_start = Instant::now();
        constraint_fn(&mut model, &id_mappings)?;
        phase_timings.push(("apply_constraints", phase_start.elapsed()));

        // Step 5: Map all objectives and solve with Pldag
        let phase_start = Instant::now();
        let solutions = self.solve_with_pldag_batch(
            model,
            id_mappings.get_pldag_id(&target_instance_id).unwrap(),
            &objective_sets,
            &id_mappings,
        )?;
        phase_timings.push(("solve", phase_start.elapsed()));

        // Step 5.5: Evaluate derived properties for all instances
        let instance_class = schema
            .classes
            .iter()
            .find(|c| c.id == _target_instance.class_id)
            .unwrap();

        // Step 6: Compile solutions into artifacts
        let mut results = Vec::new();
        let elapsed = start_time.elapsed();
        let total_time = std::cmp::max(1, elapsed.as_micros() / 1000) as u64;

        for ((objective_id, _), solution) in objective_sets.iter().zip(solutions.iter()) {
            let mut artifact = self.compile_artifact(
                request.clone(),
                target_instance_id.clone(),
                resolved_instances.clone(),
                solution.clone(),
                &id_mappings,
                total_time,
                &phase_timings,
            )?;

            // Only calculate derived properties if requested
            if let Some(requested_props) = &derived_properties {
                if !requested_props.is_empty() {
                    for derived_property in instance_class.derived.iter() {
                        // Only calculate if this property was requested
                        if requested_props.contains(&derived_property.name) {
                            match &derived_property.fn_short {
                                Some(short) => {
                                    let derived_value = self.resolve_derived_property(
                                        &_target_instance,
                                        &resolved_instances,
                                        solution,
                                        &short,
                                    );
                                    if let Some(value) = derived_value {
                                        let mut property_map = HashMap::new();
                                        property_map.insert("value".to_string(), value.clone());

                                        // Determine and add type based on the JSON value type
                                        let type_str = get_json_value_type(&value);
                                        property_map.insert(
                                            "type".to_string(),
                                            serde_json::Value::String(type_str.to_string()),
                                        );

                                        artifact
                                            .derived_properties
                                            .insert(derived_property.name.clone(), property_map);
                                    }
                                }
                                None => continue,
                            }
                        }
                    }
                }
            }

            results.push((objective_id.clone(), artifact));
        }

        Ok(results)
    }

    // Remove fetch_commit_data method as we now have commit data directly

    /// Step 2: Resolve all relationships to concrete instance IDs
    pub fn resolve_all_relationships(
        &self,
        mut instances: Vec<Instance>,
        schema: &Schema,
    ) -> Result<Vec<Instance>> {
        // Create a map of all instances by type for quick lookup
        let mut instances_by_type: HashMap<String, Vec<Instance>> = HashMap::new();
        for instance in &instances {
            instances_by_type
                .entry(instance.class_id.clone())
                .or_default()
                .push(instance.clone());
        }

        // Process each instance
        for instance in instances.iter_mut() {
            // Get class definition
            let class_def = schema.get_class_by_id(&instance.class_id).ok_or_else(|| {
                anyhow::anyhow!("Class {} not found in schema", instance.class_id)
            })?;

            // Process each relationship in the class definition
            for rel_def in &class_def.relationships {
                // Check if instance already has this relationship
                let selection = instance
                    .relationships
                    .get(&rel_def.id)
                    .cloned()
                    .unwrap_or_else(|| {
                        // Use schema default pool
                        self.create_selection_from_default_pool(&rel_def.default_pool)
                    });

                // Resolve the selection to concrete IDs
                let resolved_ids =
                    self.resolve_selection(&selection, rel_def, &instances_by_type)?;

                // Update instance with resolved IDs
                instance.relationships.insert(
                    rel_def.id.clone(),
                    RelationshipSelection::SimpleIds(resolved_ids),
                );
            }
        }

        Ok(instances)
    }

    /// Create a selection from default pool definition
    fn create_selection_from_default_pool(
        &self,
        default_pool: &DefaultPool,
    ) -> RelationshipSelection {
        match default_pool {
            DefaultPool::All => RelationshipSelection::PoolBased {
                pool: None,
                selection: Some(SelectionSpec::Unresolved),
            },
            DefaultPool::None => RelationshipSelection::SimpleIds(vec![]),
            DefaultPool::Filter { types, filter } => RelationshipSelection::PoolBased {
                pool: Some(InstanceFilter {
                    types: types.clone(),
                    where_clause: filter.as_ref().and_then(|f| f.where_clause.clone()),
                    sort: filter.as_ref().and_then(|f| f.sort.clone()),
                    limit: filter.as_ref().and_then(|f| f.limit),
                }),
                selection: Some(SelectionSpec::Unresolved),
            },
        }
    }

    /// Resolve a relationship selection to concrete instance IDs
    fn resolve_selection(
        &self,
        selection: &RelationshipSelection,
        rel_def: &RelationshipDef,
        instances_by_type: &HashMap<String, Vec<Instance>>,
    ) -> Result<Vec<Id>> {
        match selection {
            RelationshipSelection::SimpleIds(ids) => Ok(ids.clone()),
            RelationshipSelection::Ids { ids } => Ok(ids.clone()),
            RelationshipSelection::PoolBased { pool, .. } => {
                // Get candidate instances from target types
                let mut candidates = Vec::new();
                for target_type in &rel_def.targets {
                    if let Some(type_instances) = instances_by_type.get(target_type) {
                        candidates.extend(type_instances.clone());
                    }
                }

                // Apply pool filter if present
                if let Some(pool_filter) = pool {
                    candidates = self.apply_pool_filter(candidates, pool_filter)?;
                }

                // Extract IDs
                Ok(candidates.into_iter().map(|i| i.id).collect())
            }
            _ => Ok(vec![]),
        }
    }

    /// Apply pool filter to instances
    fn apply_pool_filter(
        &self,
        mut instances: Vec<Instance>,
        filter: &InstanceFilter,
    ) -> Result<Vec<Instance>> {
        // Apply where clause
        if let Some(where_clause) = &filter.where_clause {
            instances = crate::logic::instance_filter::filter_instances(instances, where_clause);
        }

        // Apply sort (simplified for now - just sort by ID if requested)
        if let Some(sort_field) = &filter.sort {
            instances.sort_by(|a, b| {
                if sort_field == "id" || sort_field == "$.id" {
                    a.id.cmp(&b.id)
                } else if let (Some(a_val), Some(b_val)) = (
                    self.get_property_value(a, sort_field.trim_start_matches("$.")),
                    self.get_property_value(b, sort_field.trim_start_matches("$.")),
                ) {
                    // Try to sort by numeric value if possible
                    match (a_val.as_f64(), b_val.as_f64()) {
                        (Some(a_num), Some(b_num)) => a_num
                            .partial_cmp(&b_num)
                            .unwrap_or(std::cmp::Ordering::Equal),
                        _ => std::cmp::Ordering::Equal,
                    }
                } else {
                    std::cmp::Ordering::Equal
                }
            });
        }

        // Apply limit
        if let Some(limit) = filter.limit {
            instances.truncate(limit);
        }

        Ok(instances)
    }

    /// Get property value from instance
    fn get_property_value(&self, instance: &Instance, field: &str) -> Option<serde_json::Value> {
        instance.properties.get(field).and_then(|prop| match prop {
            crate::model::PropertyValue::Literal(typed_val) => Some(typed_val.value.clone()),
            _ => None,
        })
    }

    /// Step 4: Setup Pldag model with topological sort
    pub fn setup_pldag_model(
        &self,
        _main_instance_id: &Id,
        instances: &[Instance],
        schema: &Schema,
    ) -> Result<(Pldag, IdMappings)> {
        let mut model = Pldag::new();
        let mut id_mappings = IdMappings::new();

        // Step 4.1: Topological sort the instances
        let sorted_instances = self.topological_sort(instances)?;

        // Step 4.2: Process instances in topological order
        for instance in sorted_instances {
            let class_def = schema
                .get_class_by_id(&instance.class_id)
                .ok_or_else(|| anyhow::anyhow!("Class {} not found", instance.class_id))?;

            // Step 4.3: Determine if instance is primitive or composite
            if class_def.relationships.is_empty() {
                // Primitive instance
                let domain = if let Some(domain) = &instance.domain {
                    domain.clone()
                } else {
                    // Use class domain constraint since every class must have a domain
                    class_def.domain_constraint.clone()
                };
                model.set_primitive(&instance.id, (domain.lower as i64, domain.upper as i64));
                id_mappings.register_primitive(&instance.id);
            } else {
                // Composite instance with relationships
                let composite_id = self.setup_composite_instance(
                    &mut model,
                    instance,
                    class_def,
                    &mut id_mappings,
                )?;
                id_mappings.register_composite(&instance.id, &composite_id);
            }
        }

        Ok((model, id_mappings))
    }

    /// Get all dependencies of an instance (including transitive dependencies)
    pub fn get_instance_dependencies(
        &self,
        target_id: &Id,
        instances: &[Instance],
    ) -> Result<HashSet<Id>> {
        let mut dependencies = HashSet::new();
        let mut to_process = VecDeque::new();

        // Start with the target instance itself
        dependencies.insert(target_id.clone());
        to_process.push_back(target_id.clone());

        // Build a map for quick instance lookup
        let instance_map: HashMap<&Id, &Instance> =
            instances.iter().map(|inst| (&inst.id, inst)).collect();

        // Process dependencies breadth-first
        while let Some(current_id) = to_process.pop_front() {
            if let Some(instance) = instance_map.get(&current_id) {
                // Add all relationship targets as dependencies
                for selection in instance.relationships.values() {
                    if let RelationshipSelection::SimpleIds(target_ids) = selection {
                        for target_id in target_ids {
                            if !dependencies.contains(target_id)
                                && instance_map.contains_key(target_id)
                            {
                                dependencies.insert(target_id.clone());
                                to_process.push_back(target_id.clone());
                            }
                        }
                    }
                }
            }
        }

        Ok(dependencies)
    }

    fn resolve_derived_property(
        &self,
        instance: &Instance,
        other_instances: &[Instance],
        solution: &HashMap<String, i64>,
        fn_short: &crate::model::FnShort,
    ) -> Option<serde_json::Value> {
        // Placeholder for future implementation
        let dependencies = self
            .get_instance_dependencies(&instance.id, other_instances)
            .ok()?;

        // Collect all property values to determine the type
        let mut values = Vec::new();

        for id in dependencies.iter() {
            if let Some(inst) = other_instances.iter().find(|inst| &inst.id == id) {
                if let Some(class_def) = self.commit_data.schema.get_class_by_id(&inst.class_id) {
                    if let Some(prop_value) = inst.properties.get(fn_short.property.as_str()) {
                        let value = match prop_value {
                            crate::model::PropertyValue::Literal(typed_val) => {
                                typed_val.value.clone()
                            }
                            crate::model::PropertyValue::Conditional(rule_set) => {
                                // For conditional properties, evaluate the rule
                                use crate::logic::evaluate_simple::SimpleEvaluator;
                                SimpleEvaluator::evaluate_rule_set(rule_set, inst)
                            }
                        };
                        if solution.get(&id.to_string()) >= Some(&1) {
                            values.push(value.clone());
                        }
                    } else if let Some(default_prop) = class_def
                        .properties
                        .iter()
                        .find(|p| p.name == fn_short.property)
                    {
                        // Return the default value if property is missing
                        if let Some(default_value) = &default_prop.value {
                            if solution.get(&id.to_string()) >= Some(&1) {
                                values.push(default_value.clone());
                            }
                        }
                    }
                }
            }
        }

        if values.is_empty() {
            return Some(serde_json::Value::Null);
        }

        // Determine the predominant type and perform appropriate operation
        let has_string = values.iter().any(|v| v.is_string());

        if has_string {
            // String concatenation mode
            if fn_short.method == "sum" || fn_short.method == "concat" {
                // Check that if there are derivation_arguments, that it contains a separator
                // If not then we use a comma as default
                let separator = fn_short
                    .args
                    .as_ref()
                    .and_then(|args| {
                        args.iter().find_map(|arg| match arg {
                            crate::model::FnArg::Named { key, value } if key == "separator" => {
                                match value {
                                    crate::model::FnArgValue::String(s) => Some(s.as_str()),
                                    _ => Some(","),
                                }
                            }
                            _ => None,
                        })
                    })
                    .unwrap_or(",");

                let concatenated = values
                    .into_iter()
                    .filter_map(|v| match v {
                        serde_json::Value::String(s) if !s.is_empty() => Some(s),
                        serde_json::Value::String(_) => None, // Filter out empty strings
                        serde_json::Value::Number(n) => Some(n.to_string()),
                        serde_json::Value::Bool(b) => Some(b.to_string()),
                        serde_json::Value::Null => None, // Filter out null values
                        _ => Some(serde_json::to_string(&v).unwrap_or_default()),
                    })
                    .collect::<Vec<String>>()
                    .iter()
                    .sorted_by(|a, b| a.cmp(b)) // Sort alphabetically
                    .join(separator);

                return Some(serde_json::Value::String(concatenated));
            } else if fn_short.method == "mean" {
                // Mean is not defined for strings
                return None;
            }
        } else {
            // Numeric addition mode
            if fn_short.method == "sum" {
                let sum = values
                    .into_iter()
                    .map(|v| match v {
                        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                        serde_json::Value::Bool(true) => 1.0,
                        serde_json::Value::Bool(false) => 0.0,
                        _ => 0.0,
                    })
                    .sum::<f64>();

                return Some(serde_json::json!(sum));
            } else if fn_short.method == "mean" {
                let sum: f64 = values
                    .iter()
                    .map(|v| match v {
                        serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
                        serde_json::Value::Bool(true) => 1.0,
                        serde_json::Value::Bool(false) => 0.0,
                        _ => 0.0,
                    })
                    .sum();
                let count = values.len() as f64;
                if count > 0.0 {
                    return Some(serde_json::json!(sum / count));
                } else {
                    return Some(serde_json::json!(0));
                }
            }
        }
        None
    }

    /// Topological sort instances based on relationship dependencies
    fn topological_sort<'b>(&self, instances: &'b [Instance]) -> Result<Vec<&'b Instance>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut graph: HashMap<&str, Vec<&str>> = HashMap::new();
        let instance_map: HashMap<&str, &Instance> =
            instances.iter().map(|i| (i.id.as_str(), i)).collect();

        // Initialize in-degree
        for instance in instances {
            in_degree.insert(&instance.id, 0);
            graph.insert(&instance.id, Vec::new());
        }

        // Build dependency graph
        for instance in instances {
            for selection in instance.relationships.values() {
                if let RelationshipSelection::SimpleIds(target_ids) = selection {
                    for target_id in target_ids {
                        if instance_map.contains_key(target_id.as_str()) {
                            // instance depends on target_id
                            graph
                                .get_mut(target_id.as_str())
                                .unwrap()
                                .push(&instance.id);
                            *in_degree.get_mut(instance.id.as_str()).unwrap() += 1;
                        }
                    }
                }
            }
        }

        // Kahn's algorithm
        let mut queue = VecDeque::new();
        let mut result = Vec::new();

        // Find all nodes with in-degree 0
        for (id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(*id);
            }
        }

        while let Some(current) = queue.pop_front() {
            if let Some(instance) = instance_map.get(current) {
                result.push(*instance);

                // Reduce in-degree for dependents
                if let Some(dependents) = graph.get(current) {
                    for &dependent in dependents {
                        let degree = in_degree.get_mut(dependent).unwrap();
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent);
                        }
                    }
                }
            }
        }

        // Check for cycles
        if result.len() != instances.len() {
            return Err(anyhow::anyhow!(
                "Circular dependency detected in instance relationships"
            ));
        }

        Ok(result)
    }

    /// Setup a composite instance in Pldag
    fn setup_composite_instance(
        &self,
        model: &mut Pldag,
        instance: &Instance,
        class_def: &crate::model::ClassDef,
        id_mappings: &mut IdMappings,
    ) -> Result<String> {
        let mut constraint_ids = Vec::new();

        // Process each relationship
        for rel_def in &class_def.relationships {
            if let Some(RelationshipSelection::SimpleIds(target_ids)) =
                instance.relationships.get(&rel_def.id)
            {
                // Map target IDs to Pldag variables
                let pldag_vars: Vec<&str> = target_ids
                    .iter()
                    .map(|id| {
                        // If id in id_mappings, get corresponding pldag id
                        // else return id
                        id_mappings.get_pldag_id(id).unwrap_or(id)
                    })
                    .collect();

                // Create constraint based on quantifier
                let constraint_id = match &rel_def.quantifier {
                    Quantifier::One => {
                        if pldag_vars.len() == 1 {
                            // For a single variable with One quantifier, just use the variable itself
                            // This allows it to work properly with base.op like Imply
                            pldag_vars[0].to_string()
                        } else {
                            // For multiple variables, enforce exactly one must be selected
                            model.set_equal(pldag_vars, 1)
                                .ok_or_else(|| anyhow::anyhow!(
                                    "Failed to create 'equal' constraint for relationship '{}' (One quantifier)",
                                    rel_def.id
                                ))?
                        }
                    }
                    Quantifier::AtLeast(n) => model.set_atleast(pldag_vars, *n as i64)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Failed to create 'at least {}' constraint for relationship '{}'",
                            n, rel_def.id
                        ))?,
                    Quantifier::AtMost(n) => model.set_atmost(pldag_vars, *n as i64)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Failed to create 'at most {}' constraint for relationship '{}'",
                            n, rel_def.id
                        ))?,
                    Quantifier::Exactly(n) => model.set_equal(pldag_vars, *n as i64)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Failed to create 'exactly {}' constraint for relationship '{}'",
                            n, rel_def.id
                        ))?,
                    Quantifier::Range(min, max) => {
                        let min_id = model.set_atleast(pldag_vars.clone(), *min as i64)
                            .ok_or_else(|| anyhow::anyhow!(
                                "Failed to create 'at least {}' constraint for relationship '{}' (Range)",
                                min, rel_def.id
                            ))?;
                        let max_id = model.set_atmost(pldag_vars, *max as i64)
                            .ok_or_else(|| anyhow::anyhow!(
                                "Failed to create 'at most {}' constraint for relationship '{}' (Range)",
                                max, rel_def.id
                            ))?;
                        model.set_and(vec![min_id, max_id])
                            .ok_or_else(|| anyhow::anyhow!(
                                "Failed to create 'and' constraint for relationship '{}' (Range)",
                                rel_def.id
                            ))?
                    }
                    Quantifier::Optional => model.set_atleast(pldag_vars, 0)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Failed to create 'at least 0' constraint for relationship '{}' (Optional)",
                            rel_def.id
                        ))?,
                    Quantifier::Any => model.set_or(pldag_vars)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Failed to create 'or' constraint for relationship '{}' (Any)",
                            rel_def.id
                        ))?,
                    Quantifier::All => model.set_and(pldag_vars)
                        .ok_or_else(|| anyhow::anyhow!(
                            "Failed to create 'and' constraint for relationship '{}' (All)",
                            rel_def.id
                        ))?,
                };

                constraint_ids.push(constraint_id);
            }
        }

        // Combine all relationship constraints
        match class_def.base.op {
            class::BaseOp::All => model.set_and(constraint_ids).ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to create 'and' constraint for class '{}' base operation",
                    class_def.id
                )
            }),
            class::BaseOp::Any => model.set_or(constraint_ids).ok_or_else(|| {
                anyhow::anyhow!(
                    "Failed to create 'or' constraint for class '{}' base operation",
                    class_def.id
                )
            }),
            class::BaseOp::AtLeast => {
                let val = class_def.base.val.unwrap_or(0);
                model
                    .set_atleast(
                        constraint_ids.iter().map(|id| id.as_str()).collect(),
                        val as i64,
                    )
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                    "Failed to create 'at least {}' constraint for class '{}' base operation",
                    val, class_def.id
                )
                    })
            }
            class::BaseOp::AtMost => {
                let val = class_def.base.val.unwrap_or(0);
                model
                    .set_atmost(
                        constraint_ids.iter().map(|id| id.as_str()).collect(),
                        val as i64,
                    )
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                    "Failed to create 'at most {}' constraint for class '{}' base operation",
                    val, class_def.id
                )
                    })
            }
            class::BaseOp::Exactly => {
                let val = class_def.base.val.unwrap_or(0);
                model
                    .set_equal(
                        constraint_ids.iter().map(|id| id.as_str()).collect(),
                        val as i64,
                    )
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                    "Failed to create 'exactly {}' constraint for class '{}' base operation",
                    val, class_def.id
                )
                    })
            }
            class::BaseOp::Imply => {
                if constraint_ids.len() != 2 {
                    Err(anyhow::anyhow!(
                        "Imply operator requires exactly 2 constraints"
                    ))
                } else {
                    model
                        .set_imply(&constraint_ids[0], &constraint_ids[1])
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Failed to create 'imply' constraint for class '{}' base operation",
                                class_def.id
                            )
                        })
                }
            }
            class::BaseOp::Equiv => {
                if constraint_ids.len() != 2 {
                    Err(anyhow::anyhow!(
                        "Equiv operator requires exactly 2 constraints"
                    ))
                } else {
                    model
                        .set_equiv(&constraint_ids[0], &constraint_ids[1])
                        .ok_or_else(|| {
                            anyhow::anyhow!(
                                "Failed to create 'equiv' constraint for class '{}' base operation",
                                class_def.id
                            )
                        })
                }
            }
        }
    }

    /// Step 4: Map objectives from our IDs to Pldag IDs
    fn map_objectives_to_pldag<'b>(
        &self,
        objectives: &HashMap<String, f64>,
        id_mappings: &'b IdMappings,
    ) -> Result<HashMap<&'b str, f64>> {
        let mut pldag_objectives = HashMap::new();

        for (instance_id, weight) in objectives {
            if let Some(pldag_id) = id_mappings.get_pldag_id(instance_id) {
                pldag_objectives.insert(pldag_id, *weight);
            }
        }

        Ok(pldag_objectives)
    }

    /// Solve with Pldag
    #[allow(dead_code)]
    fn solve_with_pldag(
        &self,
        mut model: Pldag,
        objectives: HashMap<&str, f64>,
        pldag_to_our: &HashMap<String, String>,
    ) -> Result<HashMap<String, i64>, SolveError> {
        // Create root constraint (all instances must be valid)
        let all_vars: Vec<&str> = pldag_to_our.keys().map(|s| s.as_str()).collect();
        let root = if all_vars.is_empty() {
            model.set_and::<String>(vec![]).ok_or_else(|| {
                SolveError::Other(anyhow::anyhow!("Failed to create empty root constraint"))
            })?
        } else {
            model
                .set_and(all_vars.into_iter().map(|s| s.to_string()).collect())
                .ok_or_else(|| {
                    SolveError::Other(anyhow::anyhow!("Failed to create root 'and' constraint"))
                })?
        };

        // Solve
        let objectives_vec = if objectives.is_empty() {
            vec![HashMap::new()]
        } else {
            vec![objectives]
        };

        let solutions = model.solve(
            objectives_vec,
            HashMap::from_iter(vec![(root.as_str(), (1, 1))]),
            true,
        );

        // Extract first solution
        if let Some(Some(solution)) = solutions.first() {
            let mut our_solution = HashMap::new();
            for (pldag_id, (value, _)) in solution {
                if let Some(our_id) = pldag_to_our.get(pldag_id) {
                    our_solution.insert(our_id.clone(), *value);
                }
            }
            Ok(our_solution)
        } else {
            Err(SolveError::UnsatisfiableConstraints {
                objectives: "default".to_string(),
            })
        }
    }

    /// Solve with Pldag for multiple objective sets efficiently
    fn solve_with_pldag_batch(
        &self,
        model: Pldag,
        root: &str,
        objective_sets: &[(String, HashMap<String, f64>)],
        id_mappings: &IdMappings,
    ) -> Result<Vec<HashMap<String, i64>>, SolveError> {
        // Map all objective sets to Pldag IDs
        let mut pldag_objectives = Vec::new();
        for (_, objectives) in objective_sets {
            let mapped = self.map_objectives_to_pldag(objectives, id_mappings)?;
            if mapped.is_empty() {
                pldag_objectives.push(HashMap::new());
            } else {
                pldag_objectives.push(mapped);
            }
        }

        // Solve all objectives at once
        let solutions = model.solve(
            pldag_objectives,
            HashMap::from_iter(vec![(root, (1, 1))]),
            true,
        );

        // Extract solutions and map back to our IDs
        let mut our_solutions = Vec::new();
        let mut unsolvable_objectives = Vec::new();

        // Get all those pldag IDs that were preset to 0 before solving
        let preset_zero_ids: HashSet<&String> = model
            .nodes
            .iter()
            .filter_map(|(id, node)| match &node.expression {
                pldag::BoolExpression::Composite(_) => None,
                pldag::BoolExpression::Primitive(bound) => {
                    if bound.0 == 0 && bound.1 == 0 {
                        Some(id)
                    } else {
                        None
                    }
                }
            })
            .collect();

        for (idx, solution_opt) in solutions.into_iter().enumerate() {
            if let Some(solution) = solution_opt {
                let mut our_solution = HashMap::new();
                for (pldag_id, (value, _)) in solution {
                    if preset_zero_ids.contains(&pldag_id) {
                        // Skip those IDs that were preset to 0
                        continue;
                    }
                    if let Some(our_id) = id_mappings.pldag_to_our.get(&pldag_id) {
                        our_solution.insert(our_id.clone(), value);
                    }
                }
                our_solutions.push(our_solution);
            } else {
                // Track which objective failed to find a solution
                if let Some((objective_id, _)) = objective_sets.get(idx) {
                    unsolvable_objectives.push(objective_id.clone());
                }
                our_solutions.push(HashMap::new());
            }
        }

        // Ensure we have a solution for each objective set
        while our_solutions.len() < objective_sets.len() {
            our_solutions.push(HashMap::new());
        }

        // If any objectives were unsolvable, return an error
        if !unsolvable_objectives.is_empty() {
            return Err(SolveError::UnsatisfiableConstraints {
                objectives: unsolvable_objectives.join(", "),
            });
        }

        Ok(our_solutions)
    }

    /// Step 5: Compile solution into artifact
    fn compile_artifact(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        mut instances: Vec<Instance>,
        solution: HashMap<String, i64>,
        id_mappings: &IdMappings,
        total_time_ms: u64,
        phase_timings: &[(&str, std::time::Duration)],
    ) -> Result<ConfigurationArtifact> {
        // Update instance domains based on solution. Exclude the instance if not in solution.
        let solution_instances: Vec<Instance> = instances
            .iter()
            .filter_map(|inst| {
                if let Some(value) = solution.get(&inst.id) {
                    let mut updated_instance = inst.clone();
                    updated_instance.domain = Some(Domain {
                        lower: *value as i32,
                        upper: *value as i32,
                    });
                    Some(updated_instance)
                } else {
                    None
                }
            })
            .collect();

        // Create artifact
        let artifact_id = generate_configuration_id(
            request.resolution_context.commit_hash.as_ref(),
            &HashMap::new(),
            &target_instance_id,
        );

        let mut artifact = ConfigurationArtifact::new(
            artifact_id,
            request.resolution_context.clone(),
            request.user_metadata.clone(),
        );

        artifact.set_configuration(solution_instances);

        // Set metadata
        artifact.solve_metadata = SolveMetadata {
            total_time_ms,
            pipeline_phases: phase_timings
                .iter()
                .map(|(name, duration)| PipelinePhase {
                    name: name.to_string(),
                    // Convert duration to milliseconds, ensure at least 1ms for very fast operations
                    duration_ms: std::cmp::max(1, duration.as_micros() / 1000) as u64,
                    details: None,
                })
                .collect(),
            solver_info: Some(SolverInfo {
                name: "pldag".to_string(),
                version: None,
                config: HashMap::new(),
            }),
            statistics: SolveStatistics {
                total_instances: artifact.instance_count(),
                variable_instances_resolved: solution.len(),
                conditional_properties_evaluated: 0,
                derived_properties_calculated: 0,
                ilp_variables: Some(id_mappings.count()),
                ilp_constraints: None,
                peak_memory_bytes: None,
            },
            issues: vec![],
        };

        Ok(artifact)
    }
}

/// Helper struct to manage ID mappings between our system and Pldag
pub struct IdMappings {
    /// Our instance ID -> Pldag variable ID
    pub our_to_pldag: HashMap<String, String>,
    /// Pldag variable ID -> Our instance ID
    pub pldag_to_our: HashMap<String, String>,
    /// Set of primitive instance IDs (directly used in Pldag)
    pub primitives: HashSet<String>,
}

impl IdMappings {
    fn new() -> Self {
        Self {
            our_to_pldag: HashMap::new(),
            pldag_to_our: HashMap::new(),
            primitives: HashSet::new(),
        }
    }

    fn register_primitive(&mut self, instance_id: &str) {
        self.primitives.insert(instance_id.to_string());
        self.our_to_pldag
            .insert(instance_id.to_string(), instance_id.to_string());
        self.pldag_to_our
            .insert(instance_id.to_string(), instance_id.to_string());
    }

    fn register_composite(&mut self, instance_id: &str, pldag_id: &str) {
        self.our_to_pldag
            .insert(instance_id.to_string(), pldag_id.to_string());
        self.pldag_to_our
            .insert(pldag_id.to_string(), instance_id.to_string());
    }

    pub fn get_pldag_id(&self, our_id: &str) -> Option<&str> {
        self.our_to_pldag.get(our_id).map(|s| s.as_str())
    }

    fn count(&self) -> usize {
        self.our_to_pldag.len()
    }
}
