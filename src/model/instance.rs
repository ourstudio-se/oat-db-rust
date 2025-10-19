use crate::model::{DataType, Domain, Id, RelationshipSelection, RuleSet};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Default user for legacy data migration
fn default_user() -> String {
    "legacy-user".to_string()
}

/// Default timestamp for legacy data migration  
fn default_timestamp() -> DateTime<Utc> {
    // Use Unix epoch as default for legacy data
    DateTime::from_timestamp(0, 0).unwrap_or_else(|| Utc::now())
}

/// Local domain override for a specific variable within an instance
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalDomain {
    pub id: Id,
    pub domain: Domain,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub id: Id,
    #[serde(rename = "class")]
    pub class_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Domain>,
    pub properties: HashMap<String, PropertyValue>,
    pub relationships: HashMap<String, RelationshipSelection>,

    /// Local domain overrides for variables within this instance
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_domains: Vec<LocalDomain>,

    /// Audit fields for tracking who created/modified this instance
    #[serde(default = "default_user")]
    pub created_by: String,
    #[serde(default = "default_timestamp")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_user")]
    pub updated_by: String,
    #[serde(default = "default_timestamp")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum PropertyValue {
    Literal(TypedValue),
    Conditional(RuleSet),
}

// Custom deserialization to support both simple values and complex formats
impl<'de> Deserialize<'de> for PropertyValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;

        // Try to parse as TypedValue (explicit format with "type" and "value" fields)
        if let Ok(typed_value) = serde_json::from_value::<TypedValue>(value.clone()) {
            return Ok(PropertyValue::Literal(typed_value));
        }

        // Try to parse as RuleSet (conditional property with "rules" field)
        if let Ok(rule_set) = serde_json::from_value::<RuleSet>(value.clone()) {
            return Ok(PropertyValue::Conditional(rule_set));
        }

        // Fallback: infer type from the raw JSON value
        let typed_value = match &value {
            serde_json::Value::String(s) => TypedValue {
                value: serde_json::Value::String(s.clone()),
                data_type: DataType::String,
            },
            serde_json::Value::Number(n) => TypedValue {
                value: serde_json::Value::Number(n.clone()),
                data_type: DataType::Number,
            },
            serde_json::Value::Bool(b) => TypedValue {
                value: serde_json::Value::Bool(*b),
                data_type: DataType::Boolean,
            },
            serde_json::Value::Null => TypedValue {
                value: serde_json::Value::Null,
                data_type: DataType::String, // Default to string for null
            },
            _ => {
                return Err(serde::de::Error::custom(
                    "Invalid property format: expected string, number, boolean, or structured type",
                ));
            }
        };

        Ok(PropertyValue::Literal(typed_value))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TypedValue {
    pub value: serde_json::Value,
    #[serde(rename = "type")]
    pub data_type: DataType,
}

impl TypedValue {
    pub fn string(value: String) -> Self {
        Self {
            value: serde_json::Value::String(value),
            data_type: DataType::String,
        }
    }

    pub fn number(value: i32) -> Self {
        Self {
            value: serde_json::Value::Number(serde_json::Number::from(value)),
            data_type: DataType::Number,
        }
    }

    pub fn boolean(value: bool) -> Self {
        Self {
            value: serde_json::Value::Bool(value),
            data_type: DataType::Boolean,
        }
    }
}

/// Instance input model for creation (without ID and version_id)
/// The ID and version_id will be set server-side upon creation
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewInstance {
    #[serde(rename = "class")]
    pub class_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Domain>,
    pub properties: HashMap<String, PropertyValue>,
    pub relationships: HashMap<String, RelationshipSelection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_domains: Option<Vec<LocalDomain>>,
}

/// Instance update model for PATCH operations
/// All fields are optional for partial updates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InstanceUpdate {
    #[serde(rename = "class")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_id: Option<Id>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Domain>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, PropertyValue>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationships: Option<HashMap<String, RelationshipSelection>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExpandedInstance {
    pub id: Id,
    #[serde(rename = "class")]
    pub class_id: Id,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<Domain>,
    pub properties: HashMap<String, serde_json::Value>,
    pub relationships: HashMap<String, ResolvedRelationship>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub included: Vec<ExpandedInstance>,

    /// Local domain overrides for variables within this instance
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub local_domains: Vec<LocalDomain>,

    /// Audit fields for tracking who created/modified this instance
    #[serde(default = "default_user")]
    pub created_by: String,
    #[serde(default = "default_timestamp")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_user")]
    pub updated_by: String,
    #[serde(default = "default_timestamp")]
    pub updated_at: DateTime<Utc>,
}

impl ExpandedInstance {
    pub fn to_instance(&self) -> Instance {
        Instance {
            id: self.id.clone(),
            class_id: self.class_id.clone(),
            domain: self.domain.clone(),
            properties: self
                .properties
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        PropertyValue::Literal(TypedValue {
                            value: v.clone(),
                            data_type: match v {
                                serde_json::Value::String(_) => DataType::String,
                                serde_json::Value::Number(_) => DataType::Number,
                                serde_json::Value::Bool(_) => DataType::Boolean,
                                _ => DataType::String, // Default/fallback type
                            },
                        }),
                    )
                })
                .collect(),
            relationships: self
                .relationships
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        RelationshipSelection::SimpleIds(v.materialized_ids.clone()),
                    )
                })
                .collect(),
            local_domains: self.local_domains.clone(),
            created_by: self.created_by.clone(),
            created_at: self.created_at,
            updated_by: self.updated_by.clone(),
            updated_at: self.updated_at,
        }
    }
}

/// Enhanced relationship resolution with transparency about how IDs were resolved
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolvedRelationship {
    /// The actual resolved instance IDs
    pub materialized_ids: Vec<Id>,

    /// How these IDs were resolved
    pub resolution_method: ResolutionMethod,

    /// Additional details about the resolution process
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_details: Option<ResolutionDetails>,
}

/// Method used to resolve the relationship
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionMethod {
    /// IDs were explicitly set (no resolution needed)
    ExplicitIds,
    /// IDs were resolved from a pool filter
    PoolFilterResolved,
    /// IDs were resolved from a pool with explicit selection
    PoolSelectionResolved,
    /// IDs were resolved from a dynamic selector
    DynamicSelectorResolved,
    /// All instances of target types were selected
    AllInstancesResolved,
    /// IDs were resolved using schema defaults
    SchemaDefaultResolved,
    /// Resolution failed or returned empty
    EmptyResolution,
}

/// Additional details about how the resolution was performed
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionDetails {
    /// The original relationship definition before resolution
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_definition: Option<serde_json::Value>,

    /// What triggered the resolution (e.g., "pool_filter", "explicit_selection")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_from: Option<String>,

    /// Description of filters/conditions applied
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter_description: Option<String>,

    /// Total number of instances that matched the pool before selection
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_pool_size: Option<usize>,

    /// Number of instances that were excluded by filters
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filtered_out_count: Option<usize>,

    /// Time taken for this relationship resolution (microseconds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_time_us: Option<u64>,

    /// Any warnings or notes about the resolution
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl NewInstance {
    pub fn into_instance(self, id: Id, user_id: String) -> Instance {
        let now = Utc::now();
        Instance {
            id,
            // branch_id field removed in commit-based architecture
            class_id: self.class_id,
            domain: self.domain,
            properties: self.properties,
            relationships: self.relationships,
            local_domains: self.local_domains.unwrap_or_default(),
            created_by: user_id.clone(),
            created_at: now,
            updated_by: user_id,
            updated_at: now,
        }
    }
}

impl Default for Instance {
    fn default() -> Self {
        let now = Utc::now();
        let system_user = "system".to_string();

        Self {
            id: "default-instance".to_string(),
            class_id: "default-class".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships: HashMap::new(),
            local_domains: Vec::new(),
            created_by: system_user.clone(),
            created_at: now,
            updated_by: system_user,
            updated_at: now,
        }
    }
}

impl Default for ExpandedInstance {
    fn default() -> Self {
        let now = Utc::now();
        let system_user = "system".to_string();

        Self {
            id: "default-instance".to_string(),
            class_id: "default-class".to_string(),
            domain: None,
            properties: HashMap::new(),
            relationships: HashMap::new(),
            included: Vec::new(),
            local_domains: Vec::new(),
            created_by: system_user.clone(),
            created_at: now,
            updated_by: system_user,
            updated_at: now,
        }
    }
}

impl Instance {
    /// Apply updates from InstanceUpdate, preserving audit trail
    pub fn apply_update(&mut self, update: InstanceUpdate, user_id: String) {
        if let Some(class_id) = update.class_id {
            self.class_id = class_id;
        }
        if let Some(domain) = update.domain {
            self.domain = Some(domain);
        }
        if let Some(properties) = update.properties {
            self.properties = properties;
        }
        if let Some(relationships) = update.relationships {
            self.relationships = relationships;
        }

        // Update audit fields (preserve created_by/created_at)
        self.updated_by = user_id;
        self.updated_at = Utc::now();
    }
}
