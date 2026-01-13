use std::future::Future;

use serde::{de::DeserializeOwned, Serialize};
use surrealdb::sql::Thing;
use surrealdb::{Error, Surreal};

use crate::repository::{ErrorIO, Repo};
use erased_serde::Serialize as ErasedSerialize;
use std::marker::PhantomData;
pub trait Model: Sized + DeserializeOwned {
    fn table_name() -> &'static str;

    fn query<'a>(repo: &'a Repo) -> QueryBuilder<'a, Self> {
        QueryBuilder::new(repo)
    }
}
enum ConditionTarget {
    And,
    Or,
}

pub enum Order {
    Asc,
    Desc,
}

impl Order {
    fn as_str(&self) -> &'static str {
        match self {
            Order::Asc => "ASC",
            Order::Desc => "DESC",
        }
    }
}
pub struct QueryBuilder<'a, M> {
    repo: &'a Repo,
    select: Option<Vec<String>>,
    where_and: Vec<String>,
    where_or: Vec<String>,
    bindings: Vec<(String, Box<dyn ErasedSerialize + Send>)>,
    order_by: Option<(String, Order)>,
    limit: Option<u64>,
    offset: Option<u64>,
    _marker: PhantomData<M>,
}

impl<'a, M: Model> QueryBuilder<'a, M> {
    fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            select: None,
            where_and: vec![],
            where_or: vec![],
            bindings: vec![],
            order_by: None,
            limit: None,
            offset: None,
            _marker: PhantomData,
        }
    }
pub async fn insert<S>(self, data: S) -> Result<M, ErrorIO>
    where
        S: Serialize + Send + 'static,
    {
        let sql = format!("CREATE {} CONTENT $data", M::table_name());

        // Bind data
        let mut query = self.repo.db.query(sql).bind(("data", data));

        // Execute
        let mut response = query.await.map_err(ErrorIO::from)?;

        // FIX: deserialize from response -> Vec<M> first
        let records: Vec<M> = response.take(0).map_err(ErrorIO::from)?;

        // Extract first record
        records.into_iter().next().ok_or_else(|| ErrorIO::Db("No record returned".to_string()))
    }
    /* ---------------- SELECT ---------------- */

    pub fn select<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.select = Some(fields.into_iter().map(Into::into).collect());
        self
    }

    /* ---------------- WHERE (AND) ---------------- */

    pub fn where_eq<V>(mut self, field: &str, value: V) -> Self
    where
        V: Serialize + Send + 'static,
    {
        self.push_condition(ConditionTarget::And, field, "=", value);
        self
    }
    pub fn where_gt<V>(mut self, field: &str, value: V) -> Self
    where
        V: Serialize + Send + 'static,
    {
        self.push_condition(ConditionTarget::And, field, ">", value);
        self
    }

    pub fn where_lt<V>(mut self, field: &str, value: V) -> Self
    where
        V: Serialize + Send + 'static,
    {
        self.push_condition(ConditionTarget::And, field, "<", value);
        self
    }

    pub fn where_in<V>(mut self, field: &str, values: Vec<V>) -> Self
    where
        V: Serialize + Send + 'static,
    {
        let key = format!("v{}", self.bindings.len());
        self.where_and.push(format!("{} IN ${}", field, key));
        self.bindings.push((key, Box::new(values)));
        self
    }

    /* ---------------- WHERE (OR) ---------------- */

    pub fn or_where_eq<V>(mut self, field: &str, value: V) -> Self
where
    V: Serialize + Send + 'static,
{
    self.push_condition(ConditionTarget::Or, field, "=", value);
    self
}
    /* ---------------- ORDER / LIMIT ---------------- */

    pub fn order_by(mut self, field: &str, order: Order) -> Self {
        self.order_by = Some((field.to_string(), order));
        self
    }

    pub fn limit(mut self, limit: u64) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn offset(mut self, offset: u64) -> Self {
        self.offset = Some(offset);
        self
    }

    /* ---------------- EXECUTION ---------------- */

    pub async fn all(self) -> Result<Vec<M>, ErrorIO> {
        let select = self
            .select
            .map(|s| s.join(", "))
            .unwrap_or_else(|| "*".to_string());

        let mut sql = format!("SELECT {} FROM {}", select, M::table_name());

        if !self.where_and.is_empty() || !self.where_or.is_empty() {
            sql.push_str(" WHERE ");

            if !self.where_and.is_empty() {
                sql.push_str(&self.where_and.join(" AND "));
            }

            if !self.where_or.is_empty() {
                if !self.where_and.is_empty() {
                    sql.push_str(" OR ");
                }
                sql.push_str(&self.where_or.join(" OR "));
            }
        }

        if let Some((field, order)) = self.order_by {
            sql.push_str(&format!(" ORDER BY {} {}", field, order.as_str()));
        }

        if let Some(limit) = self.limit {
            sql.push_str(&format!(" LIMIT {}", limit));
        }

        if let Some(offset) = self.offset {
            sql.push_str(&format!(" START {}", offset));
        }

        let mut query = self.repo.db.query(sql);

        for (k, v) in self.bindings {
    // Move k into bind, giving it a 'static lifetime
            query = query.bind((k, v));
        }

        let mut response = query.await.map_err(ErrorIO::from)?;
        let data: Vec<M> = response.take(0).map_err(ErrorIO::from)?;

        Ok(data)
    }

    pub async fn first(self) -> Result<Option<M>, ErrorIO> {
        let mut results = self.limit(1).all().await?;
        Ok(results.pop())
    }

    /* ---------------- INTERNAL ---------------- */

    fn push_condition<V>(
    &mut self,
    target: ConditionTarget,
    field: &str,
    op: &str,
    value: V,
)
where
    V: Serialize + Send + 'static,
{
    let key = format!("v{}", self.bindings.len());
    let condition = format!("{} {} ${}", field, op, key);

    match target {
        ConditionTarget::And => self.where_and.push(condition),
        ConditionTarget::Or => self.where_or.push(condition),
    }

    self.bindings.push((key, Box::new(value)));
}

}