use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::SurrealValue;
use std::marker::PhantomData;
use crate::{model::{Model, SqlState}, repository::{Repo}};
use crate::error::ErrorIO;

fn quote_table(name: &str) -> String {
    format!("`{name}`")
}

use std::future::IntoFuture;
use std::future::Future;
use std::pin::Pin;
/* ===========================
   UPDATE / UPSERT
=========================== */

pub struct Update<'a, M, R = M> {
    repo: &'a Repo,
    state: SqlState,
    sets: Vec<String>,
    _m: PhantomData<M>,
    _r: PhantomData<R>,
}

impl<'a, M, R> IntoFuture for Update<'a, M, R>
where
    M: Model + SurrealValue + Send + Sync + 'static,
    R: Serialize + DeserializeOwned + SurrealValue + Send + Sync + 'static,
{
    type Output = Result<R, ErrorIO>;

    type IntoFuture = Pin<Box<dyn Future<Output = Self::Output> + Send + 'a>>;

    fn into_future(self) -> Self::IntoFuture {
        Box::pin(async move {
            self.update_as::<R>().await
        })
    }
}

impl<'a, M: Model,R> Update<'a, M,R> {
    #[allow(dead_code)]
    pub(super) fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            state: SqlState::new(),
            sets: vec![],
            _m: PhantomData,
            _r: PhantomData,
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
    pub async fn value<V,RS>(self, data: V) -> Result<RS, ErrorIO>
    where
        V: Serialize + SurrealValue + Send + Sync + 'static,
        RS: DeserializeOwned + SurrealValue,
    {
        let mut state = self.state;

        // bind the whole value
        let key = state.bind(data);

        let mut sql = format!(
            "UPDATE {} CONTENT ${}",
            quote_table(M::table_name()),
            key
        );

        if !state.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&state.conditions.join(" AND "));
        }

        sql.push_str(" RETURN *");

        let mut query = self.repo.db.query(sql);
        for (k, v) in state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        res.take::<Option<RS>>(0)
            .map_err(ErrorIO::from)?
            .ok_or_else(|| ErrorIO::Db("Update failed".into()))
    }
    pub fn where_eq<V: Serialize + SurrealValue + Send + Sync + 'static>(
        mut self,
        field: &str,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} = ${key}"));
        self
    }
    pub fn by_id<V: Serialize + SurrealValue + Send + Sync + 'static>(
        mut self,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("id = ${key}"));
        self
    }
    pub async fn update_as<T>(self) -> Result<T, ErrorIO>
    where
        T: DeserializeOwned + SurrealValue,
    {
        let mut sql = format!(
            "UPDATE {} SET {}",
            quote_table(M::table_name()),
            self.sets.join(", ")
        );

        if !self.state.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.conditions.join(" AND "));
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
            quote_table(M::table_name()),
            self.sets.join(", ")
        );

        if !self.state.conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.conditions.join(" AND "));
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
    pub fn as_type<NR>(self) -> Update<'a, M, NR> {
        Update {
            repo:self.repo,
            state: self.state,
            sets: self.sets,
            _m: PhantomData,
            _r: PhantomData,
        }
    }
}
