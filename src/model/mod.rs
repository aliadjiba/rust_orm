mod query;
mod upsert;
mod delete;
mod insert;
mod relations;
mod model;
pub use insert::Insert;
pub use query::Query;
pub use delete::Delete;
pub use upsert::Update;
// pub use insert::Insert;
pub use model::*;
use serde::Serialize;
use erased_serde::Serialize as ErasedSerialize;
use std::sync::Arc;
pub use relations::*;
pub struct Page<T> {
    pub data: Vec<T>,
    pub page: u64,
    pub per_page: u64,
    pub total: u64,
    pub total_pages: u64,
}

/* ===========================
   SQL STATE
=========================== */
struct SqlState {
    where_and: Vec<String>,
    bindings: Vec<(String, Arc<dyn ErasedSerialize + Send + Sync>)>,
}
impl SqlState {
    fn new() -> Self {
        Self {
            where_and: vec![],
            bindings: vec![],
        }
    }

 fn bind<V: Serialize + Send + Sync + 'static>(&mut self, value: V) -> String {
        let key = format!("v{}", self.bindings.len());
        self.bindings.push((key.clone(), Arc::new(value)));
        key
    }
}

