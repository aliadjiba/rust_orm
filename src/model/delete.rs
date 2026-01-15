use serde::Serialize;
use std::marker::PhantomData;
use crate::{model::{Model, SqlState}, repository::{ErrorIO, Repo}};

pub struct Delete<'a, M> {
    repo: &'a Repo,
    state: SqlState,
    _m: PhantomData<M>,
}

impl<'a, M: Model> Delete<'a, M> {
    pub(super) fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            state: SqlState::new(),
            _m: PhantomData,
        }
    }

    pub fn where_eq<V: Serialize + Send + Sync + 'static,>(
        mut self,
        field: &str,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("{field} = ${key}"));
        self
    }

    pub async fn exec(self) -> Result<usize, ErrorIO> {
        let mut sql = format!("DELETE FROM {}", M::table_name());

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        let vals: Vec<surrealdb::Value> = res.take(0).map_err(ErrorIO::from)?;
        Ok(vals.len())
    }
}

