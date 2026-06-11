pub mod edges;
pub mod query;
mod model;
pub mod upsert;
pub mod relations;
pub use model::Model;
pub use query::QueryBuilder as Query;
pub use query::*;
use surrealdb::types::{SurrealValue, Value};
pub use upsert::Update;
pub use edges::*;
pub use relations::*;
pub struct Page<T> {
    pub data: Vec<T>,
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
    pub total_pages: u64,
}


pub trait SurrealType {
    fn surreal_type() -> &'static str {
        "string"  // sensible default
    }
}

pub trait SurrealSchema {
    fn nested_fields(_table: &str, _prefix: &str) -> Vec<String> {
        vec![]
    }
}

pub trait SurrealEnum {
    fn surreal_type() -> &'static str { "string" }
}


/* ===========================
   SQL STATE
=========================== */
#[derive(Clone, Debug)]
struct SqlState {
    conditions: Vec<String>,
    bindings: Vec<(String, Value)>,
}
impl SqlState {
    fn new() -> Self {
        Self {
            conditions: vec![],
            bindings: vec![],
        }
    }
    #[allow(dead_code)]
    pub fn add_condition(&mut self, condition: String) {
        self.conditions.push(condition);
    }

    pub fn bind<V>(&mut self, value: V) -> String
        where
            V: SurrealValue,
        {
            let key = format!("v{}", self.bindings.len());
            self.bindings.push((key.clone(), value.into_value()));
            key
        }
    pub fn bind_value<V>(&mut self, _field: &str, value: V) -> String
    where
        V: SurrealValue,
    {
        let key = format!("v{}", self.bindings.len());
        self.bindings.push((key.clone(), value.into_value()));
        key
    }
    #[allow(dead_code)]
    pub fn where_clause(&self) -> String {
        if self.conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", self.conditions.join(" AND "))
        }
    }
}