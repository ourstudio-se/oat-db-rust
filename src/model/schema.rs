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

    /// Normalize the schema to ensure all PropertyDef instances have the value field
    /// This is useful for migration from older versions that don't have the value field
    pub fn normalize(&mut self) {
        for class in &mut self.classes {
            for property in &mut class.properties {
                // Ensure the value field exists (it should already be None if missing due to serde defaults)
                // This method is primarily for explicit normalization and future migration logic
                if property.value.is_none() {
                    property.value = None; // Explicitly set to None for consistency
                }
            }
        }
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<serde_json::Value>, // Default/constant value for this property
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
pub struct FnShort {
    pub method: String,  // e.g., "sum"
    pub property: String, // e.g., "price"
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DerivedDef {
    pub id: Id,
    pub name: String, // Logical name used for derived values (e.g., "totalPrice")
    pub data_type: DataType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expr: Option<Expr>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fn_short: Option<FnShort>,
}

impl DerivedDef {
    /// Get the expression for this derived property, expanding fn_short if needed
    pub fn get_expr(&self, class_def: &ClassDef) -> Option<Expr> {
        if let Some(expr) = &self.expr {
            Some(expr.clone())
        } else if let Some(fn_short) = &self.fn_short {
            match fn_short.method.as_str() {
                "sum" => {
                    // Build expression: own property + sum of all children's property
                    let own_prop = Expr::Prop {
                        prop: fn_short.property.clone(),
                    };
                    
                    // Collect all relationship sums
                    let mut sum_expr = own_prop;
                    
                    for rel in &class_def.relationships {
                        let rel_sum = Expr::Sum {
                            over: rel.name.clone(),
                            prop: fn_short.property.clone(),
                            r#where: None,
                        };
                        
                        sum_expr = Expr::Add {
                            left: Box::new(sum_expr),
                            right: Box::new(rel_sum),
                        };
                    }
                    
                    Some(sum_expr)
                }
                _ => None, // Unsupported method
            }
        } else {
            None
        }
    }
}
