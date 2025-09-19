use crate::class;
use crate::model::{
    generate_configuration_id, CommitData, ConfigurationArtifact, DefaultPool, Domain, Id,
    Instance, InstanceFilter, NewConfigurationArtifact, PipelinePhase, Quantifier, RelationshipDef,
    RelationshipSelection, Schema, SelectionSpec, SolveMetadata, SolveStatistics, SolverInfo,
};
use anyhow::Result;
use pldag::Pldag;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

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

    /// Execute the solve pipeline for a specific instance (ILP-based approach)
    pub fn solve_instance(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
    ) -> Result<ConfigurationArtifact> {
        self.solve_instance_with_objectives(request, target_instance_id, HashMap::new())
    }

    /// Execute the solve pipeline with objectives for combinatorial search
    pub fn solve_instance_with_objectives(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objectives: HashMap<String, f64>,
    ) -> Result<ConfigurationArtifact> {
        // Delegate to batch method with single objective set
        let results = self.solve_instance_with_multiple_objectives(
            request,
            target_instance_id,
            vec![("default".to_string(), objectives)],
        )?;

        // Extract the single result
        results
            .into_iter()
            .next()
            .map(|(_, artifact)| artifact)
            .ok_or_else(|| anyhow::anyhow!("No solution returned from batch solve"))
    }

    /// Execute the solve pipeline with multiple objective sets for efficient batch solving
    pub fn solve_instance_with_multiple_objectives(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objective_sets: Vec<(String, HashMap<String, f64>)>, // (objective_id, objectives)
    ) -> Result<Vec<(String, ConfigurationArtifact)>> {
        let start_time = Instant::now();

        // Step 1: Get instances and schema from commit data
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

        // Step 2: Build dependency tree and filter instances early
        let dependencies = self.get_instance_dependencies(&target_instance_id, &all_instances)?;
        let instances: Vec<Instance> = all_instances
            .into_iter()
            .filter(|inst| dependencies.contains(&inst.id))
            .collect();

        // Step 3: Resolve all pool filters and materialize relationships for filtered instances
        let resolved_instances = self.resolve_all_relationships(instances, schema)?;

        // Step 4: Setup Pldag model (no longer needs to filter)
        let (model, id_mappings) =
            self.setup_pldag_model(&target_instance_id, &resolved_instances, schema)?;

        // Step 5: Map all objectives and solve with Pldag (EFFICIENTLY)
        let solutions = self.solve_with_pldag_batch(
            model,
            id_mappings.get_pldag_id(&target_instance_id).unwrap(),
            &objective_sets,
            &id_mappings,
            &id_mappings.pldag_to_our,
        )?;

        // Step 6: Compile solutions into artifacts
        let mut results = Vec::new();
        let total_time = start_time.elapsed().as_millis() as u64;

        for ((objective_id, _), solution) in objective_sets.iter().zip(solutions.iter()) {
            let artifact = self.compile_artifact(
                request.clone(),
                target_instance_id.clone(),
                resolved_instances.clone(),
                solution.clone(),
                &id_mappings,
                total_time,
            )?;
            results.push((objective_id.clone(), artifact));
        }

        Ok(results)
    }

    // Remove fetch_commit_data method as we now have commit data directly

    /// Step 2: Resolve all relationships to concrete instance IDs
    fn resolve_all_relationships(
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
    fn setup_pldag_model(
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
        let mut composite_ids = HashSet::new();
        for instance in sorted_instances {
            let class_def = schema
                .get_class_by_id(&instance.class_id)
                .ok_or_else(|| anyhow::anyhow!("Class {} not found", instance.class_id))?;

            // Step 4.3: Determine if instance is primitive or composite
            if instance.relationships.is_empty() || class_def.relationships.is_empty() {
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
                composite_ids.insert(composite_id.clone());
                id_mappings.register_composite(&instance.id, &composite_id);
            }
        }

        Ok((model, id_mappings))
    }

    /// Get all dependencies of an instance (including transitive dependencies)
    fn get_instance_dependencies(
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
                if target_ids.is_empty() {
                    continue;
                }

                // Map target IDs to Pldag variables
                let pldag_vars: Vec<&str> = target_ids
                    .iter()
                    .filter_map(|id| id_mappings.get_pldag_id(id))
                    .collect();

                if pldag_vars.is_empty() {
                    continue;
                }

                // Create constraint based on quantifier
                let constraint_id = match &rel_def.quantifier {
                    Quantifier::AtLeast(n) => model.set_atleast(pldag_vars, *n as i64),
                    Quantifier::AtMost(n) => model.set_atmost(pldag_vars, *n as i64),
                    Quantifier::Exactly(n) => model.set_equal(pldag_vars, *n as i64),
                    Quantifier::Range(min, max) => {
                        let min_id = model.set_atleast(pldag_vars.clone(), *min as i64);
                        let max_id = model.set_atmost(pldag_vars, *max as i64);
                        model.set_and(vec![min_id, max_id])
                    }
                    Quantifier::Optional => model.set_atleast(pldag_vars, 0),
                    Quantifier::Any => model.set_or(pldag_vars),
                    Quantifier::All => {
                        model.set_and(pldag_vars.into_iter().map(|s| s.to_string()).collect())
                    }
                };

                constraint_ids.push(constraint_id);
            }
        }

        // Combine all relationship constraints
        if constraint_ids.is_empty() {
            // No valid relationships, create a constant true node
            Ok(model.set_and::<String>(vec![]))
        } else if constraint_ids.len() == 1 {
            Ok(constraint_ids[0].clone())
        } else {
            match class_def.base.op {
                class::BaseOp::All => Ok(model.set_and(constraint_ids)),
                class::BaseOp::Any => Ok(model.set_or(constraint_ids)),
                class::BaseOp::AtLeast => Ok(model.set_atleast(
                    constraint_ids.iter().map(|id| id.as_str()).collect(),
                    class_def.base.val.unwrap_or(0) as i64,
                )),
                class::BaseOp::AtMost => Ok(model.set_atmost(
                    constraint_ids.iter().map(|id| id.as_str()).collect(),
                    class_def.base.val.unwrap_or(0) as i64,
                )),
                class::BaseOp::Exactly => Ok(model.set_equal(
                    constraint_ids.iter().map(|id| id.as_str()).collect(),
                    class_def.base.val.unwrap_or(0) as i64,
                )),
                class::BaseOp::Imply => {
                    if constraint_ids.len() != 2 {
                        Err(anyhow::anyhow!(
                            "Imply operator requires exactly 2 constraints"
                        ))
                    } else {
                        Ok(model.set_imply(&constraint_ids[0], &constraint_ids[1]))
                    }
                }
                class::BaseOp::Equiv => {
                    if constraint_ids.len() != 2 {
                        Err(anyhow::anyhow!(
                            "Equiv operator requires exactly 2 constraints"
                        ))
                    } else {
                        Ok(model.set_equiv(&constraint_ids[0], &constraint_ids[1]))
                    }
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
    ) -> Result<HashMap<String, i64>> {
        // Create root constraint (all instances must be valid)
        let all_vars: Vec<&str> = pldag_to_our.keys().map(|s| s.as_str()).collect();
        let root = if all_vars.is_empty() {
            model.set_and::<String>(vec![])
        } else {
            model.set_and(all_vars.into_iter().map(|s| s.to_string()).collect())
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
            Ok(HashMap::new())
        }
    }

    /// Solve with Pldag for multiple objective sets efficiently
    fn solve_with_pldag_batch(
        &self,
        model: Pldag,
        root: &str,
        objective_sets: &[(String, HashMap<String, f64>)],
        id_mappings: &IdMappings,
        pldag_to_our: &HashMap<String, String>,
    ) -> Result<Vec<HashMap<String, i64>>> {
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
        for solution_opt in solutions {
            if let Some(solution) = solution_opt {
                let mut our_solution = HashMap::new();
                for (pldag_id, (value, _)) in solution {
                    if let Some(our_id) = pldag_to_our.get(&pldag_id) {
                        our_solution.insert(our_id.clone(), value);
                    }
                }
                our_solutions.push(our_solution);
            } else {
                our_solutions.push(HashMap::new());
            }
        }

        // Ensure we have a solution for each objective set
        while our_solutions.len() < objective_sets.len() {
            our_solutions.push(HashMap::new());
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
    ) -> Result<ConfigurationArtifact> {
        // Update instance domains based on solution
        for instance in instances.iter_mut() {
            if let Some(&value) = solution.get(&instance.id) {
                instance.domain = Some(Domain::constant(value as i32));
            } else if instance.domain.is_none() {
                // Default domain for instances not in solution
                instance.domain = Some(Domain::binary());
            }
        }

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

        artifact.set_configuration(instances);

        // Set metadata
        artifact.solve_metadata = SolveMetadata {
            total_time_ms,
            pipeline_phases: vec![
                PipelinePhase {
                    name: "fetch_data".to_string(),
                    duration_ms: 0,
                    details: None,
                },
                PipelinePhase {
                    name: "resolve_relationships".to_string(),
                    duration_ms: 0,
                    details: None,
                },
                PipelinePhase {
                    name: "setup_pldag".to_string(),
                    duration_ms: 0,
                    details: None,
                },
                PipelinePhase {
                    name: "solve".to_string(),
                    duration_ms: 0,
                    details: None,
                },
            ],
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
struct IdMappings {
    /// Our instance ID -> Pldag variable ID
    our_to_pldag: HashMap<String, String>,
    /// Pldag variable ID -> Our instance ID
    pldag_to_our: HashMap<String, String>,
    /// Set of primitive instance IDs (directly used in Pldag)
    primitives: HashSet<String>,
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

    fn get_pldag_id(&self, our_id: &str) -> Option<&str> {
        self.our_to_pldag.get(our_id).map(|s| s.as_str())
    }

    fn count(&self) -> usize {
        self.our_to_pldag.len()
    }
}

/// Legacy solve pipeline that takes a Store - for backward compatibility
/// This is used by API handlers until they are refactored to use commit-based approach
pub struct SolvePipelineWithStore<
    'a,
    S: crate::store::traits::Store + crate::store::traits::CommitStore,
> {
    store: &'a S,
}

impl<'a, S: crate::store::traits::Store + crate::store::traits::CommitStore>
    SolvePipelineWithStore<'a, S>
{
    /// Create a new solve pipeline with store
    pub fn new(store: &'a S) -> Self {
        Self { store }
    }

    /// Execute the complete solve pipeline (deprecated - use solve_instance instead)
    /// This method is deprecated in favor of the ILP-based solve_instance approach
    pub async fn solve(&self, _request: NewConfigurationArtifact) -> Result<ConfigurationArtifact> {
        // For now, return an error indicating this method is deprecated
        Err(anyhow::anyhow!(
            "The solve method is deprecated. Use solve_instance with a specific target instance ID instead."
        ))
    }

    /// Execute the solve pipeline for a specific instance (ILP-based approach)
    pub async fn solve_instance(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
    ) -> Result<ConfigurationArtifact> {
        // Fetch the commit based on resolution context
        let commit = if let Some(commit_hash) = &request.resolution_context.commit_hash {
            // Fetch specific commit
            self.store
                .get_commit(commit_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Commit {} not found", commit_hash))?
        } else {
            // Fetch current commit from branch
            let branch = self
                .store
                .get_branch(
                    &request.resolution_context.database_id,
                    &request.resolution_context.branch_id,
                )
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("Branch {} not found", request.resolution_context.branch_id)
                })?;

            if branch.current_commit_hash.is_empty() {
                return Err(anyhow::anyhow!(
                    "Branch {} has no current commit",
                    request.resolution_context.branch_id
                ));
            }

            self.store
                .get_commit(&branch.current_commit_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Commit {} not found", branch.current_commit_hash))?
        };

        // Get commit data and create pipeline
        let commit_data = commit
            .get_data()
            .map_err(|e| anyhow::anyhow!("Failed to read commit data: {}", e))?;
        let pipeline = SolvePipeline::new(&commit_data);
        pipeline.solve_instance(request, target_instance_id)
    }

    /// Execute the solve pipeline with objectives for combinatorial search
    pub async fn solve_instance_with_objectives(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objectives: HashMap<String, f64>,
    ) -> Result<ConfigurationArtifact> {
        // Fetch the commit based on resolution context
        let commit = if let Some(commit_hash) = &request.resolution_context.commit_hash {
            // Fetch specific commit
            self.store
                .get_commit(commit_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Commit {} not found", commit_hash))?
        } else {
            // Fetch current commit from branch
            let branch = self
                .store
                .get_branch(
                    &request.resolution_context.database_id,
                    &request.resolution_context.branch_id,
                )
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("Branch {} not found", request.resolution_context.branch_id)
                })?;

            if branch.current_commit_hash.is_empty() {
                return Err(anyhow::anyhow!(
                    "Branch {} has no current commit",
                    request.resolution_context.branch_id
                ));
            }

            self.store
                .get_commit(&branch.current_commit_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Commit {} not found", branch.current_commit_hash))?
        };

        // Get commit data and create pipeline
        let commit_data = commit
            .get_data()
            .map_err(|e| anyhow::anyhow!("Failed to read commit data: {}", e))?;
        let pipeline = SolvePipeline::new(&commit_data);
        pipeline.solve_instance_with_objectives(request, target_instance_id, objectives)
    }

    /// Execute the solve pipeline with multiple objective sets for efficient batch solving
    pub async fn solve_instance_with_multiple_objectives(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objective_sets: Vec<(String, HashMap<String, f64>)>,
    ) -> Result<Vec<(String, ConfigurationArtifact)>> {
        // Fetch the commit based on resolution context
        let commit = if let Some(commit_hash) = &request.resolution_context.commit_hash {
            // Fetch specific commit
            self.store
                .get_commit(commit_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Commit {} not found", commit_hash))?
        } else {
            // Fetch current commit from branch
            let branch = self
                .store
                .get_branch(
                    &request.resolution_context.database_id,
                    &request.resolution_context.branch_id,
                )
                .await?
                .ok_or_else(|| {
                    anyhow::anyhow!("Branch {} not found", request.resolution_context.branch_id)
                })?;

            if branch.current_commit_hash.is_empty() {
                return Err(anyhow::anyhow!(
                    "Branch {} has no current commit",
                    request.resolution_context.branch_id
                ));
            }

            self.store
                .get_commit(&branch.current_commit_hash)
                .await?
                .ok_or_else(|| anyhow::anyhow!("Commit {} not found", branch.current_commit_hash))?
        };

        // Get commit data and create pipeline
        let commit_data = commit
            .get_data()
            .map_err(|e| anyhow::anyhow!("Failed to read commit data: {}", e))?;
        let pipeline = SolvePipeline::new(&commit_data);
        pipeline.solve_instance_with_multiple_objectives(
            request,
            target_instance_id,
            objective_sets,
        )
    }
}
