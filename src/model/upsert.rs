use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::SurrealValue;
use std::marker::PhantomData;
use crate::{model::{Model, SqlState}, repository::{ErrorIO, Repo}};
use std::future::IntoFuture;
use std::future::Future;
use std::pin::Pin;
/* ===========================
   UPDATE / UPSERT
=========================== */

pub struct Update<'a, M> {
    repo: &'a Repo,
    state: SqlState,
    sets: Vec<String>,
    _m: PhantomData<M>,
}

impl<'a, M> IntoFuture for Update<'a, M>
where
    M: Model + SurrealValue + Send + Sync + 'static,   // 🔥 ADD Send HERE
{
    type Output = Result<M, ErrorIO>;

    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            self.update_as::<M>().await
        })
    }
}

impl<'a, M: Model> Update<'a, M> {
    pub(super) fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            state: SqlState::new(),
            sets: vec![],
            _m: PhantomData,
        }
    }

    pub fn set<V: Serialize + SurrealValue + Send + Sync + 'static,>(
        mut self,
        field: &str,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.sets.push(format!("{field} = ${key}"));
        self
    }
    pub fn values<I, S, V>(mut self, items: I) -> Self
    where
        I: IntoIterator<Item = (S, V)>,
        S: Into<String>,
        V: Serialize + SurrealValue + Send + Sync + 'static,
    {
        for (field, value) in items {
            let key = self.state.bind(value);
            self.sets.push(format!("{} = ${}", field.into(), key));
        }
        self
    }
    pub async fn value<V>(self, data: V) -> Result<M, ErrorIO>
    where
        V: Serialize + SurrealValue + Send + Sync + 'static,
    {
        let mut state = self.state;

        // bind the whole value
        let key = state.bind(data);

        let mut sql = format!(
            "UPDATE {} CONTENT ${}",
            M::table_name(),
            key
        );

        if !state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&state.where_and.join(" AND "));
        }

        sql.push_str(" RETURN *");

        let mut query = self.repo.db.query(sql);
        for (k, v) in state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        res.take::<Option<M>>(0)
            .map_err(ErrorIO::from)?
            .ok_or_else(|| ErrorIO::Db("Update failed".into()))
    }
    pub fn where_eq<V: Serialize + SurrealValue + Send + Sync + 'static>(
        mut self,
        field: &str,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("{field} = ${key}"));
        self
    }
    pub fn by_id<V: Serialize + SurrealValue + Send + Sync + 'static>(
        mut self,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("id = ${key}"));
        self
    }
    pub async fn update_as<T>(self) -> Result<T, ErrorIO>
    where
        T: DeserializeOwned + SurrealValue,
    {
        let mut sql = format!(
            "UPDATE {} SET {}",
            M::table_name(),
            self.sets.join(", ")
        );

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        sql.push_str(" RETURN *");

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        res.take::<Option<T>>(0)
            .map_err(ErrorIO::from)?
            .ok_or_else(|| ErrorIO::Db("Update failed".into()))
    }

    pub async fn upsert_as<T>(self) -> Result<T, ErrorIO>
    where
        T: DeserializeOwned + SurrealValue,
    {
        let mut sql = format!(
            "UPSERT INTO {} CONTENT {{ {} }}",
            M::table_name(),
            self.sets.join(", ")
        );

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        sql.push_str(" RETURN *");

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        res.take::<Option<T>>(0)
            .map_err(ErrorIO::from)?
            .ok_or_else(|| ErrorIO::Db("Upsert failed".into()))
    }
}
