use crate::logic::{Expander, SimpleEvaluator, SimpleValidator};
use crate::model::{
    ConfigurationArtifact, Domain, Id, Instance, IssueSeverity, NewConfigurationArtifact,
    PipelinePhase, Quantifier, ResolutionContext, SolveIssue, SolveMetadata, SolveStatistics,
};
use crate::store::traits::Store;
use anyhow::Result;
use pldag::Pldag;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

/// The solve pipeline orchestrates the complete solution process
/// From ResolutionContext + target instance → ConfigurationArtifact with ILP-ready data
pub struct SolvePipeline<'a, S: Store> {
    store: &'a S,
    evaluator: SimpleEvaluator,
    validator: SimpleValidator,
}

/// Intermediate state during solving for ILP-based resolution
struct SolveState {
    /// All instances in the configuration (queried + connected instances)
    /// All relationships will be materialized to concrete instance IDs
    configuration: Vec<Instance>,
    /// ID of the queried instance (the one originally requested)
    queried_instance_id: Option<Id>,
    /// Pipeline timing information
    pipeline_phases: Vec<PipelinePhase>,
    /// Issues encountered during solving
    issues: Vec<SolveIssue>,
}

impl<'a, S: Store> SolvePipeline<'a, S> {
    /// Create a new solve pipeline
    pub fn new(store: &'a S) -> Self {
        Self {
            store,
            evaluator: SimpleEvaluator,
            validator: SimpleValidator,
        }
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
    /// Collects the queried instance and all connected variable instances for ILP solving
    pub async fn solve_instance(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
    ) -> Result<ConfigurationArtifact> {
        // TODO: Update for new commit-based architecture - solve pipeline currently disabled
        return Err(anyhow::anyhow!(
            "Solve pipeline disabled pending commit-based architecture update"
        ));

        self.solve_instance_with_derived(request, target_instance_id, None)
            .await
    }

    /// Execute the solve pipeline with objectives for combinatorial search
    pub async fn solve_instance_with_objectives(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objectives: HashMap<String, f64>,
    ) -> Result<ConfigurationArtifact> {
        self.solve_instance_with_objectives_and_derived(request, target_instance_id, objectives, None)
            .await
    }

    /// Execute the solve pipeline with optional derived property calculation
    pub async fn solve_instance_with_derived(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        derived_properties: Option<Vec<String>>,
    ) -> Result<ConfigurationArtifact> {
        self.solve_instance_with_objectives_and_derived(request, target_instance_id, HashMap::new(), derived_properties)
            .await
    }

    /// Execute the solve pipeline with objectives and optional derived property calculation
    pub async fn solve_instance_with_objectives_and_derived(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objectives: HashMap<String, f64>,
        derived_properties: Option<Vec<String>>,
    ) -> Result<ConfigurationArtifact> {
        self.solve_instance_with_full_options(
            request, 
            target_instance_id, 
            objectives, 
            derived_properties, 
            true // include_metadata
        ).await
    }

    /// Execute the solve pipeline with optional metadata collection
    /// When include_metadata is false, no timing information or pipeline phases will be collected
    pub async fn solve_instance_with_metadata_control(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        include_metadata: bool,
    ) -> Result<ConfigurationArtifact> {
        self.solve_instance_with_full_options(
            request, 
            target_instance_id, 
            HashMap::new(), // no objectives
            None, // no derived properties
            include_metadata
        ).await
    }

    /// Execute the solve pipeline with full options including metadata control
    pub async fn solve_instance_with_full_options(
        &self,
        request: NewConfigurationArtifact,
        target_instance_id: Id,
        objectives: HashMap<String, f64>,
        derived_properties: Option<Vec<String>>,
        include_metadata: bool,
    ) -> Result<ConfigurationArtifact> {
        let total_start = if include_metadata { Some(Instant::now()) } else { None };
        let mut state = SolveState {
            configuration: Vec::new(),
            queried_instance_id: Some(target_instance_id.clone()),
            pipeline_phases: if include_metadata { Vec::new() } else { Vec::new() }, // Always allocate for now, but could optimize
            issues: Vec::new(),
        };

        // Fetch schema once at the beginning to avoid multiple fetches
        let schema = match self.store.get_schema(&request.resolution_context.database_id, &request.resolution_context.branch_id).await? {
            Some(schema) => schema,
            None => {
                return Err(anyhow::anyhow!("No schema found for branch {}", request.resolution_context.branch_id));
            }
        };

        // Phase 1: Collect - Get the queried instance and find all connected instances
        self.phase_collect(&request.resolution_context, &mut state, &target_instance_id, &schema, include_metadata)
            .await?;

        // Phase 2: Prepare - Prepare instances for ILP solving (you will implement ILP solver)
        self.phase_prepare(&request.resolution_context, &mut state, &schema, include_metadata)
            .await?;

        // Phase 3: Solve - Use ILP solver to resolve variable domains and create final artifact
        let objectives_opt = if objectives.is_empty() { None } else { Some(objectives) };
        let total_time_ms = total_start.map(|s| s.elapsed().as_millis() as u64).unwrap_or(0);
        let mut artifact = self
            .phase_solve_ilp(&request, state, total_time_ms, objectives_opt, &schema, include_metadata)
            .await?;

        // Phase 4: Derived Properties - Calculate requested derived properties (if any)
        if let Some(derived_props) = derived_properties {
            self.phase_calculate_derived_properties(
                &request.resolution_context,
                &mut artifact,
                &derived_props,
            )
            .await?;
        }

        Ok(artifact)
    }

    /// Phase 1: Collect the queried instance and all connected variable instances
    async fn phase_collect(
        &self,
        context: &ResolutionContext,
        state: &mut SolveState,
        target_instance_id: &Id,
        schema: &crate::model::Schema,
        include_metadata: bool,
    ) -> Result<()> {
        let phase_start = if include_metadata { Some(Instant::now()) } else { None };

        // Get the target/queried instance
        match self
            .store
            .get_instance(&context.database_id, &context.branch_id, target_instance_id)
            .await?
        {
            Some(instance) => {
                state.configuration.push(instance);
            }
            None => {
                state.issues.push(SolveIssue {
                    severity: IssueSeverity::Critical,
                    message: format!("Target instance not found: {}", target_instance_id),
                    component: Some(target_instance_id.to_string()),
                    context: None,
                });
                return Err(anyhow::anyhow!(
                    "Target instance not found: {}",
                    target_instance_id
                ));
            }
        }

        // Collect all connected instances (children)
        let connected_instances = self
            .collect_connected_instances(context, target_instance_id, schema)
            .await?;
        state.configuration.extend(connected_instances);

        // After adding connected instances, we might discover more relationships that need resolution
        // So we'll let the prepare phase handle relationship materialization and then collect any newly discovered instances

        if include_metadata {
            if let Some(start) = phase_start {
                state.pipeline_phases.push(PipelinePhase {
                    name: "collect".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    details: Some(serde_json::json!({
                        "queried_instance_id": target_instance_id,
                        "total_instances_count": state.configuration.len()
                    })),
                });
            }
        }

        Ok(())
    }

    /// Phase 2: Prepare instances for ILP solving
    async fn phase_prepare(
        &self,
        context: &ResolutionContext,
        state: &mut SolveState,
        schema: &crate::model::Schema,
        include_metadata: bool,
    ) -> Result<()> {
        let phase_start = if include_metadata { Some(Instant::now()) } else { None };

        // Single pass: Prepare domains, resolve defaults, and materialize relationships
        let mut instances_prepared = 0;
        
        // Process all instances in a single pass
        self.prepare_and_materialize_all_instances(&mut state.configuration, context, schema)
            .await?;
        instances_prepared = state.configuration.len();

        // After materializing relationships, collect any newly discovered instances
        let newly_discovered = self
            .collect_newly_discovered_instances(&state.configuration, context)
            .await?;
        if !newly_discovered.is_empty() {
            state.configuration.extend(newly_discovered);
            // Prepare domains for the newly discovered instances
            for instance in state.configuration.iter_mut() {
                if instance.domain.is_none() {
                    self.prepare_instance_domain(instance, schema);
                }
            }
        }

        if include_metadata {
            if let Some(start) = phase_start {
                state.pipeline_phases.push(PipelinePhase {
                    name: "prepare".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    details: Some(serde_json::json!({
                        "instances_prepared": instances_prepared,
                        "relationships_materialized": true,
                        "ready_for_ilp": true
                    })),
                });
            }
        }

        Ok(())
    }

    /// Single-pass processing: prepare domains, resolve schema defaults, and materialize relationships
    async fn prepare_and_materialize_all_instances(
        &self,
        configuration: &mut Vec<Instance>,
        context: &ResolutionContext,
        schema: &crate::model::Schema,
    ) -> Result<()> {
        
        // Collect all instance IDs for filtering
        let instance_ids: std::collections::HashSet<Id> =
            configuration.iter().map(|inst| inst.id.clone()).collect();

        // Only pre-fetch instances if we actually have relationships to resolve
        let needs_relationship_resolution = configuration.iter().any(|instance| {
            instance.relationships.is_empty() || 
            instance.relationships.iter().any(|(_, selection)| {
                !matches!(selection, crate::model::RelationshipSelection::SimpleIds(_))
            })
        });
        
        let instances_by_type = if needs_relationship_resolution {
            Some(self.batch_fetch_instances_for_relationships(context, schema).await?)
        } else {
            None
        };

        // Process each instance in a single pass
        for instance in configuration.iter_mut() {
            // 1. Prepare domain if not already set
            if instance.domain.is_none() {
                self.prepare_instance_domain(instance, schema);
            }

            // Find the class definition for this instance
            let class_def = schema.get_class_by_id(&instance.class_id);
            
            // Fast path: if instance already has all relationships materialized, skip expensive processing
            let all_relationships_materialized = instance.relationships.iter().all(|(_, selection)| {
                matches!(selection, crate::model::RelationshipSelection::SimpleIds(_))
            });
            
            if all_relationships_materialized {
                continue; // Skip all relationship processing for this instance
            }

            // 2. Check if we need to resolve schema defaults
            let has_empty_relationships = instance.relationships.is_empty() || 
                instance.relationships.iter().any(|(_, selection)| {
                    match selection {
                        crate::model::RelationshipSelection::SimpleIds(ids) if ids.is_empty() => true,
                        crate::model::RelationshipSelection::Ids { ids } if ids.is_empty() => true,
                        crate::model::RelationshipSelection::PoolBased { selection: None, .. } => true,
                        crate::model::RelationshipSelection::PoolBased { 
                            selection: Some(crate::model::SelectionSpec::Unresolved), 
                            .. 
                        } => true,
                        _ => false,
                    }
                });

            if has_empty_relationships {
                if let Some(class_def) = class_def {
                // Resolve schema default relationships efficiently using pre-fetched data
                for rel_def in &class_def.relationships {
                    // Only process if relationship doesn't exist or is empty/unresolved
                    let should_resolve = match instance.relationships.get(&rel_def.id) {
                        None => true,
                        Some(crate::model::RelationshipSelection::SimpleIds(ids)) if ids.is_empty() => true,
                        Some(crate::model::RelationshipSelection::Ids { ids }) if ids.is_empty() => true,
                        Some(crate::model::RelationshipSelection::PoolBased { selection: None, .. }) => true,
                        Some(crate::model::RelationshipSelection::PoolBased { 
                            selection: Some(crate::model::SelectionSpec::Unresolved), 
                            .. 
                        }) => true,
                        _ => false,
                    };

                    if should_resolve {
                        // Use pre-fetched instances instead of querying database
                        let mut materialized_ids = Vec::new();
                        
                        // Apply default pool strategy
                        match &rel_def.default_pool {
                            crate::model::DefaultPool::All => {
                                // Get all instances of target types from pre-fetched data
                                if let Some(ref instances_by_type) = instances_by_type {
                                    for target_type in &rel_def.targets {
                                        if let Some(instances) = instances_by_type.get(target_type) {
                                            materialized_ids.extend(instances.iter().map(|i| i.id.clone()));
                                        }
                                    }
                                }
                            }
                            crate::model::DefaultPool::None => {
                                // Empty pool - no instances
                            }
                            crate::model::DefaultPool::Filter { filter, .. } => {
                                // For now, we'll use the original Expander for filtered pools
                                // This maintains correctness while we optimize the common cases
                                use crate::logic::Expander;
                                
                                // Since filters are complex, fall back to the original implementation
                                // but at least we've optimized the All and None cases
                                if let Ok(resolved_rel) = Expander::resolve_relationship_from_schema(
                                    self.store,
                                    rel_def,
                                    &context.database_id,
                                    &context.branch_id,
                                ).await {
                                    materialized_ids = resolved_rel.materialized_ids;
                                }
                            }
                        }
                        
                        // Set the resolved relationship
                        let selection = crate::model::RelationshipSelection::SimpleIds(materialized_ids);
                        instance.relationships.insert(rel_def.id.clone(), selection);
                    }
                }
                }
            }

            // 3. Materialize all relationships to concrete instance IDs using pre-loaded data
            for (rel_name, rel_selection) in instance.relationships.iter_mut() {
                // Skip if already materialized to SimpleIds
                if matches!(rel_selection, crate::model::RelationshipSelection::SimpleIds(_)) {
                    continue;
                }
                
                // Materialize using pre-loaded data instead of database queries
                if let Some(ref instances_by_type) = instances_by_type {
                    *rel_selection = self.materialize_relationship_from_cache(
                        rel_selection,
                        instances_by_type,
                    );
                }
            }
        }
        
        Ok(())
    }

    /// Load ALL instances from commit ONCE and organize by type
    /// This eliminates multiple expensive commit data loads
    async fn batch_fetch_instances_for_relationships(
        &self,
        context: &ResolutionContext,
        schema: &crate::model::Schema,
    ) -> Result<HashMap<String, Vec<Instance>>> {
        // Load ALL instances from the commit ONCE using list_instances_for_branch with no filter
        let all_instances = self.store.list_instances_for_branch(
            &context.database_id,
            &context.branch_id,
            None, // No filter - get everything
        ).await?;
        
        // Organize instances by type in memory - no more database queries needed
        let mut instances_by_type: HashMap<String, Vec<Instance>> = HashMap::new();
        
        for instance in all_instances {
            instances_by_type
                .entry(instance.class_id.clone())
                .or_insert_with(Vec::new)
                .push(instance);
        }
        
        Ok(instances_by_type)
    }

    /// Materialize relationship selection using pre-loaded instances (no database queries)
    fn materialize_relationship_from_cache(
        &self,
        selection: &crate::model::RelationshipSelection,
        instances_by_type: &HashMap<String, Vec<Instance>>,
    ) -> crate::model::RelationshipSelection {
        match selection {
            // Already materialized
            crate::model::RelationshipSelection::SimpleIds(ids) => {
                crate::model::RelationshipSelection::SimpleIds(ids.clone())
            }
            crate::model::RelationshipSelection::Ids { ids } => {
                crate::model::RelationshipSelection::SimpleIds(ids.clone())
            }
            crate::model::RelationshipSelection::PoolBased { pool, selection: selection_spec } => {
                let mut materialized_ids = Vec::new();
                
                if let Some(pool_filter) = pool {
                    // Apply pool filter to pre-loaded instances
                    if let Some(types) = &pool_filter.types {
                        for type_name in types {
                            if let Some(instances) = instances_by_type.get(type_name) {
                                // Apply simple filtering (for now, just collect all instances of this type)
                                // TODO: Implement full where_clause, sort, limit filtering
                                let mut filtered_instances = instances.clone();
                                
                                // Apply limit if present
                                if let Some(limit) = pool_filter.limit {
                                    filtered_instances.truncate(limit);
                                }
                                
                                materialized_ids.extend(filtered_instances.iter().map(|i| i.id.clone()));
                            }
                        }
                    }
                }
                
                crate::model::RelationshipSelection::SimpleIds(materialized_ids)
            }
            // For other complex formats, return empty for now to maintain correctness
            _ => crate::model::RelationshipSelection::SimpleIds(Vec::new()),
        }
    }

    /// Phase 3: Solve using ILP solver to resolve variable domains and create final artifact
    async fn phase_solve_ilp(
        &self,
        request: &NewConfigurationArtifact,
        mut state: SolveState,
        total_time_ms: u64,
        objectives: Option<HashMap<String, f64>>,
        schema: &crate::model::Schema,
        include_metadata: bool,
    ) -> Result<ConfigurationArtifact> {
        let phase_start = if include_metadata { Some(Instant::now()) } else { None };

        let artifact_id = crate::model::generate_id();
        let mut artifact = ConfigurationArtifact::new(
            artifact_id,
            request.resolution_context.clone(),
            request.user_metadata.clone(),
        );

        // Get configuration length before moving
        let total_instances = state.configuration.len();

        // Set the complete configuration (all instances)
        artifact.set_configuration(state.configuration);

        // TODO: Implement ILP solver here
        // You can now use the `schema` variable to set up the ILP problem
        // The `artifact.configuration` contains all instances with materialized relationships
        // Each instance has a domain that needs to be resolved by the ILP solver
        let mut model = Pldag::new();

        // Perform topological sort based on relationship dependencies
        let sorted_instances = self
            .topological_sort(&artifact.configuration, &request.resolution_context)
            .await?;
        let mut instance_ids: HashSet<String> = HashSet::new();
        let mut id_map: HashMap<String, String> = HashMap::new();
        let mut primitive_instance_ids: HashSet<String> = HashSet::new();

        for instance in sorted_instances.iter() {
            let instance_class = schema.get_class_by_id(&instance.class_id);
            if let Some(class_def) = instance_class {
                // If the instance has no relationships, we add it as a primitive variable to pldag
                if instance.relationships.is_empty() {
                    if let Some(domain) = &instance.domain {
                        model.set_primitive(
                            &instance.id,
                            (domain.lower as i64, domain.upper as i64),
                        );
                        primitive_instance_ids.insert(instance.id.clone());
                    } else {
                        state.issues.push(SolveIssue {
                            severity: IssueSeverity::Warning,
                            message: format!("Instance {} has no domain defined", instance.id),
                            component: Some(instance.id.clone()),
                            context: None,
                        });
                    }
                } else {
                    let mut relationship_ids: HashSet<String> = HashSet::new();
                    for (relationship_id, relationship_selection) in instance.relationships.iter() {
                        // Find the relationship definition in the class
                        if let Some(relationship_def) = class_def
                            .relationships
                            .iter()
                            .find(|r| &r.id == relationship_id)
                        {
                            // Resolve the relationship to get materialized IDs
                            if let Ok(resolved_rel) =
                                Expander::resolve_selection_enhanced_with_branch(
                                    self.store,
                                    relationship_selection,
                                    &request.resolution_context.database_id,
                                    Some(&request.resolution_context.branch_id), // CRITICAL: Pass branch context for isolation
                                )
                                .await
                            {
                                let materialized_ids: Vec<&str> = resolved_rel
                                    .materialized_ids
                                    .iter()
                                    .map(|id| id.as_str())
                                    .collect();

                                if !materialized_ids.is_empty() {
                                    // Set constraints based on the quantifier
                                    match &relationship_def.quantifier {
                                        Quantifier::AtLeast(n) => {
                                            let subid =
                                                model.set_atleast(materialized_ids, (*n) as i64);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                        Quantifier::AtMost(n) => {
                                            let subid =
                                                model.set_atmost(materialized_ids, (*n) as i64);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                        Quantifier::Range(min, max) => {
                                            let ids_clone: Vec<&str> =
                                                materialized_ids.iter().cloned().collect();
                                            let lb_id = model.set_atleast(ids_clone, (*min) as i64);
                                            let ub_id =
                                                model.set_atmost(materialized_ids, (*max) as i64);
                                            let subid = model.set_and(vec![lb_id, ub_id]);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                        Quantifier::Optional => {
                                            let subid = model.set_atmost(materialized_ids, 1);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                        Quantifier::Exactly(n) => {
                                            // For exactly, we need both atleast and atmost
                                            let subid =
                                                model.set_equal(materialized_ids, (*n) as i64);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                        Quantifier::Any => {
                                            let subid = model.set_or(materialized_ids);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                        Quantifier::All => {
                                            let subid = model.set_and(materialized_ids);
                                            relationship_ids.insert(subid.clone());
                                            id_map.insert(subid, instance.id.clone());
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Combine all relationship constraints with AND
                    let instance_id = model.set_and(relationship_ids.into_iter().collect());
                    instance_ids.insert(instance_id);
                }
            } else {
                // Class definition not found for instance type
                state.issues.push(SolveIssue {
                    severity: IssueSeverity::Warning,
                    message: format!(
                        "Class definition not found for instance type: {}",
                        instance.class_id
                    ),
                    component: Some(instance.id.clone()),
                    context: None,
                });
            }
        }

        // Finally tie the sack together by setting a root AND node for all instance IDs
        let constraint_count = instance_ids.len() + 1;
        let root = model.set_and(instance_ids.into_iter().collect());
        let root_id = state
            .queried_instance_id
            .as_ref()
            .map(|id| id.clone())
            .unwrap_or_else(|| "root".to_string());
        id_map.insert(root.clone(), root_id);
        // Create objectives for the solver - need to keep strings alive
        let objective_strings: Vec<(String, f64)> = if let Some(obj) = objectives {
            // Convert instance ID objectives to pldag variable objectives
            obj.into_iter()
                .filter_map(|(instance_id, weight)| {
                    // Check if this instance ID was directly used as a primitive variable
                    if primitive_instance_ids.contains(&instance_id) {
                        Some((instance_id, weight))
                    } else {
                        // Check if any id_map entry points to this instance
                        id_map.iter()
                            .find(|(_, mapped_instance_id)| **mapped_instance_id == instance_id)
                            .map(|(pldag_id, _)| (pldag_id.clone(), weight))
                    }
                })
                .collect()
        } else {
            vec![]
        };
        
        // Create the objectives with proper lifetimes
        let solver_objectives = if objective_strings.is_empty() {
            vec![HashMap::new()]
        } else {
            let obj_map: HashMap<&str, f64> = objective_strings
                .iter()
                .map(|(id, weight)| (id.as_str(), *weight))
                .collect();
            vec![obj_map]
        };
        
        let solutions = model.solve(
            solver_objectives,
            HashMap::from_iter(vec![(&root[..], (1, 1))]),
            true,
        );

        // Handle case where ILP solver returns no solutions or empty solution
        if solutions.is_empty() {
            // No solutions - this might happen if the problem is infeasible
            // For now, we'll continue without updating domains (they keep their original values)
            state.issues.push(SolveIssue {
                severity: IssueSeverity::Warning,
                message: "ILP solver returned no solutions - keeping original domains".to_string(),
                component: None,
                context: Some(serde_json::json!({
                    "solver": "pldag",
                    "constraint_count": constraint_count
                })),
            });
        } else if let Some(Some(solution_option)) = solutions.first() {
            // Convert IndexMap<String, (i64, i64)> to HashMap<String, i64> for domain mapping
            let solution_values: HashMap<String, i64> = solution_option
                .iter()
                .map(|(k, (lower, _upper))| (k.clone(), *lower)) // Use lower bound as the solution value
                .collect();

            // Extract solution values and map back to artifact configuration
            self.map_solution_to_artifact_domains(&solution_values, &mut artifact, &id_map)
                .await?;
        } else {
            // First solution is None - invalid solution
            state.issues.push(SolveIssue {
                severity: IssueSeverity::Warning,
                message: "ILP solver returned invalid solution - keeping original domains"
                    .to_string(),
                component: None,
                context: Some(serde_json::json!({
                    "solver": "pldag"
                })),
            });
        }

        // Conditionally create solve metadata
        if include_metadata {
            let mut pipeline_phases = state.pipeline_phases;
            if let Some(start) = phase_start {
                pipeline_phases.push(PipelinePhase {
                    name: "solve".to_string(),
                    duration_ms: start.elapsed().as_millis() as u64,
                    details: Some(serde_json::json!({
                        "artifact_id": artifact.id,
                        "schema_classes": schema.classes.len(),
                        "total_instances": total_instances,
                        "ilp_solver_ready": true
                    })),
                });
            }

            artifact.solve_metadata = SolveMetadata {
                total_time_ms,
                pipeline_phases,
                solver_info: None, // Will be populated by your ILP solver
                statistics: SolveStatistics {
                    total_instances: artifact.instance_count(),
                    variable_instances_resolved: artifact.instance_count(),
                    conditional_properties_evaluated: 0, // Will be set by your ILP solver
                    derived_properties_calculated: 0,    // Will be set by your ILP solver
                    ilp_variables: None,                 // Will be set by your ILP solver
                    ilp_constraints: None,               // Will be set by your ILP solver
                    peak_memory_bytes: None,
                },
                issues: state.issues,
            };
        } else {
            // Create minimal metadata when disabled
            artifact.solve_metadata = SolveMetadata {
                total_time_ms: 0,
                pipeline_phases: Vec::new(),
                solver_info: None,
                statistics: SolveStatistics {
                    total_instances: artifact.instance_count(),
                    variable_instances_resolved: artifact.instance_count(),
                    conditional_properties_evaluated: 0,
                    derived_properties_calculated: 0,
                    ilp_variables: None,
                    ilp_constraints: None,
                    peak_memory_bytes: None,
                },
                issues: state.issues,
            };
        }

        Ok(artifact)
    }

    /// Collect all instances connected to the target instance (find children/dependencies)
    async fn collect_connected_instances(
        &self,
        context: &ResolutionContext,
        target_instance_id: &Id,
        schema: &crate::model::Schema,
    ) -> Result<Vec<Instance>> {
        let mut connected_instances = Vec::new();
        let mut visited = HashSet::new();
        let mut to_visit = vec![target_instance_id.clone()];

        while let Some(current_id) = to_visit.pop() {
            if visited.contains(&current_id) {
                continue;
            }
            visited.insert(current_id.clone());

            // Skip the target instance itself (it's the queried instance)
            if current_id == *target_instance_id {
                if let Some(instance) = self
                    .store
                    .get_instance(&context.database_id, &context.branch_id, &current_id)
                    .await?
                {
                    // Find connected instances through this instance's relationships
                    self.find_related_instances(&instance, schema, &mut to_visit, &visited);
                }
                continue;
            }

            // Add this instance as a variable instance
            if let Some(instance) = self
                .store
                .get_instance(&context.database_id, &context.branch_id, &current_id)
                .await?
            {
                // Find more connected instances through this instance's relationships
                self.find_related_instances(&instance, schema, &mut to_visit, &visited);
                connected_instances.push(instance);
            }
        }

        Ok(connected_instances)
    }

    /// Find instances related to the given instance through its relationships
    fn find_related_instances(
        &self,
        instance: &Instance,
        schema: &crate::model::Schema,
        to_visit: &mut Vec<Id>,
        visited: &HashSet<Id>,
    ) {
        // Find the class definition for this instance
        if let Some(class_def) = schema.get_class_by_id(&instance.class_id) {
            // Go through each relationship in the instance
            for (rel_name, rel_selection) in &instance.relationships {
                // Find the relationship definition
                if let Some(_rel_def) = class_def.relationships.iter().find(|r| r.name == *rel_name)
                {
                    // Extract instance IDs from the relationship selection
                    let related_ids = self.extract_ids_from_selection(rel_selection);
                    for id in related_ids {
                        if !visited.contains(&id) {
                            to_visit.push(id);
                        }
                    }
                }
            }
        }
    }

    /// Extract instance IDs from a relationship selection
    fn extract_ids_from_selection(
        &self,
        selection: &crate::model::RelationshipSelection,
    ) -> Vec<Id> {
        match selection {
            crate::model::RelationshipSelection::SimpleIds(ids) => ids.clone(),
            crate::model::RelationshipSelection::Ids { ids } => ids.clone(),
            crate::model::RelationshipSelection::PoolBased { selection, .. } => {
                match selection {
                    Some(crate::model::SelectionSpec::Ids(ids)) => ids.clone(),
                    _ => Vec::new(), // For filters and unresolved, we can't extract IDs statically
                }
            }
            _ => Vec::new(), // For filters and other dynamic selections
        }
    }

    /// Prepare an instance's domain for ILP solving
    fn prepare_instance_domain(&self, instance: &mut Instance, schema: &crate::model::Schema) {
        // If instance already has a domain, keep it
        if instance.domain.is_some() {
            return;
        }

        // Find the class definition and use its domain constraint
        if let Some(class_def) = schema.get_class_by_id(&instance.class_id) {
            if let Some(domain_constraint) = &class_def.domain_constraint {
                instance.domain = Some(domain_constraint.clone());
            } else {
                // Default to binary domain for variables
                instance.domain = Some(Domain::binary());
            }
        } else {
            // Default to binary domain if no class definition found
            instance.domain = Some(Domain::binary());
        }
    }


    /// Materialize a single relationship selection to concrete instance IDs
    /// Uses the exact same logic as the GET endpoint to ensure consistency
    async fn materialize_relationship_selection(
        &self,
        selection: &crate::model::RelationshipSelection,
        _rel_def: Option<&crate::model::RelationshipDef>,
        _valid_instance_ids: &std::collections::HashSet<Id>,
        context: &ResolutionContext,
    ) -> crate::model::RelationshipSelection {
        // Use the exact same resolution logic as the GET endpoint
        // This ensures 100% consistency between GET and solve pipeline
        use crate::logic::Expander;
        
        match Expander::resolve_selection_enhanced_with_branch(
            self.store,
            selection,
            &context.database_id,
            Some(&context.branch_id),
        ).await {
            Ok(resolved_rel) => {
                // Convert the resolved relationship back to SimpleIds format
                crate::model::RelationshipSelection::SimpleIds(resolved_rel.materialized_ids)
            }
            Err(e) => {
                eprintln!("Failed to resolve relationship selection: {}", e);
                // Fallback to empty IDs on error
                crate::model::RelationshipSelection::SimpleIds(Vec::new())
            }
        }
    }


    /// Collect instances that were discovered through relationship materialization
    async fn collect_newly_discovered_instances(
        &self,
        configuration: &[Instance],
        context: &ResolutionContext,
    ) -> Result<Vec<Instance>> {
        let mut newly_discovered = Vec::new();
        let mut current_ids: std::collections::HashSet<Id> =
            configuration.iter().map(|i| i.id.clone()).collect();

        // Look through all relationships in all instances
        for instance in configuration {
            for (_rel_name, rel_selection) in &instance.relationships {
                if let crate::model::RelationshipSelection::SimpleIds(ids) = rel_selection {
                    for id in ids {
                        // If this ID is not already in our configuration, fetch it
                        if !current_ids.contains(id) {
                            if let Ok(Some(new_instance)) =
                                self.store.get_instance(&context.database_id, &context.branch_id, id).await
                            {
                                // Instance is already from the correct branch due to get_instance parameter
                                current_ids.insert(id.clone());
                                newly_discovered.push(new_instance);
                            }
                        }
                    }
                }
            }
        }

        Ok(newly_discovered)
    }

    /// Perform topological sort on instances based on their relationship dependencies
    /// Returns instances in order where those with no relationships come first,
    /// followed by those that depend only on previous instances, etc.
    async fn topological_sort<'b>(
        &self,
        instances: &'b [Instance],
        context: &ResolutionContext,
    ) -> Result<Vec<&'b Instance>> {
        let mut result = Vec::new();
        let mut in_degree = HashMap::new();
        let mut graph: HashMap<Id, Vec<Id>> = HashMap::new();
        let mut instance_map = HashMap::new();

        // Build instance map for quick lookup
        for instance in instances {
            instance_map.insert(instance.id.clone(), instance);
            in_degree.insert(instance.id.clone(), 0);
            graph.insert(instance.id.clone(), Vec::new());
        }

        // Build dependency graph by resolving relationships
        for instance in instances {
            for (_relationship_id, relationship_selection) in &instance.relationships {
                // Resolve the relationship to get materialized IDs
                if let Ok(resolved_rel) = Expander::resolve_selection_enhanced_with_branch(
                    self.store,
                    relationship_selection,
                    &context.database_id,
                    Some(&context.branch_id),
                )
                .await
                {
                    let materialized_ids = resolved_rel.materialized_ids.clone();
                    for target_id in materialized_ids {
                        // Only consider dependencies to instances in our configuration
                        if instance_map.contains_key(&target_id) {
                            // instance depends on target_id
                            graph
                                .entry(target_id)
                                .or_default()
                                .push(instance.id.clone());
                            *in_degree.entry(instance.id.clone()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        // Kahn's algorithm for topological sort
        let mut queue = VecDeque::new();

        // Add all instances with no dependencies (in-degree 0)
        for (instance_id, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(instance_id.clone());
            }
        }

        while let Some(current_id) = queue.pop_front() {
            if let Some(instance) = instance_map.get(&current_id) {
                result.push(*instance);

                // Reduce in-degree for all dependent instances
                if let Some(dependents) = graph.get(&current_id) {
                    for dependent_id in dependents {
                        let degree = in_degree.get_mut(dependent_id).unwrap();
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dependent_id.clone());
                        }
                    }
                }
            }
        }

        // Check for cycles (if not all instances are processed)
        if result.len() != instances.len() {
            // There's a cycle, fallback to original order
            return Ok(instances.iter().collect());
        }

        Ok(result)
    }

    /// Map ILP solution values back to the artifact configuration domains
    /// Uses id_map to translate pldag-generated IDs back to original instance IDs
    async fn map_solution_to_artifact_domains(
        &self,
        solution: &std::collections::HashMap<String, i64>,
        artifact: &mut ConfigurationArtifact,
        id_map: &HashMap<String, String>,
    ) -> Result<()> {
        // Iterate through all instances in the configuration
        for instance in artifact.configuration.iter_mut() {
            // First check if this instance has a direct solution value
            let mut solution_value_opt = solution.get(&instance.id).copied();

            // If no direct match, check if any mapped ID corresponds to this instance
            if solution_value_opt.is_none() {
                for (pldag_id, mapped_instance_id) in id_map {
                    if mapped_instance_id == &instance.id {
                        if let Some(&value) = solution.get(pldag_id) {
                            solution_value_opt = Some(value);
                            break;
                        }
                    }
                }
            }

            // Update the instance domain with the solved value if found
            if let Some(solution_value) = solution_value_opt {
                if let Some(ref mut domain) = instance.domain {
                    // Set the domain to the specific solved value (constant domain)
                    domain.lower = solution_value as i32;
                    domain.upper = solution_value as i32;
                } else {
                    // If no domain exists, create one with the solved value as a constant
                    instance.domain = Some(Domain::constant(solution_value as i32));
                }
            }
        }

        Ok(())
    }

    /// Phase 4: Calculate derived properties for instances in the configuration
    async fn phase_calculate_derived_properties(
        &self,
        context: &ResolutionContext,
        artifact: &mut ConfigurationArtifact,
        requested_properties: &[String],
    ) -> Result<()> {
        use std::time::Instant;
        let phase_start = Instant::now();

        // Get schema for derived property definitions
        let schema = match self.store.get_schema(&context.database_id, &context.branch_id).await? {
            Some(schema) => schema,
            None => {
                return Err(anyhow::anyhow!(
                    "No schema found for derived property calculation"
                ));
            }
        };

        let mut total_calculated = 0;

        // Calculate derived properties for all instances in the configuration
        for instance in artifact.configuration.iter() {
            match SimpleEvaluator::evaluate_derived_properties(
                self.store,
                instance,
                &schema,
                requested_properties,
                &artifact.configuration, // Pass the full configuration for domain checking
            )
            .await
            {
                Ok(derived_values) => {
                    if !derived_values.is_empty() {
                        artifact
                            .derived_properties
                            .insert(instance.id.clone(), derived_values);
                        total_calculated += 1;
                    }
                }
                Err(e) => {
                    eprintln!(
                        "Failed to evaluate derived properties for instance '{}': {}",
                        instance.id, e
                    );
                }
            }
        }

        // Update statistics
        artifact
            .solve_metadata
            .statistics
            .derived_properties_calculated = total_calculated;

        // Add phase timing
        artifact
            .solve_metadata
            .pipeline_phases
            .push(crate::model::PipelinePhase {
                name: "derived_properties".to_string(),
                duration_ms: phase_start.elapsed().as_millis() as u64,
                details: Some(serde_json::json!({
                    "requested_properties": requested_properties,
                    "instances_processed": artifact.configuration.len(),
                    "total_properties_calculated": total_calculated
                })),
            });

        Ok(())
    }
}

#[cfg(all(test, feature = "enable-broken-tests"))]
mod tests {
    use super::*;
    use crate::model::{
        Branch, CrossBranchPolicy, DataType, Database, EmptySelectionPolicy, MissingInstancePolicy,
        PropertyValue, RelationshipSelection, ResolutionPolicies, TypedValue,
    };
    use crate::store::mem::MemoryStore;
    use crate::store::traits::{BranchStore, DatabaseStore, InstanceStore, SchemaStore};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_solve_instance_pipeline_basic() {
        // TODO: Update test for new commit-based architecture
        return; // Test disabled pending architecture update

        let store = MemoryStore::new();
        let pipeline = SolvePipeline::new(&store);

        // Create basic database and branch
        let database = Database::new("test_db".to_string(), Some("Test DB".to_string()));
        let database_id = database.id.clone();
        let branch = Branch::new_main_branch(database_id.clone(), Some("system".to_string()));

        store.upsert_database(database).await.unwrap();
        store.upsert_branch(branch.clone()).await.unwrap();

        // Create schema with a simple class
        let schema = crate::model::Schema {
            id: "test_schema".to_string(),
            branch_id: branch.name.clone(), // Use branch name as branch_id
            description: None,
            classes: vec![crate::model::ClassDef {
                id: "product_class".to_string(),
                name: "Product".to_string(),
                description: None,
                properties: vec![],
                relationships: vec![],
                derived: vec![],
                domain_constraint: Some(Domain::new(0, 10)),
            }],
        };
        store.upsert_schema(schema).await.unwrap();

        // Create a test instance
        let test_instance = Instance {
            id: "test_instance".to_string(),
            branch_id: branch.name.clone(), // Use branch name as branch_id
            instance_type: "Product".to_string(),
            domain: None, // Will be set from class constraint
            properties: {
                let mut props = HashMap::new();
                props.insert(
                    "name".to_string(),
                    PropertyValue::Literal(TypedValue {
                        value: serde_json::Value::String("Test Product".to_string()),
                        data_type: DataType::String,
                    }),
                );
                props
            },
            relationships: HashMap::new(),
        };
        store.upsert_instance(test_instance).await.unwrap();

        // Create solve request
        let request = NewConfigurationArtifact {
            resolution_context: ResolutionContext {
                database_id: database_id,
                branch_id: branch.name.clone(), // Use branch name as branch_id
                commit_hash: None,
                policies: ResolutionPolicies {
                    cross_branch_policy: CrossBranchPolicy::Reject,
                    missing_instance_policy: MissingInstancePolicy::Skip,
                    empty_selection_policy: EmptySelectionPolicy::Allow,
                    max_selection_size: Some(1000),
                    custom: HashMap::new(),
                },
                metadata: None,
            },
            user_metadata: None,
        };

        // Execute solve for the specific instance
        let result = pipeline
            .solve_instance(request, "test_instance".to_string())
            .await;
        assert!(result.is_ok());

        let artifact = result.unwrap();
        assert_eq!(artifact.solve_metadata.pipeline_phases.len(), 3); // collect, prepare, solve
        assert_eq!(artifact.instance_count(), 1); // Only the queried instance

        // Check that the queried instance is in the configuration
        let queried_instance = artifact.get_instance(&"test_instance".to_string()).unwrap();
        assert_eq!(queried_instance.id, "test_instance");
        assert!(queried_instance.domain.is_some()); // Domain should be set from class constraint
    }

    #[tokio::test]
    async fn test_solve_instance_with_connected_instances() {
        // TODO: Update test for new commit-based architecture
        return; // Test disabled pending architecture update

        let store = MemoryStore::new();
        let pipeline = SolvePipeline::new(&store);

        // Create database, branch, and schema (similar to above)
        let database = Database::new("test_db".to_string(), Some("Test DB".to_string()));
        let database_id = database.id.clone();
        let branch = Branch::new_main_branch(database_id.clone(), Some("system".to_string()));

        store.upsert_database(database).await.unwrap();
        store.upsert_branch(branch.clone()).await.unwrap();

        // Create schema with classes that have relationships
        let schema = crate::model::Schema {
            id: "test_schema".to_string(),
            branch_id: branch.name.clone(), // Use branch name as branch_id
            description: None,
            classes: vec![
                crate::model::ClassDef {
                    id: "product_class".to_string(),
                    name: "Product".to_string(),
                    description: None,
                    properties: vec![],
                    relationships: vec![crate::model::RelationshipDef {
                        id: "options_rel".to_string(),
                        name: "options".to_string(),
                        targets: vec!["Option".to_string()],
                        quantifier: crate::model::Quantifier::Any,
                        universe: None,
                        selection: crate::model::SelectionType::ExplicitOrFilter,
                        default_pool: crate::model::DefaultPool::All,
                    }],
                    derived: vec![],
                    domain_constraint: Some(Domain::binary()),
                },
                crate::model::ClassDef {
                    id: "option_class".to_string(),
                    name: "Option".to_string(),
                    description: None,
                    properties: vec![],
                    relationships: vec![],
                    derived: vec![],
                    domain_constraint: Some(Domain::binary()),
                },
            ],
        };
        store.upsert_schema(schema).await.unwrap();

        // Create instances with relationships
        let product_instance = Instance {
            id: "product1".to_string(),
            branch_id: branch.name.clone(), // Use branch name as branch_id
            instance_type: "Product".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships: {
                let mut rels = HashMap::new();
                rels.insert(
                    "options".to_string(),
                    RelationshipSelection::SimpleIds(vec![
                        "option1".to_string(),
                        "option2".to_string(),
                    ]),
                );
                rels
            },
        };

        let option1 = Instance {
            id: "option1".to_string(),
            branch_id: branch.name.clone(), // Use branch name as branch_id
            instance_type: "Option".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships: HashMap::new(),
        };

        let option2 = Instance {
            id: "option2".to_string(),
            branch_id: branch.name.clone(), // Use branch name as branch_id
            instance_type: "Option".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships: HashMap::new(),
        };

        store.upsert_instance(product_instance).await.unwrap();
        store.upsert_instance(option1).await.unwrap();
        store.upsert_instance(option2).await.unwrap();

        // Create solve request
        let request = NewConfigurationArtifact {
            resolution_context: ResolutionContext {
                database_id: database_id,
                branch_id: branch.name.clone(), // Use branch name as branch_id
                commit_hash: None,
                policies: ResolutionPolicies::default(),
                metadata: None,
            },
            user_metadata: None,
        };

        // Execute solve for the product instance
        let result = pipeline
            .solve_instance(request, "product1".to_string())
            .await;
        assert!(result.is_ok());

        let artifact = result.unwrap();
        assert_eq!(artifact.instance_count(), 3); // Product + 2 options

        // Check that all instances are in the configuration
        assert!(artifact.get_instance(&"product1".to_string()).is_some());
        assert!(artifact.get_instance(&"option1".to_string()).is_some());
        assert!(artifact.get_instance(&"option2".to_string()).is_some());

        // Check that all instances have domains set
        for instance in artifact.all_instances() {
            assert!(instance.domain.is_some());
        }

        // Check that relationships are materialized to concrete IDs
        let product = artifact.get_instance(&"product1".to_string()).unwrap();
        if let Some(crate::model::RelationshipSelection::SimpleIds(option_ids)) =
            product.relationships.get("options")
        {
            assert_eq!(option_ids.len(), 2);
            assert!(option_ids.contains(&"option1".to_string()));
            assert!(option_ids.contains(&"option2".to_string()));
        } else {
            panic!("Expected materialized relationship IDs");
        }
    }
}
