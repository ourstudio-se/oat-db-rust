use crate::model::{DerivedDef, Domain, Id, PropertyDef, RelationshipDef};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Default user for legacy data migration
fn default_user() -> String {
    "legacy-user".to_string()
}

/// Default timestamp for legacy data migration
fn default_timestamp() -> DateTime<Utc> {
    // Use Unix epoch as default for legacy data
    DateTime::from_timestamp(0, 0).unwrap_or_else(|| Utc::now())
}

/// Represents a class/type definition within a schema
/// Each class defines the structure for instances of that type
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassDef {
    /// Unique identifier for this class definition
    pub id: Id,

    /// Name of the class/type (e.g., "Underbed", "Size", "Fabric", "Leg")
    pub name: String,

    /// Properties specific to this class
    pub properties: Vec<PropertyDef>,

    /// Relationships specific to this class  
    pub relationships: Vec<RelationshipDef>,

    /// Derived fields specific to this class
    pub derived: Vec<DerivedDef>,

    /// Optional description of what this class represents
    pub description: Option<String>,

    /// Domain constraint for instances of this class (defines allowed lower/upper bounds)
    /// If None, instances have no domain constraints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_constraint: Option<Domain>,

    /// Audit fields for tracking who created/modified this class
    #[serde(default = "default_user")]
    pub created_by: String,
    #[serde(default = "default_timestamp")]
    pub created_at: DateTime<Utc>,
    #[serde(default = "default_user")]
    pub updated_by: String,
    #[serde(default = "default_timestamp")]
    pub updated_at: DateTime<Utc>,
}

/// Class definition input model for creation
/// The ID can be provided by the user or will be generated server-side if not provided
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewClassDef {
    /// Optional ID - if not provided, will be generated server-side
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<Id>,

    /// Name of the class/type (e.g., "Underbed", "Size", "Fabric", "Leg")
    pub name: String,

    /// Properties specific to this class
    pub properties: Vec<PropertyDef>,

    /// Relationships specific to this class  
    pub relationships: Vec<RelationshipDef>,

    /// Derived fields specific to this class
    pub derived: Vec<DerivedDef>,

    /// Optional description of what this class represents
    pub description: Option<String>,

    /// Domain constraint for instances of this class (defines allowed lower/upper bounds)
    /// If None, instances have no domain constraints
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_constraint: Option<Domain>,
}

/// Class definition update model for PATCH operations
/// All fields are optional for partial updates
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ClassDefUpdate {
    /// Name of the class/type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// Properties specific to this class (replaces entire property list)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<Vec<PropertyDef>>,

    /// Relationships specific to this class (replaces entire relationship list)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationships: Option<Vec<RelationshipDef>>,

    /// Derived fields specific to this class (replaces entire derived list)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derived: Option<Vec<DerivedDef>>,

    /// Optional description of what this class represents
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,

    /// Domain constraint for instances of this class (defines allowed lower/upper bounds)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain_constraint: Option<Domain>,
}

impl Default for ClassDef {
    fn default() -> Self {
        let now = Utc::now();
        let system_user = "system".to_string();
        
        Self {
            id: "default-class".to_string(),
            name: "Default".to_string(),
            properties: Vec::new(),
            relationships: Vec::new(),
            derived: Vec::new(),
            description: None,
            domain_constraint: None,
            created_by: system_user.clone(),
            created_at: now,
            updated_by: system_user,
            updated_at: now,
        }
    }
}

impl ClassDef {
    /// Create a new ClassDef from NewClassDef with audit information
    pub fn from_new(new_class: NewClassDef, user_id: String) -> Self {
        let now = Utc::now();
        Self {
            id: new_class.id.unwrap_or_else(crate::model::generate_id),
            name: new_class.name,
            properties: new_class.properties,
            relationships: new_class.relationships,
            derived: new_class.derived,
            description: new_class.description,
            domain_constraint: new_class.domain_constraint,
            created_by: user_id.clone(),
            created_at: now,
            updated_by: user_id,
            updated_at: now,
        }
    }

    /// Update this ClassDef with changes from ClassDefUpdate, preserving audit trail
    pub fn apply_update(&mut self, update: ClassDefUpdate, user_id: String) {
        if let Some(name) = update.name {
            self.name = name;
        }
        if let Some(properties) = update.properties {
            self.properties = properties;
        }
        if let Some(relationships) = update.relationships {
            self.relationships = relationships;
        }
        if let Some(derived) = update.derived {
            self.derived = derived;
        }
        if let Some(description) = update.description {
            self.description = Some(description);
        }
        if let Some(domain_constraint) = update.domain_constraint {
            self.domain_constraint = Some(domain_constraint);
        }
        
        // Update audit fields (preserve created_by/created_at)
        self.updated_by = user_id;
        self.updated_at = Utc::now();
    }
}
