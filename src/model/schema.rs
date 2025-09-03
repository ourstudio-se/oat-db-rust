use crate::model::{ClassDef, DataType, Expr, Id, InstanceFilter, Quantifier, SelectionType};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Schema {
    pub id: Id,
    /// Collection of class/type definitions
    pub classes: Vec<ClassDef>,
    /// Optional schema description
    pub description: Option<String>,
}

impl Schema {
    /// Find a class definition by name
    pub fn get_class(&self, class_name: &str) -> Option<&ClassDef> {
        self.classes.iter().find(|class| class.name == class_name)
    }

    /// Find a class definition by ID
    pub fn get_class_by_id(&self, class_id: &Id) -> Option<&ClassDef> {
        self.classes.iter().find(|class| &class.id == class_id)
    }

    /// Find a property definition within any class by ID
    pub fn get_property_by_id(&self, property_id: &Id) -> Option<(&ClassDef, &PropertyDef)> {
        for class in &self.classes {
            if let Some(prop) = class.properties.iter().find(|p| &p.id == property_id) {
                return Some((class, prop));
            }
        }
        None
    }

    /// Find a relationship definition within any class by ID
    pub fn get_relationship_by_id(
        &self,
        relationship_id: &Id,
    ) -> Option<(&ClassDef, &RelationshipDef)> {
        for class in &self.classes {
            if let Some(rel) = class
                .relationships
                .iter()
                .find(|r| &r.id == relationship_id)
            {
                return Some((class, rel));
            }
        }
        None
    }

    /// Find a derived definition within any class by ID
    pub fn get_derived_by_id(&self, derived_id: &Id) -> Option<(&ClassDef, &DerivedDef)> {
        for class in &self.classes {
            if let Some(derived) = class.derived.iter().find(|d| &d.id == derived_id) {
                return Some((class, derived));
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropertyDef {
    pub id: Id,
    pub name: String, // Logical name used in instances (e.g., "name", "price")
    pub data_type: DataType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RelationshipDef {
    pub id: Id,
    pub name: String, // Logical name used in instances (e.g., "size", "fabric")
    pub targets: Vec<String>,
    pub quantifier: Quantifier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub universe: Option<String>,
    pub selection: SelectionType,
    /// Default pool for this relationship - what instances are considered by default
    pub default_pool: DefaultPool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum DefaultPool {
    /// No instances are included in the default pool
    None,
    /// All instances of the target type(s) are included
    All,
    /// A filtered subset of the target type(s) based on conditions
    Filter { 
        #[serde(rename = "type")]
        types: Option<Vec<String>>,
        #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
        filter: Option<InstanceFilter>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedDef {
    pub id: Id,
    pub name: String, // Logical name used for derived values (e.g., "totalPrice")
    pub data_type: DataType,
    pub expr: Expr,
}
