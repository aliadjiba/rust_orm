use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::SurrealValue;
// use async_trait::async_trait;
use std::marker::PhantomData;
use crate::{model::{Edge, Insert, Model, Page, SqlState}, repository::{ErrorIO, Repo}};

pub struct Query<'a, M, R = M> {
    repo: &'a Repo,
    state: SqlState,

    select: Option<Vec<String>>,
    order: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,

    table: String,

    group_by: Option<Vec<String>>,
    with: Option<Vec<String>>,

    _m: PhantomData<M>,
    _r: PhantomData<R>,
}


impl<'a, M, R> Clone for Query<'a, M, R>
where
    M: Model + SurrealValue,
    R: DeserializeOwned + SurrealValue,
{
        fn clone(&self) -> Self {
        Self {
            repo: self.repo,

            state: SqlState {
                where_and: self.state.where_and.clone(),
                bindings: vec![],
            },

            select: self.select.clone(),
            order: self.order.clone(),

            limit: self.limit,
            offset: self.offset,

            table: self.table.clone(),

            group_by: self.group_by.clone(),
            with: self.with.clone(),

            _m: PhantomData,
            _r: PhantomData,
        }
    }

}


impl<'a, M, R> Query<'a, M, R>
where
    M: Model + SurrealValue,
    R: DeserializeOwned + SurrealValue,
{
    pub fn new(repo:&'a Repo) -> Self {
        Self {
            repo: repo,
            state: SqlState::new(),

            select: None,
            order: None,
            limit: None,
            offset: None,

            table: M::table_name(),
            
            group_by: None,
            with: None,

            _m: PhantomData,
            _r: PhantomData,
        }
    }

    pub fn relate(
    &self,
    from_id: impl Into<String>,
    edge: &str,
    to_id: impl Into<String>,
) -> Edge<'a> {
    Edge::new(self.repo, from_id, edge, to_id)
}
}

impl<'a, M, R> Query<'a, M, R>
where
    M: Model + SurrealValue,
    R: DeserializeOwned + SurrealValue,
{

    pub fn from_table(mut self, table: &str) -> Self {
        self.table = table.to_string();
        self
    }
    pub fn select<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.select = Some(fields.into_iter().map(Into::into).collect());
        self
    }
        pub fn as_type<NR>(self) -> Query<'a, M, NR> {
        Query {
            repo: self.repo,

            state: self.state,

            select: self.select,
            order: self.order,

            limit: self.limit,
            offset: self.offset,

            table: self.table,

            group_by: self.group_by,
            with: self.with,

            _m: PhantomData,
            _r: PhantomData,
        }
    }

    pub fn where_eq<V: Serialize + SurrealValue+ Send + Sync + 'static,>(
        mut self,
        field: &str,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("{field} = ${key}"));
        self
    }
    pub fn by_id<V: Serialize + SurrealValue + Send + Sync + 'static,>(
        mut self,
        value: V,
    ) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("id = ${key}"));
        self
    }
    pub fn where_or_eq<I, S, V>(mut self, conditions: I) -> Self
    where
        I: IntoIterator<Item = (S, V)>,   // S instead of impl Into<String>
        S: Into<String>,                  // now we declare S separately
        V: Serialize + SurrealValue + Send + Sync + 'static,
    {
        let mut parts = Vec::new();
        for (field, value) in conditions {
            let key = self.state.bind(value);
            parts.push(format!("{} = ${}", field.into(), key));
        }
        if !parts.is_empty() {
            self.state.where_and.push(format!("({})", parts.join(" OR ")));
        }
        self
    }
    pub fn where_or_in<I, S, V>(mut self, conditions: I) -> Self
    where
        I: IntoIterator<Item = (S, Vec<V>)>, // S instead of impl Into<String>
        S: Into<String>,                      // S implements Into<String>
        V: Serialize + SurrealValue + Send + Sync + 'static,
    {
        let mut parts = Vec::new();
        for (field, values) in conditions {
            if !values.is_empty() {
                let key = self.state.bind(values);
                parts.push(format!("{} IN ${}", field.into(), key));
            }
        }
        if !parts.is_empty() {
            self.state.where_and.push(format!("({})", parts.join(" OR ")));
        }
        self
    }
    pub fn where_in<V: Serialize + Send + Sync + SurrealValue  + 'static,>(
        mut self,
        field: &str,
        values: Vec<V>,
    ) -> Self {
        let key = self.state.bind(values);
        self.state.where_and.push(format!("{field} IN ${key}"));
        self
    }
    pub fn order_by(mut self, column: &str, direction: &str) -> Self {
        self.order = Some(format!("{column} {direction}"));
        self
    }
    pub fn latest(self) -> Self {
        self.order_by("created_at", "DESC")
    }
    pub fn oldest(self) -> Self {
        self.order_by("created_at", "ASC")
    }
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }
    pub fn offset(mut self, n: u64) -> Self {
        self.offset = Some(n);
        self
    }

   pub async fn all(self) -> Result<Vec<R>, ErrorIO> {
    let mut select = self
        .select
        .map(|s| s.join(", "))
        .unwrap_or_else(|| "*".to_string());

    if let Some(with) = self.with {
        for w in with {
            select.push_str(
                &format!(" , {w}.* AS {w} ")
                // &format!(" , (<~{}.{w})[0] AS {} ", M::table_name(), w)
            );
        }
    }

    let mut sql =
        format!("SELECT {select} FROM {} ", self.table);

    if !self.state.where_and.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&self.state.where_and.join(" AND "));
    }

    if let Some(order) = self.order {
        sql.push_str(&format!(" ORDER BY {order}"));
    }

    if let Some(limit) = self.limit {
        sql.push_str(&format!(" LIMIT {limit}"));
    }

    if let Some(offset) = self.offset {
        sql.push_str(&format!(" START {offset}"));
    }

    if let Some(groups) = &self.group_by {
        sql.push_str(" GROUP BY ");
        sql.push_str(&groups.join(", "));
    }
    dbg!(&sql);
    let mut query = self.repo.db.query(sql);

    for (k, v) in self.state.bindings {
        query = query.bind((k, v));
    }

    let mut res =
        query.await.map_err(ErrorIO::from)?;

    let rows: Vec<R> =
        res.take(0).map_err(ErrorIO::from)?;

    Ok(rows)
}
    pub async fn count(self) -> Result<u64, ErrorIO> {
        let mut sql = format!("SELECT count() FROM {}", self.table);

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        let count: Option<u64> = res.take(0).map_err(ErrorIO::from)?;
        Ok(count.unwrap_or(0))
    }

    pub async fn exists(self) -> Result<bool, ErrorIO> {
        Ok(self.limit(1).count().await? > 0)
    }

    pub async fn sum(self, field: &str) -> Result<f64, ErrorIO> {
        let mut sql = format!("SELECT math::sum({field}) FROM {}", self.table);

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        let value: Option<f64> = res.take(0).map_err(ErrorIO::from)?;
        Ok(value.unwrap_or(0.0))
    }
    pub async fn avg(self, field: &str) -> Result<f64, ErrorIO> {
        let mut sql = format!("SELECT math::mean({field}) FROM {}", self.table);

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        let value: Option<f64> = res.take(0).map_err(ErrorIO::from)?;
        Ok(value.unwrap_or(0.0))
    }
    pub fn group_by<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.group_by = Some(fields.into_iter().map(Into::into).collect());
        self
    }
    pub async fn paginate(
        self,
        page: u64,
        per_page: u64,
    ) -> Result<Page<R>, ErrorIO> {
        let total = self.clone().count().await?;

        let offset =
            (page.saturating_sub(1)) * per_page;

        let data = self
            .limit(per_page)
            .offset(offset)
            .all()
            .await?;

        let total_pages =
            (total + per_page - 1) / per_page;

        Ok(Page {
            data,

            page,
            per_page,

            total,
            total_pages,
        })
    }
    pub fn insert(self) -> Insert<'a, M> {
        Insert::new(self.repo)
    }

    pub async fn values<V>(self, data: V) -> Result<M, ErrorIO>
    where
        V: Serialize + Send + SurrealValue + 'static,
    {
        self.insert().values(data).await
    }
    pub fn with(mut self, n: &str) -> Self {
        match self.with {
            Some(mut k)=>{
                k.push(n.into());
               self.with=Some(k)
            },
            None=>{
                self.with = Some(vec![n.into()]);
            }
        }
        self
    }
    pub async fn first(self) -> Result<Option<R>, ErrorIO> {
    let mut rows = self.limit(1).all().await?;
    Ok(rows.pop())
}
}