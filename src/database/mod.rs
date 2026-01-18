use async_trait::async_trait;
use surrealdb::Value;

#[async_trait]
pub trait Database: Send + Sync {
    async fn query(
        &self,
        sql: String,
        bindings: Vec<(String, Value)>,
    ) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>>;
}