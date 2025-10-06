//! Analysis module for statistical analysis of instance properties
//!
//! Provides various analysis methods like distribution, aggregation, etc.
//! on instance properties within a solved configuration.

use crate::logic::solve_pipeline::SolvePipeline;
use crate::model::{
    CommitData, Id, Instance, NewConfigurationArtifact, ResolutionContext, ResolutionPolicies,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Analysis request containing method and parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "lowercase")]
pub enum AnalysisMethod {
    /// Distribution analysis with configurable intervals
    Distribution { params: DistributionParams },
    /// Summary statistics (min, max, mean, median, etc.)
    Summary { params: SummaryParams },
    /// Aggregation by grouping
    Aggregate { params: AggregateParams },
}

/// Parameters for distribution analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionParams {
    /// Field to analyze (e.g., "price", "$.properties.weight")
    pub field: String,
    /// Interval size for buckets
    pub interval: f64,
    /// Minimum value for distribution
    pub min: f64,
    /// Maximum value for distribution
    pub max: f64,
}

/// Parameters for summary statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryParams {
    /// Field to analyze
    pub field: String,
}

/// Parameters for aggregation analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateParams {
    /// Field to aggregate
    pub field: String,
    /// Field to group by (e.g., "class_id")
    pub group_by: String,
    /// Aggregation function: sum, avg, min, max, count
    #[serde(default = "default_agg_function")]
    pub function: String,
}

fn default_agg_function() -> String {
    "sum".to_string()
}

/// Result of an analysis operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum AnalysisResult {
    /// Distribution result with buckets
    Distribution {
        field: String,
        buckets: Vec<DistributionBucket>,
        total_count: usize,
        stats: DistributionStats,
    },
    /// Summary statistics result
    Summary {
        field: String,
        min: f64,
        max: f64,
        mean: f64,
        median: f64,
        count: usize,
    },
    /// Aggregation result
    Aggregate {
        field: String,
        group_by: String,
        function: String,
        groups: Vec<AggregateGroup>,
    },
}

/// A bucket in a distribution - represents a configuration closest to a target interval
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionBucket {
    /// Target interval value we're trying to get close to
    pub target: f64,
    /// Actual achieved sum in the closest configuration (≤ target)
    pub achieved: f64,
    /// Whether a valid configuration was found
    pub has_solution: bool,
    /// Gap between target and achieved (target - achieved)
    pub gap: f64,
}

/// Statistics for distribution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributionStats {
    pub mean: f64,
    pub median: f64,
    pub std_dev: f64,
}

/// A group in an aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateGroup {
    pub key: String,
    pub value: f64,
    pub count: usize,
}

/// Analyzer for instance analysis
pub struct InstanceAnalyzer;

impl InstanceAnalyzer {
    /// Analyze instances based on the requested method
    pub fn analyze(
        method: AnalysisMethod,
        target_instance_id: &Id,
        commit_data: &CommitData,
    ) -> anyhow::Result<AnalysisResult> {
        match method {
            AnalysisMethod::Distribution { params } => {
                Self::analyze_distribution(params, target_instance_id, commit_data)
            }
            AnalysisMethod::Summary { params } => {
                Self::analyze_summary(params, target_instance_id, commit_data)
            }
            AnalysisMethod::Aggregate { params } => {
                Self::analyze_aggregate(params, target_instance_id, commit_data)
            }
        }
    }

    /// Analyze distribution of a field using solver with constraints
    fn analyze_distribution(
        params: DistributionParams,
        target_instance_id: &Id,
        commit_data: &CommitData,
    ) -> anyhow::Result<AnalysisResult> {
        // Step 1: Extract field values for all instances
        let field_values: HashMap<String, f64> = commit_data
            .instances
            .iter()
            .filter_map(|inst| {
                Self::extract_field_value(&params.field, inst)
                    .ok()
                    .map(|val| (inst.id.clone(), val))
            })
            .collect();

        if field_values.is_empty() {
            return Ok(AnalysisResult::Distribution {
                field: params.field,
                buckets: vec![],
                total_count: 0,
                stats: DistributionStats {
                    mean: 0.0,
                    median: 0.0,
                    std_dev: 0.0,
                },
            });
        }

        // Step 2: Calculate statistics from all field values
        let all_values: Vec<f64> = field_values.values().copied().collect();
        let stats = Self::calculate_stats(&all_values);

        // Step 3: Create buckets by solving with constraints for each interval
        // We iterate through interval values: min+interval, min+2*interval, ..., max
        // For each interval value, we solve with constraint: sum(field * selection) <= interval
        let num_buckets = ((params.max - params.min) / params.interval).ceil() as usize;
        let mut buckets = Vec::new();
        let pipeline = SolvePipeline::new(commit_data);

        // Build all objectives at once for batch solving
        let mut objective_sets = Vec::new();
        for i in 0..num_buckets {
            let interval_value = params.min + ((i + 1) as f64 * params.interval);
            objective_sets.push((
                format!("interval_{}", interval_value),
                field_values.clone(), // Maximize sum of field values
            ));
        }

        // Create a dummy request with minimal resolution context
        let request = NewConfigurationArtifact {
            resolution_context: ResolutionContext {
                database_id: "analysis".to_string(),
                branch_id: "analysis".to_string(),
                commit_hash: None,
                policies: ResolutionPolicies::default(),
                metadata: None,
            },
            user_metadata: None,
        };

        // Solve once with all objectives and interval-specific constraints
        for i in 0..num_buckets {
            let bucket_min = params.min + (i as f64 * params.interval);
            let bucket_max = bucket_min + params.interval;
            let interval_value = bucket_max;

            // Solve with constraint: sum(field_value * instance_selection) <= interval_value
            let result = pipeline.solve_instance_with_constraints(
                request.clone(),
                target_instance_id.clone(),
                vec![(format!("interval_{}", interval_value), field_values.clone())],
                None,
                |model, id_mappings| {
                    // Build "at most interval_value" constraint
                    // sum(field_value * var) - interval_value <= 0
                    // => sum(-field_value * var) + interval_value >= 0
                    let mut coefficients = Vec::new();

                    for (instance_id, &field_value) in field_values.iter() {
                        if let Some(pldag_id) = id_mappings.get_pldag_id(instance_id) {
                            coefficients.push((pldag_id, -(field_value as i64)));
                        }
                    }

                    model.set_gelineq(coefficients, interval_value as i64);
                    Ok(())
                },
            );

            // The solver maximizes the sum, so it will find the configuration
            // with maximum sum of field values that is <= interval_value
            let bucket = match result {
                Ok(solutions) => {
                    solutions
                        .first()
                        .map(|(_, artifact)| {
                            // Calculate the actual sum achieved in this configuration
                            // Sum = field_value * domain.lower for each instance
                            let achieved_sum: f64 = artifact
                                .configuration
                                .iter()
                                .filter_map(|inst| {
                                    field_values.get(&inst.id).and_then(|&field_value| {
                                        inst.domain.as_ref().map(|domain| {
                                            field_value * domain.lower as f64
                                        })
                                    })
                                })
                                .sum();

                            DistributionBucket {
                                target: interval_value,
                                achieved: achieved_sum,
                                has_solution: true,
                                gap: interval_value - achieved_sum,
                            }
                        })
                        .unwrap_or(DistributionBucket {
                            target: interval_value,
                            achieved: 0.0,
                            has_solution: false,
                            gap: interval_value,
                        })
                }
                Err(_) => DistributionBucket {
                    target: interval_value,
                    achieved: 0.0,
                    has_solution: false,
                    gap: interval_value,
                },
            };

            buckets.push(bucket);
        }

        Ok(AnalysisResult::Distribution {
            field: params.field,
            buckets,
            total_count: all_values.len(),
            stats,
        })
    }

    /// Analyze summary statistics
    fn analyze_summary(
        params: SummaryParams,
        target_instance_id: &Id,
        commit_data: &CommitData,
    ) -> anyhow::Result<AnalysisResult> {
        let dependencies = Self::get_dependencies(target_instance_id, &commit_data.instances)?;
        let mut values = Self::extract_field_values(&params.field, &commit_data.instances)?;

        if values.is_empty() {
            return Ok(AnalysisResult::Summary {
                field: params.field,
                min: 0.0,
                max: 0.0,
                mean: 0.0,
                median: 0.0,
                count: 0,
            });
        }

        values.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let min = *values.first().unwrap();
        let max = *values.last().unwrap();
        let mean = values.iter().sum::<f64>() / values.len() as f64;
        let median = if values.len() % 2 == 0 {
            (values[values.len() / 2 - 1] + values[values.len() / 2]) / 2.0
        } else {
            values[values.len() / 2]
        };

        Ok(AnalysisResult::Summary {
            field: params.field,
            min,
            max,
            mean,
            median,
            count: values.len(),
        })
    }

    /// Analyze aggregation by groups
    fn analyze_aggregate(
        params: AggregateParams,
        target_instance_id: &Id,
        commit_data: &CommitData,
    ) -> anyhow::Result<AnalysisResult> {
        let dependencies = Self::get_dependencies(target_instance_id, &commit_data.instances)?;

        // Group instances
        let mut groups: HashMap<String, Vec<f64>> = HashMap::new();

        for id in &dependencies {
            if let Some(instance) = commit_data.instances.iter().find(|i| &i.id == id) {
                let group_key = Self::extract_group_key(&params.group_by, instance)?;
                let value = Self::extract_field_value(&params.field, instance)?;

                groups.entry(group_key).or_default().push(value);
            }
        }

        // Apply aggregation function
        let mut result_groups = Vec::new();

        for (key, values) in groups {
            let (value, count) = match params.function.as_str() {
                "sum" => (values.iter().sum(), values.len()),
                "avg" | "mean" => (
                    values.iter().sum::<f64>() / values.len() as f64,
                    values.len(),
                ),
                "min" => (
                    values.iter().copied().fold(f64::INFINITY, f64::min),
                    values.len(),
                ),
                "max" => (
                    values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                    values.len(),
                ),
                "count" => (values.len() as f64, values.len()),
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unknown aggregation function: {}",
                        params.function
                    ))
                }
            };

            result_groups.push(AggregateGroup { key, value, count });
        }

        // Sort by key for consistent output
        result_groups.sort_by(|a, b| a.key.cmp(&b.key));

        Ok(AnalysisResult::Aggregate {
            field: params.field,
            group_by: params.group_by,
            function: params.function,
            groups: result_groups,
        })
    }

    /// Get all dependencies of an instance
    fn get_dependencies(target_id: &Id, instances: &[Instance]) -> anyhow::Result<Vec<Id>> {
        use std::collections::{HashSet, VecDeque};

        let mut dependencies = HashSet::new();
        let mut to_process = VecDeque::new();

        dependencies.insert(target_id.clone());
        to_process.push_back(target_id.clone());

        let instance_map: HashMap<&Id, &Instance> =
            instances.iter().map(|inst| (&inst.id, inst)).collect();

        while let Some(current_id) = to_process.pop_front() {
            if let Some(instance) = instance_map.get(&current_id) {
                for selection in instance.relationships.values() {
                    if let crate::model::RelationshipSelection::SimpleIds(target_ids) = selection {
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

        Ok(dependencies.into_iter().collect())
    }

    /// Extract numeric values from a field across instances
    fn extract_field_values(field: &str, all_instances: &[Instance]) -> anyhow::Result<Vec<f64>> {
        let mut values = Vec::new();

        for instance in all_instances {
            if let Ok(value) = Self::extract_field_value(field, instance) {
                values.push(value);
            }
        }

        Ok(values)
    }

    /// Extract a single numeric value from a field
    fn extract_field_value(field: &str, instance: &Instance) -> anyhow::Result<f64> {
        // Handle property access (remove leading $. if present)
        let field_name = field.strip_prefix("$.properties.").unwrap_or(field);

        if let Some(prop) = instance.properties.get(field_name) {
            match prop {
                crate::model::PropertyValue::Literal(typed_val) => {
                    if let Some(num) = typed_val.value.as_f64() {
                        return Ok(num);
                    }
                    if let Some(int) = typed_val.value.as_i64() {
                        return Ok(int as f64);
                    }
                }
                _ => {}
            }
        }

        Err(anyhow::anyhow!(
            "Field '{}' not found or not numeric in instance {}",
            field,
            instance.id
        ))
    }

    /// Extract group key from instance
    fn extract_group_key(field: &str, instance: &Instance) -> anyhow::Result<String> {
        match field {
            "class_id" => Ok(instance.class_id.clone()),
            "id" => Ok(instance.id.clone()),
            _ => {
                // Try to get from properties as string
                let field_name = field.strip_prefix("$.properties.").unwrap_or(field);
                if let Some(prop) = instance.properties.get(field_name) {
                    if let crate::model::PropertyValue::Literal(typed_val) = prop {
                        return Ok(typed_val.value.to_string());
                    }
                }
                Err(anyhow::anyhow!(
                    "Field '{}' not found in instance {}",
                    field,
                    instance.id
                ))
            }
        }
    }

    /// Calculate statistics for a set of values
    fn calculate_stats(values: &[f64]) -> DistributionStats {
        if values.is_empty() {
            return DistributionStats {
                mean: 0.0,
                median: 0.0,
                std_dev: 0.0,
            };
        }

        let mean = values.iter().sum::<f64>() / values.len() as f64;

        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

        let median = if sorted.len() % 2 == 0 {
            (sorted[sorted.len() / 2 - 1] + sorted[sorted.len() / 2]) / 2.0
        } else {
            sorted[sorted.len() / 2]
        };

        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
        let std_dev = variance.sqrt();

        DistributionStats {
            mean,
            median,
            std_dev,
        }
    }
}
