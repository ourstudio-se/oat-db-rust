use crate::model::{
    Id, Instance, ResolutionContext, ResolutionContextMetadata, ResolutionPolicies,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration artifact represents the immutable result of a solve operation
/// Contains all instances in the resolved configuration with materialized relationships
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationArtifact {
    /// Unique identifier for this artifact
    pub id: Id,

    /// When this artifact was created
    pub created_at: chrono::DateTime<chrono::Utc>,

    /// Resolution context that was used for this solve
    pub resolution_context: ResolutionContext,

    /// All instances in the resolved configuration (queried instance + all connected instances)
    /// All relationships are materialized with concrete instance IDs
    /// All instances have their domains resolved by the ILP solver
    pub configuration: Vec<Instance>,

    /// Solve metadata containing solver information, timing, etc.
    pub solve_metadata: SolveMetadata,

    /// Optional user-provided metadata about this artifact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<ArtifactUserMetadata>,

    /// Calculated derived properties for instances
    /// Maps instance_id -> property_name -> calculated_value
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub derived_properties: HashMap<Id, HashMap<String, serde_json::Value>>,
}

/// Notes about selector resolution (warnings, fallbacks, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionNote {
    /// Type of note (warning, info, error, etc.)
    pub note_type: ResolutionNoteType,

    /// Human-readable message about this resolution step
    pub message: String,

    /// Optional additional context or data
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Types of resolution notes
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionNoteType {
    /// Informational note
    Info,
    /// Warning about potential issues
    Warning,
    /// Error that was handled gracefully
    Error,
    /// Cross-branch reference detected
    CrossBranch,
    /// Missing instance was skipped
    SkippedMissing,
    /// Fallback selector was used
    UsedFallback,
    /// Selection was truncated due to size limits
    Truncated,
}

/// Metadata about the solve operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolveMetadata {
    /// Total time taken for the solve operation (in milliseconds)
    pub total_time_ms: u64,

    /// Phases of the solve pipeline with their timings
    pub pipeline_phases: Vec<PipelinePhase>,

    /// Solver used (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub solver_info: Option<SolverInfo>,

    /// Statistics about the solve operation
    pub statistics: SolveStatistics,

    /// Any errors or warnings during solving
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<SolveIssue>,
}

/// Information about a pipeline phase
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PipelinePhase {
    /// Name of the phase (snapshot, expand, compile, solve, etc.)
    pub name: String,

    /// Time taken for this phase (in milliseconds)
    pub duration_ms: u64,

    /// Optional additional information about this phase
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Information about the solver used
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolverInfo {
    /// Name of the solver (e.g., "CPLEX", "Gurobi", "OR-Tools")
    pub name: String,

    /// Version of the solver
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// Solver-specific configuration used
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub config: HashMap<String, serde_json::Value>,
}

/// Statistics about the solve operation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SolveStatistics {
    /// Number of instances in the configuration (queried + variable instances)
    pub total_instances: usize,

    /// Number of variable instances resolved by ILP solver
    pub variable_instances_resolved: usize,

    /// Number of conditional properties evaluated
    pub conditional_properties_evaluated: usize,

    /// Number of derived properties calculated
    pub derived_properties_calculated: usize,

    /// Number of ILP variables in the problem
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilp_variables: Option<usize>,

    /// Number of ILP constraints in the problem
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ilp_constraints: Option<usize>,

    /// Memory usage peak (in bytes, if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peak_memory_bytes: Option<usize>,
}

/// Issues that occurred during solving
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolveIssue {
    /// Severity of the issue
    pub severity: IssueSeverity,

    /// Human-readable description of the issue
    pub message: String,

    /// Which component caused the issue (instance_id, selector_id, etc.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,

    /// Additional context about the issue
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<serde_json::Value>,
}

/// Severity levels for solve issues
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    /// Information only
    Info,
    /// Warning - solved but with potential issues
    Warning,
    /// Error - solved but with compromises
    Error,
    /// Critical - solve failed
    Critical,
}

/// User-provided metadata for artifacts
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtifactUserMetadata {
    /// Human-readable name for this configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Description of what this configuration represents
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Tags for categorizing artifacts
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// User or system that created this artifact
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_by: Option<String>,

    /// Custom properties for extensions
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub custom: HashMap<String, serde_json::Value>,
}

/// Configuration artifact input model for creation
/// The ID and created_at will be generated server-side
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewConfigurationArtifact {
    /// Resolution context to use for solving
    pub resolution_context: ResolutionContext,

    /// Optional user metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<ArtifactUserMetadata>,
}

/// Instance query request for instance-specific solve operations
/// Database ID, branch ID, and instance ID are extracted from path parameters
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceQueryRequest {
    /// Resolution policies for this query
    #[serde(default)]
    pub policies: ResolutionPolicies,

    /// Optional commit hash for point-in-time resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,

    /// Optional user metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<ArtifactUserMetadata>,

    /// Optional context metadata  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_metadata: Option<ResolutionContextMetadata>,

    /// Optional list of derived property names to include in the response
    /// If None, no derived properties are calculated
    /// If Some(vec), only the specified derived properties are calculated and included
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_properties: Option<Vec<String>>,
}

/// Simple request for instance query with just property-weight pairs
/// Example: {"price": -1.0, "weight": 0.5, "derived_properties": ["total_cost", "summary"]}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleInstanceQueryRequest {
    /// Map of property names to objective weights
    /// Special key "derived_properties" is treated as a comma-separated list
    #[serde(flatten)]
    pub objectives: HashMap<String, serde_json::Value>,
}

impl SimpleInstanceQueryRequest {
    /// Extract the objective weights (f64 values) from the request
    pub fn get_objectives(&self) -> HashMap<String, f64> {
        self.objectives
            .iter()
            .filter_map(|(k, v)| {
                if k == "derived_properties" {
                    None
                } else {
                    v.as_f64().map(|weight| (k.clone(), weight))
                }
            })
            .collect()
    }

    /// Extract the derived properties list from the request
    pub fn get_derived_properties(&self) -> Option<Vec<String>> {
        self.objectives.get("derived_properties").and_then(|v| {
            // Handle both array format and comma-separated string format
            if let Some(arr) = v.as_array() {
                Some(
                    arr.iter()
                        .filter_map(|s| s.as_str().map(|s| s.to_string()))
                        .collect(),
                )
            } else if let Some(s) = v.as_str() {
                Some(s.split(',').map(|s| s.trim().to_string()).collect())
            } else {
                None
            }
        })
    }
}

/// Simple batch request for multiple queries with just a list of property-weight pairs
/// Example: {
///   "queries": [
///     {"id": "min_price", "price": -1.0, "weight": 0.5},
///     {"id": "max_comfort", "comfort": 1.0, "price": -0.5}
///   ],
///   "derived_properties": ["total_cost", "summary"]
/// }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleBatchInstanceQueryRequest {
    /// List of query objectives, each with an optional "id" field
    pub queries: Vec<HashMap<String, serde_json::Value>>,

    /// Optional derived properties to include in all responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_properties: Option<Vec<String>>,
}

impl SimpleBatchInstanceQueryRequest {
    /// Convert to the format expected by the solve pipeline
    pub fn to_objective_sets(&self) -> Vec<(String, HashMap<String, f64>)> {
        self.queries
            .iter()
            .enumerate()
            .map(|(idx, query)| {
                // Extract ID if provided, otherwise generate one
                let id = query
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| format!("query_{}", idx));

                // Extract objectives (all numeric fields except "id")
                let objectives: HashMap<String, f64> = query
                    .iter()
                    .filter_map(|(k, v)| {
                        if k == "id" {
                            None
                        } else {
                            v.as_f64().map(|weight| (k.clone(), weight))
                        }
                    })
                    .collect();

                (id, objectives)
            })
            .collect()
    }
}

/// Simple response for batch queries - just a list of configurations with their IDs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleBatchQueryResponse {
    /// List of configuration results
    pub results: Vec<SimpleConfigurationResult>,
}

/// A single configuration result in a simple batch query
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimpleConfigurationResult {
    /// The query ID this configuration corresponds to
    pub id: String,

    /// The resulting configuration artifact
    pub configuration: ConfigurationArtifact,
}

/// Request for batch instance queries with multiple objectives
/// Returns multiple configurations, one for each objective set
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchInstanceQueryRequest {
    /// List of objective sets to solve for
    /// Each objective set will produce one configuration in the response
    pub objectives: Vec<ObjectiveSet>,

    /// Resolution policies for all queries
    #[serde(default)]
    pub policies: ResolutionPolicies,

    /// Optional commit hash for point-in-time resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,

    /// Optional user metadata
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_metadata: Option<ArtifactUserMetadata>,

    /// Optional context metadata  
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_metadata: Option<ResolutionContextMetadata>,

    /// Optional list of derived property names to include in all responses
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived_properties: Option<Vec<String>>,

    /// Whether to include detailed solve metadata in responses (default: false for performance)
    #[serde(default)]
    pub include_metadata: bool,
}

/// A single set of objectives for solving
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ObjectiveSet {
    /// Unique identifier for this objective set (for matching with results)
    pub id: String,

    /// Map of instance ID to objective weight (coefficient for optimization)
    /// Positive weights favor selection, negative weights penalize selection
    pub objective: HashMap<String, f64>,
}

/// Response containing multiple configuration solutions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchQueryResponse {
    /// List of configuration results, one for each objective set
    pub configurations: Vec<ConfigurationResult>,

    /// Overall batch query metadata
    pub batch_metadata: BatchQueryMetadata,
}

/// A single configuration result from batch solving
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigurationResult {
    /// The objective set ID this configuration corresponds to
    pub objective_id: String,

    /// The resulting configuration artifact
    pub artifact: ConfigurationArtifact,

    /// Whether this configuration solved successfully
    pub success: bool,

    /// Error message if solution failed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Metadata for batch query operations
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BatchQueryMetadata {
    /// Total time for the entire batch operation (milliseconds)
    pub total_time_ms: u64,

    /// Number of objectives processed
    pub objectives_processed: usize,

    /// Number of successful solutions
    pub successful_solutions: usize,

    /// Number of failed solutions
    pub failed_solutions: usize,

    /// Instance ID that was queried
    pub queried_instance_id: String,

    /// Database and branch context
    pub database_id: String,
    pub branch_id: String,

    /// Optional commit hash for point-in-time resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commit_hash: Option<String>,
}

impl ConfigurationArtifact {
    /// Create a new configuration artifact
    pub fn new(
        id: Id,
        resolution_context: ResolutionContext,
        user_metadata: Option<ArtifactUserMetadata>,
    ) -> Self {
        Self {
            id,
            created_at: chrono::Utc::now(),
            resolution_context,
            configuration: Vec::new(),
            solve_metadata: SolveMetadata {
                total_time_ms: 0,
                pipeline_phases: Vec::new(),
                solver_info: None,
                statistics: SolveStatistics {
                    total_instances: 0,
                    variable_instances_resolved: 0,
                    conditional_properties_evaluated: 0,
                    derived_properties_calculated: 0,
                    ilp_variables: None,
                    ilp_constraints: None,
                    peak_memory_bytes: None,
                },
                issues: Vec::new(),
            },
            user_metadata,
            derived_properties: HashMap::new(),
        }
    }

    /// Add an instance to the configuration
    pub fn add_instance(&mut self, instance: Instance) {
        self.configuration.push(instance);
        self.solve_metadata.statistics.total_instances = self.configuration.len();
        self.solve_metadata.statistics.variable_instances_resolved = self.configuration.len();
    }

    /// Set the complete configuration (all instances)
    pub fn set_configuration(&mut self, instances: Vec<Instance>) {
        self.configuration = instances;
        self.solve_metadata.statistics.total_instances = self.configuration.len();
        self.solve_metadata.statistics.variable_instances_resolved = self.configuration.len();
    }

    /// Get the total number of instances in this configuration
    pub fn instance_count(&self) -> usize {
        self.configuration.len()
    }

    /// Check if this artifact represents a complete configuration
    /// (all instances have constant domains)
    pub fn is_complete_configuration(&self) -> bool {
        self.configuration.iter().all(|instance| {
            instance
                .domain
                .as_ref()
                .map(|d| d.is_constant())
                .unwrap_or(true) // No domain means it's complete
        })
    }

    /// Get a summary of the solve operation
    pub fn solve_summary(&self) -> String {
        let complete = if self.is_complete_configuration() {
            "complete"
        } else {
            "partial"
        };
        format!(
            "{} configuration with {} instances (solved in {}ms)",
            complete,
            self.instance_count(),
            self.solve_metadata.total_time_ms
        )
    }

    /// Check if the solve had any issues
    pub fn has_issues(&self) -> bool {
        !self.solve_metadata.issues.is_empty()
    }

    /// Get all issues from the solve operation
    pub fn all_issues(&self) -> Vec<String> {
        self.solve_metadata
            .issues
            .iter()
            .map(|issue| format!("{}: {}", issue.severity, issue.message))
            .collect()
    }

    /// Get all instances in this configuration
    pub fn all_instances(&self) -> &Vec<Instance> {
        &self.configuration
    }

    /// Find an instance by ID
    pub fn get_instance(&self, instance_id: &Id) -> Option<&Instance> {
        self.configuration
            .iter()
            .find(|instance| instance.id == *instance_id)
    }

    /// Find an instance by ID (mutable)
    pub fn get_instance_mut(&mut self, instance_id: &Id) -> Option<&mut Instance> {
        self.configuration
            .iter_mut()
            .find(|instance| instance.id == *instance_id)
    }

    /// Update ILP solver statistics
    pub fn update_ilp_statistics(&mut self, variables: usize, constraints: usize) {
        self.solve_metadata.statistics.ilp_variables = Some(variables);
        self.solve_metadata.statistics.ilp_constraints = Some(constraints);
    }
}

impl std::fmt::Display for ResolutionNoteType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResolutionNoteType::Info => write!(f, "INFO"),
            ResolutionNoteType::Warning => write!(f, "WARN"),
            ResolutionNoteType::Error => write!(f, "ERROR"),
            ResolutionNoteType::CrossBranch => write!(f, "CROSS_BRANCH"),
            ResolutionNoteType::SkippedMissing => write!(f, "SKIPPED"),
            ResolutionNoteType::UsedFallback => write!(f, "FALLBACK"),
            ResolutionNoteType::Truncated => write!(f, "TRUNCATED"),
        }
    }
}

impl std::fmt::Display for IssueSeverity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueSeverity::Info => write!(f, "INFO"),
            IssueSeverity::Warning => write!(f, "WARN"),
            IssueSeverity::Error => write!(f, "ERROR"),
            IssueSeverity::Critical => write!(f, "CRITICAL"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Domain, ResolutionPolicies};
    use std::collections::HashMap;

    fn create_test_instance(id: &str, class_id: &str) -> Instance {
        Instance {
            id: id.to_string(),
            // branch_id field removed in commit-based architecture
            class_id: class_id.to_string(),
            domain: Some(Domain::constant(1)),
            properties: HashMap::new(),
            relationships: HashMap::new(),
            created_at: chrono::Utc::now(),
            local_domains: Vec::new(),
            created_by: "test-user".to_string(),
            updated_at: chrono::Utc::now(),
            updated_by: "test-user".to_string(),
        }
    }

    #[test]
    fn test_artifact_creation() {
        let resolution_context = ResolutionContext {
            database_id: "db1".to_string(),
            branch_id: "branch1".to_string(),
            commit_hash: None,
            policies: ResolutionPolicies::default(),
            metadata: None,
        };

        let artifact =
            ConfigurationArtifact::new("artifact1".to_string(), resolution_context, None);

        assert_eq!(artifact.id, "artifact1");
        assert_eq!(artifact.instance_count(), 0); // Empty configuration
        assert!(artifact.is_complete_configuration()); // Empty is considered complete
        assert!(!artifact.has_issues());
    }

    #[test]
    fn test_artifact_with_variable_instances() {
        let resolution_context = ResolutionContext {
            database_id: "db1".to_string(),
            branch_id: "branch1".to_string(),
            commit_hash: None,
            policies: ResolutionPolicies::default(),
            metadata: None,
        };

        let mut artifact =
            ConfigurationArtifact::new("artifact1".to_string(), resolution_context, None);

        // Start with empty configuration
        assert_eq!(artifact.instance_count(), 0);
        assert!(artifact.is_complete_configuration());

        // Add a complete instance
        let complete_instance = create_test_instance("inst1", "Product");
        artifact.add_instance(complete_instance);

        assert_eq!(artifact.instance_count(), 1);
        assert!(artifact.is_complete_configuration());

        // Add an incomplete instance with range domain
        let mut incomplete_instance = create_test_instance("inst2", "Option");
        incomplete_instance.domain = Some(Domain::new(0, 2));
        artifact.add_instance(incomplete_instance);

        assert_eq!(artifact.instance_count(), 2);
        assert!(!artifact.is_complete_configuration());
        assert_eq!(
            artifact
                .solve_metadata
                .statistics
                .variable_instances_resolved,
            2
        );
    }

    #[test]
    fn test_get_instance() {
        let resolution_context = ResolutionContext {
            database_id: "db1".to_string(),
            branch_id: "branch1".to_string(),
            commit_hash: None,
            policies: ResolutionPolicies::default(),
            metadata: None,
        };

        let mut artifact =
            ConfigurationArtifact::new("artifact1".to_string(), resolution_context, None);

        let instance1 = create_test_instance("inst1", "Product");
        let instance2 = create_test_instance("inst2", "Option");
        artifact.add_instance(instance1);
        artifact.add_instance(instance2);

        // Test finding instances
        assert!(artifact.get_instance(&"inst1".to_string()).is_some());
        assert!(artifact.get_instance(&"inst2".to_string()).is_some());

        // Test missing instance
        assert!(artifact.get_instance(&"missing".to_string()).is_none());
    }

    #[test]
    fn test_artifact_summary() {
        let resolution_context = ResolutionContext {
            database_id: "db1".to_string(),
            branch_id: "branch1".to_string(),
            commit_hash: None,
            policies: ResolutionPolicies::default(),
            metadata: None,
        };

        let mut artifact =
            ConfigurationArtifact::new("artifact1".to_string(), resolution_context, None);

        let queried_instance = create_test_instance("queried1", "Product");
        artifact.add_instance(queried_instance);

        artifact.solve_metadata.total_time_ms = 250;

        let summary = artifact.solve_summary();
        assert!(summary.contains("complete configuration"));
        assert!(summary.contains("1 instances"));
        assert!(summary.contains("250ms"));
    }

    #[test]
    fn test_ilp_statistics() {
        let resolution_context = ResolutionContext {
            database_id: "db1".to_string(),
            branch_id: "branch1".to_string(),
            commit_hash: None,
            policies: ResolutionPolicies::default(),
            metadata: None,
        };

        let mut artifact =
            ConfigurationArtifact::new("artifact1".to_string(), resolution_context, None);

        let queried_instance = create_test_instance("queried1", "Product");
        artifact.add_instance(queried_instance);

        artifact.update_ilp_statistics(10, 15);

        assert_eq!(artifact.solve_metadata.statistics.ilp_variables, Some(10));
        assert_eq!(artifact.solve_metadata.statistics.ilp_constraints, Some(15));
    }
}
