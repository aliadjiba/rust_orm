use serde::{Serialize, de::DeserializeOwned};
use async_trait::async_trait;
use std::marker::PhantomData;
use crate::{model::{Insert, Model, Page, SqlState}, repository::{DbClient, ErrorIO, Repo}};
use serde_json::Value;
//new query design
#[derive(Debug, Clone)]
pub enum Expr {
    Eq(String, Value),
    In(String, Vec<Value>),
    Or(Vec<Expr>),
    And(Vec<Expr>),
}

#[derive(Debug, Clone)]
pub enum SelectItem {
    All,
    Columns(Vec<String>),
}
#[derive(Debug, Clone)]
pub struct QueryAst {
    pub table: String,
    pub select: SelectItem,
    pub filter: Option<Expr>,
    pub order: Option<(String, OrderDir)>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub group_by: Option<Vec<String>>,
    pub bindings: Vec<(String, Value)>,
}
#[derive(Debug, Clone)]
pub enum OrderDir {
    Asc,
    Desc,
}
trait QueryCompiler {
    fn compile_select(&self, ast: &QueryAst) -> (String, Vec<Value>);
}
pub struct Query<'a, M> {
    repo: &'a Repo,
    ast: QueryAst,
    _m: PhantomData<M>,
}
impl<'a, M> Clone for Query<'a, M> {
    fn clone(&self) -> Self {
        Self {
            repo: self.repo,
            ast: self.ast.clone(), // make sure QueryAst implements Clone
            _m: PhantomData,
        }
    }
}

impl<'a, M: Model> Query<'a, M> {
    pub fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            ast: QueryAst {
                table: M::table_name().to_string(),
                select: SelectItem::All,
                filter: None,
                order: None,
                limit: None,
                offset: None,
                bindings: vec![],
                group_by: Some(vec![]),
            },
            _m: PhantomData,
        }
    }
    pub fn from_table(mut self, table: &str) -> Self {
        self.ast.table = table.to_string();
        self
    }
    pub fn select<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ast.select = SelectItem::Columns(
            fields.into_iter().map(Into::into).collect()
        );
        self
    }
    pub fn where_eq<V: Serialize>(mut self, field: &str, value: V) -> Self {
        let expr = Expr::Eq(
            field.to_string(),
            serde_json::to_value(value).unwrap(),
        );
    
        self.ast.filter = match self.ast.filter {
            Some(existing) => Some(Expr::And(vec![existing, expr])),
            None => Some(expr),
        };
    
        self
    }
    pub fn where_or_eq<I, S, V>(mut self, conditions: I) -> Self
    where
        I: IntoIterator<Item = (S, V)>,
        S: Into<String>,
        V: Serialize,
    {
        let exprs = conditions
            .into_iter()
            .map(|(f, v)| {
                Expr::Eq(
                    f.into(),
                    serde_json::to_value(v).unwrap(),
                )
            })
            .collect();

        let or_expr = Expr::Or(exprs);

        self.ast.filter = match self.ast.filter {
            Some(existing) => Some(Expr::And(vec![existing, or_expr])),
            None => Some(or_expr),
        };

        self
    }
    pub fn where_or_in<I, S, V>(mut self, conditions: I) -> Self
    where
        I: IntoIterator<Item = (S, Vec<V>)>,
        S: Into<String>,
        V: Serialize,
    {
        let exprs: Vec<Expr> = conditions
            .into_iter()
            .filter(|(_, v)| !v.is_empty())
            .map(|(field, values)| {
                Expr::In(
                    field.into(),
                    values
                        .into_iter()
                        .map(|v| serde_json::to_value(v).unwrap())
                        .collect(),
                )
            })
            .collect();

        if !exprs.is_empty() {
            let or_expr = Expr::Or(exprs);

            self.ast.filter = match self.ast.filter {
                Some(existing) => Some(Expr::And(vec![existing, or_expr])),
                None => Some(or_expr),
            };
        }

        self
    }
    pub fn where_in<V: Serialize>(mut self, field: &str, values: Vec<V>) -> Self {
        let vals = values
            .into_iter()
            .map(|v| serde_json::to_value(v).unwrap())
            .collect();
    
        let expr = Expr::In(field.to_string(), vals);
    
        self.ast.filter = match self.ast.filter {
            Some(existing) => Some(Expr::And(vec![existing, expr])),
            None => Some(expr),
        };
    
        self
    }
    pub fn order_by(mut self, column: &str, dir: OrderDir) -> Self {
        self.ast.order = Some((column.to_string(), dir));
        self
    }
    
    
    pub fn latest(mut self) -> Self {
        self.ast.order = Some(("created_at".into(), OrderDir::Desc));
        self
    }
    pub fn limit(mut self, n: u64) -> Self {
        self.ast.limit = Some(n);
        self
    }
    pub fn oldest(self) -> Self {
        self.order_by("created_at", OrderDir::Desc)
    }
    pub fn offset(mut self, n: u64) -> Self {
        self.ast.offset = Some(n);
        self
    }    
    pub async fn first(self) -> Result<Option<M>, ErrorIO> {
        let mut rows = self.limit(1).all().await?;
        Ok(rows.pop())
    }
    pub async fn exists(self) -> Result<bool, ErrorIO> {
        Ok(self.limit(1).count().await? > 0)
    }

    fn push_where(&self, sql: &mut String) {
        if let Some(filter) = &self.ast.filter {
            // Compile the Expr into SQL
            let mut bindings = vec![];
            let filter_sql = Self::compile_expr(filter, &mut bindings);
    
            // Append WHERE clause
            sql.push_str(" WHERE ");
            sql.push_str(&filter_sql);
    
            // Add bindings to AST's bindings
            // (optional: you can merge them into self.ast.bindings)
            // For example:
            // self.ast.bindings.extend(bindings);
        }
    }
    
    pub async fn aggregate<T>(self, expr: &str) -> Result<T, ErrorIO>
    where
        T: DeserializeOwned + Default,
    {
        // Build SQL string
        let mut sql = format!("SELECT {expr} FROM {}", self.ast.table);
    
        // Add filter conditions
        if let Some(filter) = &self.ast.filter {
            sql.push_str(" WHERE ");
            sql.push_str(&Self::compile_expr(filter, &mut self.ast.bindings.clone()));
        }
    
        // Create the query
        let mut query = self.repo.db.query(sql);
    
        // Bind the values
        for (k, v) in &self.ast.bindings {
            query = query.bind((k.clone(), v.clone()));
        }
    
        // Execute
        let mut res = query.await.map_err(ErrorIO::from)?;
    
        // Take first row, default if empty
        Ok(res.take::<Option<T>>(0).map_err(ErrorIO::from)?.unwrap_or_default())
    }
    pub async fn count(self) -> Result<u64, ErrorIO> {
        self.aggregate("count()").await
    }
    pub fn group_by<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.ast.group_by = Some(fields.into_iter().map(Into::into).collect());
        self
    }
    pub async fn execute<T>(&mut self, sql: impl Into<String>) -> Result<Vec<T>, ErrorIO>
    where
        T: DeserializeOwned,
    {
        let sql = sql.into();
        let mut res = self.repo.db.query(sql).await.map_err(ErrorIO::from)?;
        Ok(res.take(0).map_err(ErrorIO::from)?)
    }
    

    pub async fn all_as<T>(&mut self) -> Result<Vec<T>, ErrorIO>
    where
        T: DeserializeOwned,
    {
        let sql = self.build_select_sql();
        let mut res = self.repo.db.query(sql).await.map_err(ErrorIO::from)?;
        Ok(res.take(0).map_err(ErrorIO::from)?)
    }

    pub async fn all(&mut self) -> Result<Vec<M>, ErrorIO>
    where
        M: DeserializeOwned,
    {
        self.all_as::<M>().await
    }

    pub async fn min(self, field: &str) -> Result<f64, ErrorIO> {
        self.aggregate::<f64>(&format!("math::min({field})")).await
    }
    
    pub async fn max(self, field: &str) -> Result<f64, ErrorIO> {
        self.aggregate::<f64>(&format!("math::max({field})")).await
    }
    pub async fn sum(self, field: &str) -> Result<f64, ErrorIO> {
        self.aggregate::<f64>(&format!("math::sum({field})")).await
    }
    
    pub async fn avg(self, field: &str) -> Result<f64, ErrorIO> {
        self.aggregate::<f64>(&format!("math::mean({field})")).await
    }
    fn compile_expr(expr: &Expr, bindings: &mut Vec<(String, Value)>) -> String {
        match expr {
            Expr::Eq(field, value) => {
                let key = format!("b{}", bindings.len());
                bindings.push((key.clone(), value.clone()));
                format!("{} = ${}", field, key)
            }
            Expr::In(field, values) => {
                let key = format!("b{}", bindings.len());
                bindings.push((key.clone(), serde_json::Value::Array(values.clone())));
                format!("{} IN ${}", field, key)
            }
            Expr::And(exprs) => {
                let parts: Vec<String> = exprs.iter().map(|e| Self::compile_expr(e, bindings)).collect();
                format!("({})", parts.join(" AND "))
            }
            Expr::Or(exprs) => {
                let parts: Vec<String> = exprs.iter().map(|e| Self::compile_expr(e, bindings)).collect();
                format!("({})", parts.join(" OR "))
            }
        }
    }
    
    
    fn build_select_sql(&mut self) -> String {
        let select_clause = match &self.ast.select {
            SelectItem::All => "*".to_string(),
            SelectItem::Columns(cols) => cols.join(", "),
        };
    
        let mut sql = format!("SELECT {} FROM {}", select_clause, self.ast.table);
    
        // Compile WHERE with bindings
        if let Some(filter) = &self.ast.filter {
            sql.push_str(" WHERE ");
            sql.push_str(&Self::compile_expr(filter, &mut self.ast.bindings));
        }
    
        // ORDER BY
        if let Some((col, dir)) = &self.ast.order {
            let dir = match dir { OrderDir::Asc => "ASC", OrderDir::Desc => "DESC" };
            sql.push_str(&format!(" ORDER BY {} {}", col, dir));
        }
    
        // LIMIT / OFFSET
        if let Some(limit) = self.ast.limit { sql.push_str(&format!(" LIMIT {}", limit)); }
        if let Some(offset) = self.ast.offset { sql.push_str(&format!(" START {}", offset)); }
    
        // GROUP BY
        if let Some(groups) = &self.ast.group_by {
            if !groups.is_empty() {
                sql.push_str(" GROUP BY ");
                sql.push_str(&groups.join(", "));
            }
        }
    
        sql
    }
    // pub async fn paginate(&mut self, page: u64, per_page: u64) -> Result<Page<M>, ErrorIO>
    // where
    //     M: DeserializeOwned,
    // {
    //     let page = page.max(1);
    //     let total = self.count().await?;
    //     let offset = (page - 1) * per_page;
    
    //     self.limit(per_page).offset(offset);
    
    //     let data = self.all().await?;
    
    //     let total_pages = (total + per_page - 1) / per_page;
    
    //     Ok(Page {
    //         data,
    //         page,
    //         per_page,
    //         total,
    //         total_pages,
    //     })
    // }
    


}


impl<'a, M: Model> QueryLike for Query<'a, M> {
    type Model = M;
    fn with_query<F>(self, f: F) -> Self
    where
        F: FnOnce(Query<'_, M>) -> Query<'_, M>,
    {
        f(self)
    }
}


#[async_trait]
pub trait QueryLike: Sized {
    type Model: Model;

    fn with_query<F>(self, f: F) -> Self
    where
        F: FnOnce(Query<'_, Self::Model>) -> Query<'_, Self::Model>;

    async fn count(self) -> Result<u64, ErrorIO> {
        let q = self.with_query(|q| q);
        q.count().await
    }

    async fn exists(self) -> Result<bool, ErrorIO> {
        let q = self.with_query(|q| q);
        q.exists().await
    }

    async fn sum(self, field: &str) -> Result<f64, ErrorIO> {
        let q = self.with_query(|q| q);
        q.sum(field).await
    }

    async fn avg(self, field: &str) -> Result<f64, ErrorIO> {
        let q = self.with_query(|q| q);
        q.avg(field).await
    }
    fn where_eq<V>(self, field: &str, value: V) -> Self
    where
        V: Serialize,
    {
        self.with_query(|q| q.where_eq(field, value))
    }

    fn where_in<V>(self, field: &str, values: Vec<V>) -> Self
    where
        V: Serialize + Send + Sync + 'static,
    {
        self.with_query(|q| q.where_in(field, values))
    }

    fn limit(self, n: u64) -> Self {
        self.with_query(|q| q.limit(n))
    }

    fn latest(self) -> Self {
        self.with_query(|q| q.latest())
    }

    fn oldest(self) -> Self {
        self.with_query(|q| q.oldest())
    }

    fn group_by<I, S>(self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.with_query(|q| q.group_by(fields))
    }

    async fn paginate(
        self,
        page: u64,
        per_page: u64,
    ) -> Result<Page<Self::Model>, ErrorIO> {
        let q = self.with_query(|q| q);
        q.paginate(page, per_page).await
    }
}


// pub fn insert(self) -> Insert<'a, M> {
//     Insert::new(self.repo)
// }

// pub async fn values<V>(self, data: V) -> Result<M, ErrorIO>
// where
//     V: Serialize + Send + 'static,
// {
//     self.insert().values(data).await
// }