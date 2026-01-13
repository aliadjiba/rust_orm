use serde::{Deserialize, Serialize};

use super::Model;
use surrealdb::sql::Thing;

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct Person {
    pub id: Option<Thing>,
    pub name: String,
    pub address: String,
    pub phone: Vec<String>,
}

impl Model for Person {
    fn table_name() -> &'static str {
        "person"
    }
}

// Manually implement Default
impl Default for Person {
    fn default() -> Self {
        Self {
            id: None,
            name: "".to_string(),
            address: "".to_string(),
            phone: vec![],
        }
    }
}
