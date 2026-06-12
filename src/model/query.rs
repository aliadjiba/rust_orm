use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use surrealdb::types::{RecordId, SurrealValue};
// use async_trait::async_trait;
use std::{marker::PhantomData};
use crate::{model::{Page, SqlState, model::Model, relations::{NestedRelation, Relation, RelationType}}, pluralizer, repository::Repo};
use crate::error::ErrorIO;

// fn quote_table(name: &str) -> String {
//     // Reserved words in SurrealDB that need escaping
//     const RESERVED: &[&str] = &["user", "order", "group", "select", "table"];
    
//     if RESERVED.contains(&name) {
//         format!("⟨{}⟩", name)  // SurrealDB uses ⟨⟩ not backticks
//     } else {
//         name.to_string()
//     }
// }
fn quote_table(name: &str) -> String {
    format!("{}", name)
}
// use super::relations::Relation;
#[derive(Debug)]
pub struct QueryBuilder<'a, M, S> {
    repo: &'a Repo,
    state: SqlState,
    value: Option<String>,
    select: Option<Vec<String>>,
    order: Option<String>,
    limit: Option<u64>,
    offset: Option<u64>,
    sets: Vec<String>,
    table: &'static str,

    group_by: Option<Vec<String>>,
    with: Option<Vec<String>>,
    nested: Option<Vec<NestedRelation>>,
    is_many: bool,
    data_json: Option<String>,

    _m: PhantomData<M>,
    _s: PhantomData<S>,
}

#[derive(Debug)]
pub struct Empty;
#[derive(Debug)]
pub struct Filtered;
#[derive(Debug)]
pub struct Filled;
// pub struct Ready;
#[derive(Debug)]
pub struct Delete<S = Empty>(PhantomData<S>);
#[derive(Debug)]
pub struct Select<S = Empty>(PhantomData<S>);
#[derive(Debug)]
pub struct Insert<S = Empty>(PhantomData<S>);
#[derive(Debug)]
pub struct Update<S = (Empty, Empty)>(PhantomData<S>);
#[allow(dead_code)]
#[derive(Debug)]
pub struct Upsert<S = (Empty, Empty)>(PhantomData<S>);




// Filterable: Select, Delete, Update
#[allow(dead_code)]
pub trait Filterable {}

impl Filterable for Delete<Empty> {}
impl Filterable for Delete<Filtered> {}

impl Filterable for Select<Empty> {}
impl Filterable for Select<Filtered> {}

impl Filterable for Update<(Empty,   Empty)> {}
impl Filterable for Update<(Empty,   Filtered)> {}
impl Filterable for Update<(Filled,  Empty)> {}
impl Filterable for Update<(Filled,  Filtered)> {}


// Fillable: Insert, Upsert, Update
pub trait Fillable {}

impl Fillable for Insert<Empty> {}
impl Fillable for Insert<Filled> {}
impl Fillable for Update<(Empty,   Empty)> {}
impl Fillable for Update<(Empty,   Filtered)> {}
impl Fillable for Update<(Filled,  Empty)> {}
impl Fillable for Update<(Filled,  Filtered)> {}


#[allow(dead_code)]
pub trait TransitionFiltered {
    type Output;
}

impl TransitionFiltered for Delete<Empty>            { type Output = Delete<Filtered>; }
impl TransitionFiltered for Delete<Filtered>         { type Output = Delete<Filtered>; }
impl TransitionFiltered for Select<Empty>            { type Output = Select<Filtered>; }
impl TransitionFiltered for Select<Filtered>         { type Output = Select<Filtered>; }

impl TransitionFiltered for Update<(Empty,  Empty)>  { type Output = Update<(Empty,  Filtered)>; }
impl TransitionFiltered for Update<(Empty,  Filtered)>{ type Output = Update<(Empty, Filtered)>; }

impl TransitionFiltered for Update<(Filled, Empty)>  { type Output = Update<(Filled, Filtered)>; }
impl TransitionFiltered for Update<(Filled, Filtered)>{ type Output = Update<(Filled,Filtered)>; }


pub trait TransitionFilled {
    type Output;
}

impl TransitionFilled for Insert<Empty>              { type Output = Insert<Filled>; }
impl TransitionFilled for Insert<Filled>             { type Output = Insert<Filled>; }

impl TransitionFilled for Update<(Empty,   Empty)>   { type Output = Update<(Filled, Empty)>; }
impl TransitionFilled for Update<(Empty,   Filtered)>{ type Output = Update<(Filled, Filtered)>; }

impl TransitionFilled for Update<(Filled,  Empty)>   { type Output = Update<(Filled, Empty)>; }
impl TransitionFilled for Update<(Filled,  Filtered)>{ type Output = Update<(Filled, Filtered)>; }




//transition
impl<'a, M, S> QueryBuilder<'a, M, S>
where
    M: Model,
{
    fn transition<T>(self) -> QueryBuilder<'a, M, T> {
        QueryBuilder {
            repo: self.repo,
            state: self.state,
            value:self.value,
            select: self.select,
            order: self.order,
            limit: self.limit,
            offset: self.offset,
            sets: self.sets,
            table: self.table,
            group_by: self.group_by,
            with: self.with,
            nested: self.nested,
            is_many: self.is_many,
            data_json: self.data_json.clone(),
            _m: PhantomData,
            _s: PhantomData,
        }
    }
}



impl<'a, M, S> QueryBuilder<'a, M, S>
where
    M: Model,
    S: Fillable + TransitionFilled,
{
    pub fn set<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        value: V,
    ) -> QueryBuilder<'a, M, S::Output> {
        let key = self.state.bind_value(field, value);
        self.sets.push(format!("{field} = ${key}"));
        self.transition()
    }

    pub fn values<V: Serialize + SurrealValue>(
        mut self,
        data: V,
    ) -> QueryBuilder<'a, M, S::Output> {
        self.state.bindings.push(("data".into(), data.into_value()));
        self.sets.push("CONTENT $data".into());
        self.transition()
    }

    pub fn values_many<V: Serialize + SurrealValue>(
        mut self,
        data: Vec<V>,
    ) -> QueryBuilder<'a, M, S::Output> {
        self.data_json = serde_json::to_string(&data).ok();
        self.state.bindings.push(("data".into(), data.into_value()));
        self.sets.push("CONTENT $data".into());
        self.is_many = true;
        self.transition()
    }
}


// Delete: only when Filtered
impl<'a, M> QueryBuilder<'a, M, Delete<Filtered>>
    where 
    M: Model
{ 
    pub async fn exec(self) -> Result<usize, ErrorIO>{
        let mut sql = format!("DELETE FROM {}", quote_table(M::table_name()));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        let vals: Vec<Value> = res.take(0).map_err(ErrorIO::from)?;
        Ok(vals.len())
    }
    pub fn filter<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        value: V,
    ) -> QueryBuilder<'a, M, Delete<Filtered>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} = ${key}"));
        self.transition()
    }
        pub fn where_<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        condition:&str,
        value: V,
    ) -> QueryBuilder<'a, M, Update<(Empty,Filtered)>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} {condition} ${key}"));
        self.transition()
    }
}

impl<'a, M> QueryBuilder<'a, M, Delete>
    where 
    M: Model
{ 
    pub fn find(mut self, value: RecordId) -> QueryBuilder<'a, M, Delete<Filtered>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("id = ${key}"));
        self.transition()
    }
    pub fn filter<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        value: V,
    ) -> QueryBuilder<'a, M, Delete<Filtered>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} = ${key}"));
        self.transition()
    }
        pub fn where_<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        condition:&str,
        value: V,
    ) -> QueryBuilder<'a, M, Update<(Empty,Filtered)>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} {condition} ${key}"));
        self.transition()
    }
}



pub trait SelectState {}
impl SelectState for Empty {}
impl SelectState for Filtered {}
pub struct Terminal;  // no SelectState impl — locks out chaining methods


// ── chaining methods: only on Empty/Filtered (not Terminal) ──────────────
impl<'a, M, S> QueryBuilder<'a, M, Select<S>>
where
    M: Model,
    S: SelectState,  // Empty | Filtered only
{
    pub fn field(mut self, fields: &'a str) -> Self
    {
        match self.select {
            Some(mut current)=>{
                current.push(fields.into());
                self.select = Some(current);
            },
            None=>{
                self.select = Some(vec![fields.into()]);
            }
        }
        self
    }
    pub fn fields(mut self, fields: &[&'static str]) -> Self
    {
        let mut select = Vec::new();
        for field in fields{
            select.push(field.to_string());
        }
        match self.select {
            Some(mut current)=>{
                current.extend(select);
                self.select = Some(current);
            },
            None=>{
                self.select = Some(select);
            }
        }
        self
    }

    pub fn with(mut self, n: &str) -> Self {
        if !n.is_empty() {
            if n.contains('.') {
                let nested = NestedRelation::parse_path(n);
                match self.nested {
                    Some(ref mut existing) => existing.extend(nested),
                    None => self.nested = Some(nested),
                }
            } else {
                match self.with {
                    Some(ref mut k) => k.push(n.into()),
                    None => self.with = Some(vec![n.into()]),
                }
            }
        }
        self
    }
     pub fn nested(mut self, path: &str) -> Self {
        let incoming = NestedRelation::parse_path(path);
        match self.nested {
            None => self.nested = Some(incoming),
            Some(ref mut existing) => {
                // Merge each incoming root into the existing tree
                for node in incoming {
                    NestedRelation::merge_into(existing, &[node.name.as_str()]);
                    // If the incoming node has children, graft them in too.
                    // parse_path already built the full subtree so we can just
                    // replace the leaf we just inserted with the full node.
                    if !node.children.is_empty() {
                        if let Some(target) = existing.iter_mut().find(|n| n.name == node.name) {
                            *target = node;
                        }
                    }
                }
            }
        }
        self
    }
    pub fn value(mut self, n: &str) -> Self {
        if !n.is_empty() {
            self.value=Some(n.into());
        }
        self
    }
    
    pub fn order_by(mut self, column: &str, direction: &str) -> Self {
        self.order = Some(format!("{column} {direction}"));
        self
    }

    pub fn latest(self) -> Self { self.order_by("created_at", "DESC") }
    pub fn oldest(self) -> Self { self.order_by("created_at", "ASC") }

    pub fn limit(mut self, n: u64) -> Self { self.limit = Some(n); self }
    pub fn offset(mut self, n: u64) -> Self { self.offset = Some(n); self }

    pub fn group_by<I, St>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = St>,
        St: Into<String>,
    {
        self.group_by = Some(fields.into_iter().map(Into::into).collect());
        self
    }

    pub fn filter<V: Serialize + SurrealValue>(
    mut self,
    field: &str,
    value: V,
    ) -> QueryBuilder<'a, M, Select<Filtered>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} = ${key}"));
        self.transition()
    }
    pub fn where_<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        op: &str,
        value: V,
    ) -> QueryBuilder<'a, M, Select<Filtered>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} {op} ${key}"));
        self.transition()
    }

    pub async fn paginate<R>(self, page: u64, per_page: u64) -> Result<Page<R>, ErrorIO>
    where
        R: DeserializeOwned + SurrealValue,
    {
        let total = self.clone().inner_count().await?;
        let offset = (page.saturating_sub(1)) * per_page;
        let data = self.limit(per_page).offset(offset).inner_all::<R>().await?;
        let total_pages = (total + per_page - 1) / per_page;
        Ok(Page { data, page, per_page, total, total_pages })
    }
    async fn inner_count(self) -> Result<u64, ErrorIO> {
        let mut sql = format!("SELECT count() FROM {}", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take::<Option<u64>>(0).map_err(ErrorIO::from)?.unwrap_or(0))
    }
    async fn inner_all<R>(self) -> Result<Vec<R>, ErrorIO>
    where
        R: DeserializeOwned + SurrealValue,
    {
        // Build the select clause
        let mut select = self
            .select
            .map(|s| s.join(", "))
            .unwrap_or_else(|| "*".to_string());

        
        let select_clause = if let Some(ref val) = self.value {
            let is_record = self.with
                .as_ref()
                .map(|w| w.iter().any(|w| w == val))
                .unwrap_or(false);
            if is_record {
                format!("VALUE {val}.*")
            } else {
                format!("VALUE {val}")
            }
    } else {
        // Collect which relation names are already handled by nested paths
        // so we don't double-emit them from self.with
        // nested_roots: all top-level relation names already handled by nested paths
        let nested_roots: std::collections::HashSet<String> = self.nested
            .as_ref()
            .map(|n| n.iter().map(|r| r.name.to_lowercase()).collect())
            .unwrap_or_default();
        // with() relations that aren't already covered by nested()
        if let Some(with) = self.with {
            for w in with {
                let w_lower = w.to_lowercase();

                // Skip if this relation is also present as a nested path
                // (nested block below will handle it with full FETCH chain)
                if nested_roots.contains(&w_lower) {
                    continue; // nested() already emits this
                }

                if let Some(rel) = Relation::get(M::table_name(), &w_lower) {
                    match rel.relation_type {
                        RelationType::BelongsToMany => {
                            let pivot_table = rel.pivot.clone().unwrap_or_default();
                            let pivot_left_key = rel.pivot_left_key.clone().unwrap_or_default();
                            let pivot_right_key = rel.pivot_right_key.clone().unwrap_or_default();
                            let (pivot_fk, pivot_child_key) = if rel.is_left.unwrap_or(true) {
                                (pivot_left_key.as_str(), pivot_right_key.as_str())
                            } else {
                                (pivot_right_key.as_str(), pivot_left_key.as_str())
                            };
                            select.push_str(&format!(
                                " , (SELECT * OMIT {pivot_fk} FROM {} WHERE {pivot_fk} = $parent.id FETCH {pivot_child_key}) AS {w} ",
                                quote_table(&pivot_table)
                            ));
                        }
                        RelationType::HasMany => {
                            let child_table = rel.child_table.clone().unwrap_or_else(|| {
                                pluralizer::singularize(&w)
                            });
                            select.push_str(&format!(
                                " , (SELECT * FROM {} WHERE {} = $parent.id) AS {w} ",
                                quote_table(&child_table),
                                M::table_name()
                            ));
                        }
                        RelationType::BelongsTo => {
                            // child_table holds the related table name (set by Relation::belongs_to)
                            let related_table = rel.child_table.clone().unwrap_or_else(|| w_lower.clone());
                            // fk is the field on Self that holds the RecordId
                            // stored in rel.fk by the macro (from `fk = "..."` or defaults to relation name)
                            let fk_col = rel.fk.clone().unwrap_or_else(|| w_lower.clone());
                            select.push_str(&format!(
                                " , (SELECT * FROM `{related_table}` WHERE id = $parent.{fk_col})[0] AS {w} "
                            ));
                        }
                    }
                } else if pluralizer::is_plural(&w_lower) {
                    let s = pluralizer::singularize(&w);
                    select.push_str(&format!(
                        " , (SELECT * FROM `{s}` WHERE {} = $parent.id) AS {w} ",
                        // quote_table(&s),
                        M::table_name()
                    ));
                } else {
                    // select.push_str(&format!(" , {w}.* "));
                    select.push_str(&format!(
                        " , (SELECT * FROM `{w_lower}` WHERE id = $parent.{w_lower})[0] AS {w} "
                    ));
                }
            }
        }

        if let Some(ref nested) = self.nested {
            select.push_str(&NestedRelation::generate_nested_sql(nested, M::table_name()));
        }

        select
    };
        let mut sql = format!("SELECT {select_clause} FROM {} ", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
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

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take(0).map_err(ErrorIO::from)?)
    }
}




// ── terminal methods: on ALL states including Terminal ────────────────────
pub trait AnySelectState {}
impl AnySelectState for Empty {}
impl AnySelectState for Filtered {}
impl AnySelectState for Terminal {}

impl<'a, M, S> QueryBuilder<'a, M, Select<S>>
where
    M: Model,
    S: AnySelectState,
{
    pub async fn all<R>(self) -> Result<Vec<R>, ErrorIO>
    where
        R: DeserializeOwned + SurrealValue,
    {
                // Build the select clause
        let mut select = self
            .select
            .map(|s| s.join(", "))
            .unwrap_or_else(|| "*".to_string());

        
        let select_clause = if let Some(ref val) = self.value {
            let is_record = self.with
                .as_ref()
                .map(|w| w.iter().any(|w| w == val))
                .unwrap_or(false);
            if is_record {
                format!("VALUE {val}.*")
            } else {
                format!("VALUE {val}")
            }
    } else {
        // Collect which relation names are already handled by nested paths
        // so we don't double-emit them from self.with
        // nested_roots: all top-level relation names already handled by nested paths
        let nested_roots: std::collections::HashSet<String> = self.nested
            .as_ref()
            .map(|n| n.iter().map(|r| r.name.to_lowercase()).collect())
            .unwrap_or_default();
        // with() relations that aren't already covered by nested()
        if let Some(with) = self.with {
            for w in with {
                let w_lower = w.to_lowercase();

                // Skip if this relation is also present as a nested path
                // (nested block below will handle it with full FETCH chain)
                if nested_roots.contains(&w_lower) {
                    continue; // nested() already emits this
                }

                if let Some(rel) = Relation::get(M::table_name(), &w_lower) {
                    match rel.relation_type {
                        RelationType::BelongsToMany => {
                            let pivot_table = rel.pivot.clone().unwrap_or_default();
                            let pivot_left_key = rel.pivot_left_key.clone().unwrap_or_default();
                            let pivot_right_key = rel.pivot_right_key.clone().unwrap_or_default();
                            let (pivot_fk, pivot_child_key) = if rel.is_left.unwrap_or(true) {
                                (pivot_left_key.as_str(), pivot_right_key.as_str())
                            } else {
                                (pivot_right_key.as_str(), pivot_left_key.as_str())
                            };
                            select.push_str(&format!(
                                " , (SELECT * OMIT {pivot_fk} FROM {} WHERE {pivot_fk} = $parent.id FETCH {pivot_child_key}) AS {w} ",
                                quote_table(&pivot_table)
                            ));
                        }
                        RelationType::HasMany => {
                            let child_table = rel.child_table.clone().unwrap_or_else(|| {
                                pluralizer::singularize(&w)
                            });
                            select.push_str(&format!(
                                " , (SELECT * FROM {} WHERE {} = $parent.id) AS {w} ",
                                quote_table(&child_table),
                                M::table_name()
                            ));
                        }
                        RelationType::BelongsTo => {
                            // child_table holds the related table name (set by Relation::belongs_to)
                            let related_table = rel.child_table.clone().unwrap_or_else(|| w_lower.clone());
                            // fk is the field on Self that holds the RecordId
                            // stored in rel.fk by the macro (from `fk = "..."` or defaults to relation name)
                            let fk_col = rel.fk.clone().unwrap_or_else(|| w_lower.clone());
                            select.push_str(&format!(
                                " , (SELECT * FROM `{related_table}` WHERE id = $parent.{fk_col})[0] AS {w} "
                            ));
                        }
                    }
                } else if pluralizer::is_plural(&w_lower) {
                    let s = pluralizer::singularize(&w);
                    select.push_str(&format!(
                        " , (SELECT * FROM `{s}` WHERE {} = $parent.id) AS {w} ",
                        // quote_table(&s),
                        M::table_name()
                    ));
                } else {
                    // select.push_str(&format!(" , {w}.* "));
                    select.push_str(&format!(
                        " , (SELECT * FROM `{w_lower}` WHERE id = $parent.{w_lower})[0] AS {w} "
                    ));
                }
            }
        }

        if let Some(ref nested) = self.nested {
            select.push_str(&NestedRelation::generate_nested_sql(nested, M::table_name()));
        }

        select
    };
        let mut sql = format!("SELECT {select_clause} FROM {} ", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
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
        // dbg!(Relation::read().get("article"));
        dbg!(&sql);
        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take(0).map_err(ErrorIO::from)?)
    }

   pub async fn first<R>(self) -> Result<Option<R>, ErrorIO>
    where
        R: DeserializeOwned + SurrealValue,
    {
        // build sql directly with LIMIT 1 instead of calling self.limit(1)
        let select = self.select.map(|s| s.join(", ")).unwrap_or_else(|| "*".to_string());
        let mut sql = format!("SELECT {select} FROM {} ", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }
        sql.push_str(" LIMIT 1");
        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take::<Vec<R>>(0).map_err(ErrorIO::from)?.pop())
    }

    pub async fn count(self) -> Result<u64, ErrorIO> {
        let mut sql = format!("SELECT VALUE count() FROM {}", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        let counts = res.take::<Vec<u64>>(0).map_err(ErrorIO::from)?;
        Ok(counts.into_iter().sum())
    }

    pub async fn exists(self) -> Result<bool, ErrorIO> {
        Ok(self.count().await? > 0)  // ← use count directly, avoid limit
    }

    pub async fn sum(self, field: &str) -> Result<f64, ErrorIO> {
        let mut sql = format!("SELECT math::sum({field}) FROM {}", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take::<Option<f64>>(0).map_err(ErrorIO::from)?.unwrap_or(0.0))
    }

    pub async fn avg(self, field: &str) -> Result<f64, ErrorIO> {
        let mut sql = format!("SELECT math::mean({field}) FROM {}", quote_table(self.table));

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take::<Option<f64>>(0).map_err(ErrorIO::from)?.unwrap_or(0.0))
    }

    pub async  fn find<R>(mut self, value: RecordId) -> Result<Option<R>,ErrorIO> 
    where 
    R:DeserializeOwned + SurrealValue
    {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("id = ${key}"));
        let last : QueryBuilder::<_, Select<Terminal>> = self.transition();
        last.first::<R>().await
    }
}






// Insert: only when Filled
impl<'a, M> QueryBuilder<'a, M, Insert<Filled>>
    where 
    M: Model
{
    pub async fn exec<R>(self) -> Result<R, ErrorIO>
    where
        R: DeserializeOwned + SurrealValue,
    {
        
        let sql = if self.is_many {
            let data_str = self.data_json.as_deref().unwrap_or("[]");
            format!("INSERT INTO {} {} RETURN *", quote_table(M::table_name()), data_str)
        } else {
            let mut sql = format!("CREATE {}", quote_table(M::table_name()));

            if let Some(content) = self.sets.iter().find(|s| s.starts_with("CONTENT")) {
                sql.push_str(&format!(" {content}"));
            } else if !self.sets.is_empty() {
                sql.push_str(" SET ");
                sql.push_str(&self.sets.join(", "));
            }

            let mut conditions = self.state.conditions.clone();
            if M::soft_delete() {
                conditions.push("deleted_at IS NULL".to_string());
            }
            if !conditions.is_empty() {
                sql.push_str(" WHERE ");
                sql.push_str(&conditions.join(" AND "));
            }

            sql.push_str(" RETURN *");
            sql
        };
        dbg!(&sql);
        // dbg!(&self.state.bindings);  // add this line
        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        // let val: surrealdb::types::Value = res.take(0).map_err(ErrorIO::from)?;
        // let json = val.into_json_value();
        // serde_json::from_value(json).map_err(|e| ErrorIO::Db(e.to_string()))
        let val: Option<R> = res.take(0).map_err(ErrorIO::from)?;
        val.ok_or_else(|| ErrorIO::Db("No record returned".to_string()))
    }
 }

// Update: only when BOTH filled and filtered
impl<'a, M> QueryBuilder<'a, M, Update<(Filled, Filtered)>>
    where 
    M: Model
{ 
     pub async fn exec<R>(self) -> Result<R, ErrorIO>
    where
        R: DeserializeOwned + SurrealValue,
    {
        let mut sql = format!("UPDATE {}", quote_table(M::table_name()));

        if let Some(content) = self.sets.iter().find(|s| s.starts_with("CONTENT")) {
            sql.push_str(&format!(" {content}"));
        } else if !self.sets.is_empty() {
            sql.push_str(" SET ");
            sql.push_str(&self.sets.join(", "));
        }

        let mut conditions = self.state.conditions.clone();
        if M::soft_delete() {
            conditions.push("deleted_at IS NULL".to_string());
        }
        if !conditions.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&conditions.join(" AND "));
        }

        sql.push_str(" RETURN *");

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        res.take::<Option<R>>(0)
            .map_err(ErrorIO::from)?
            .ok_or_else(|| ErrorIO::Db("Update failed".into()))
    }
}


impl<'a, M> QueryBuilder<'a, M, Update<(Empty, Empty)>>
    where 
    M: Model
{ 
    pub fn find(mut self, value: RecordId) -> QueryBuilder<'a, M, Update<(Empty,Filtered)>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("id = ${key}"));
        self.transition()
    }
    pub fn filter<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        value: V,
    ) -> QueryBuilder<'a, M, Update<(Empty,Filtered)>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} = ${key}"));
        self.transition()
    }
    pub fn where_<V: Serialize + SurrealValue>(
        mut self,
        field: &str,
        condition:&str,
        value: V,
    ) -> QueryBuilder<'a, M, Update<(Empty,Filtered)>> {
        let key = self.state.bind(value);
        self.state.conditions.push(format!("{field} {condition} ${key}"));
        self.transition()
    }
}




//clone
impl<'a, M, S> Clone for QueryBuilder<'a, M, S>
where
    M: Model,
{
        fn clone(&self) -> Self {
        Self {
            repo: self.repo,

            state: SqlState {
                conditions: self.state.conditions.clone(),
                bindings: vec![],
            },
            value:self.value.clone(),
            select: self.select.clone(),
            order: self.order.clone(),
            sets: self.sets.clone(),

            limit: self.limit,
            offset: self.offset,

            table: self.table,

            group_by: self.group_by.clone(),
            with: self.with.clone(),
            nested: self.nested.clone(),
            is_many: self.is_many,
            data_json: self.data_json.clone(),

            _m: PhantomData,
            _s: PhantomData,
        }
    }

}


// //new
impl<'a, M, S> QueryBuilder<'a, M, S>
where
    M: Model,
{
    pub fn new(repo: &'a Repo) -> Self {
        Self {
            repo: repo,

            state: SqlState::new(),
            value:None,
            select: None,
            order: None,
            limit: None,
            offset: None,
            sets: vec![],

            table: M::table_name(),
            
            group_by: None,
            with: None,
            nested: None,
            is_many: false,
            data_json: None,

            _m: PhantomData,
            _s: PhantomData,
        }
    }
}
