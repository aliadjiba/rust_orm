use serde::Deserialize;
use serde::{de::DeserializeOwned, Serialize};
use crate::repository::{ErrorIO, Repo};
use erased_serde::Serialize as ErasedSerialize;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::marker::PhantomData;
pub struct CachedRelation<Child> {
    cache: std::collections::HashMap<String, Vec<Child>>,
}

impl<Child: Model> CachedRelation<Child> {
    pub fn get(&self, key: &str) -> Option<&Vec<Child>> {
        self.cache.get(key)
    }

    pub fn insert(&mut self, key: String, value: Vec<Child>) {
        self.cache.insert(key, value);
    }
}

pub struct EagerLoad<Parent, R> {
    parent: Parent,
    relation: R,
}

pub trait Relation<Parent> {
    type Child: Model;
    fn load(&self, parent_ids: Vec<String>, repo: &Repo) -> Query<'_, Self::Child>;
}

pub trait Model: Sized + DeserializeOwned {
    fn table_name() -> &'static str;
    fn relations(&self) -> &Relations;
    fn relations_mut(&mut self) -> &mut Relations;
    fn with<'a, R>(self, relation: R) -> EagerLoad<Self, R>
    where
        R: Relation<Self>,
    {
        EagerLoad {
            parent: self,
            relation,
        }
    }
    fn query<'a>(repo: &'a Repo) -> Query<'a, Self> {
        Query::new(repo)
    }

    fn insert<'a>(repo: &'a Repo) -> Insert<'a, Self> {
        Insert::new(repo)
    }

    fn update<'a>(repo: &'a Repo) -> Update<'a, Self> {
        Update::new(repo)
    }

    fn delete<'a>(repo: &'a Repo) -> Delete<'a, Self> {
        Delete::new(repo)
    }
}

struct SqlState {
    where_and: Vec<String>,
    where_or: Vec<String>,
    bindings: Vec<(String, Box<dyn ErasedSerialize + Send>)>,
}

impl SqlState {
    fn new() -> Self {
        Self {
            where_and: vec![],
            where_or: vec![],
            bindings: vec![],
        }
    }

    fn bind<V: Serialize + Send + 'static>(&mut self, value: V) -> String {
        let key = format!("v{}", self.bindings.len());
        self.bindings.push((key.clone(), Box::new(value)));
        key
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
pub struct Query<'a, M> {
    repo: &'a Repo,
    state: SqlState,
    select: Option<Vec<String>>,
    order: Option<String>,
    bindings: Vec<(String, Box<dyn erased_serde::Serialize + Send>)>,
    limit: Option<u64>,
    where_and: Vec<String>,
    where_or: Vec<String>,
    offset: Option<u64>,
    table_name_override: Option<String>,
    _m: PhantomData<M>,
}
impl<'a, M: Model> Query<'a, M> {
    fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            state: SqlState::new(),
            select: None,
            order: None,
            table_name_override: None,
            bindings:vec![],
            where_or: vec![],
            where_and:vec![],
            limit: None,
            offset: None,
            _m: PhantomData,
        }
    }
    pub async fn first(self) -> Result<Option<M>, ErrorIO> {
        self.limit(1)
            .all()
            .await
            .map(|mut v| v.pop())
    }
    pub fn order_by(mut self, column: &str, direction: &str) -> Self {
        self.order = Some(format!("{} {}", column, direction));
        self
    }
    pub fn latest(self) -> Self {
        self.order_by("created_at", "DESC")
    }

    pub fn oldest(self) -> Self {
        self.order_by("created_at", "ASC")
    }
    pub fn from(repo: &'a Repo) -> Self {
        Self::new(repo)
    }
    pub fn from_table(mut self, table_name: &str) -> Self {
        self.table_name_override = Some(table_name.to_string());
        self
    }
     pub fn where_in<V>(mut self, field: &str, values: Vec<V>) -> Self
    where
        V: Serialize + Send + 'static,
    {
        let key = format!("v{}", self.bindings.len());
        self.where_and.push(format!("{} IN ${}", field, key));
        self.bindings.push((key, Box::new(values) as Box<dyn erased_serde::Serialize + Send>));
        self
    }
    pub fn limit(mut self, n: u64) -> Self {
        self.limit = Some(n);
        self
    }
     pub async fn one(self) -> Result<Option<M>, ErrorIO> {
        let mut records = self.limit(1).all().await?;
        Ok(records.pop())
    }
    pub fn select<I, S>(mut self, fields: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.select = Some(fields.into_iter().map(Into::into).collect());
        self
    }
    pub fn where_eq<V: Serialize + Send + 'static>(mut self, field: &str, value: V) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("{field} = ${key}"));
        self
    }

    pub async fn all(self) -> Result<Vec<M>, ErrorIO> {
        self.all_as::<M>().await
    }

    pub async fn all_as<T>(self) -> Result<Vec<T>, ErrorIO>
    where
        T: DeserializeOwned,
    {
        let select = self
            .select
            .map(|s| s.join(", "))
            .unwrap_or_else(|| "*".to_string());

        let mut sql = format!("SELECT {} FROM {}", select, M::table_name());

        if !self.state.where_and.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&self.state.where_and.join(" AND "));
        }

        let mut query = self.repo.db.query(sql);
        for (k, v) in self.state.bindings {
            query = query.bind((k, v));
        }

        let mut res = query.await.map_err(ErrorIO::from)?;
        Ok(res.take(0).map_err(ErrorIO::from)?)
    }
}

pub struct Insert<'a, M> {
    repo: &'a Repo,
    _m: PhantomData<M>,
}

impl<'a, M: Model> Insert<'a, M> {
    fn new(repo: &'a Repo) -> Self {
        Self { repo, _m: PhantomData }
    }

    pub async fn values<S>(self, data: S) -> Result<M, ErrorIO>
    where
        S: Serialize + Send + 'static,
    {
        let sql = format!("CREATE {} CONTENT $data", M::table_name());
        let mut res = self.repo.db
            .query(sql)
            .bind(("data", data))
            .await
            .map_err(ErrorIO::from)?;

        let records: Vec<M> = res.take(0).map_err(ErrorIO::from)?;
        records.into_iter().next().ok_or_else(|| ErrorIO::Db("Insert failed".to_string()))
    }
}
pub struct Update<'a, M> {
    repo: &'a Repo,
    state: SqlState,
    sets: Vec<String>,
    _m: PhantomData<M>,
}

impl<'a, M: Model> Update<'a, M> {
    fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            state: SqlState::new(),
            sets: vec![],
            _m: PhantomData,
        }
    }

pub fn set<V: Serialize + Send + 'static>(mut self, field: &str, value: V) -> Self {
    let key = self.state.bind(value);
    self.sets.push(format!("{field} = ${key}")); // ✅ use '='
    self
}

    pub fn where_eq<V: Serialize + Send + 'static>(mut self, field: &str, value: V) -> Self {
        let key = self.state.bind(value);
        self.state.where_and.push(format!("{field} = ${key}"));
        self
    }
    pub async fn update_as<T>(self) -> Result<T, ErrorIO>
where
    T: DeserializeOwned,
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
    let record: Option<T> = res.take(0).map_err(ErrorIO::from)?;
    record.ok_or_else(|| ErrorIO::Db("Update failed".to_string()))
}


pub async fn upsert_as<T>(self) -> Result<T, ErrorIO>
where
    T: DeserializeOwned,
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
    let record: Option<T> = res.take(0).map_err(ErrorIO::from)?;
    record.ok_or_else(|| ErrorIO::Db("Upsert failed".to_string()))
}


}
pub struct Delete<'a, M> {
    repo: &'a Repo,
    state: SqlState,
    _m: PhantomData<M>,
}

impl<'a, M: Model> Delete<'a, M> {
    fn new(repo: &'a Repo) -> Self {
        Self {
            repo,
            state: SqlState::new(),
            _m: PhantomData,
        }
    }

    pub fn where_eq<V: Serialize + Send + 'static>(mut self, field: &str, value: V) -> Self {
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

pub trait Relationship: Model {
    fn table() -> &'static str {
        Self::table_name()
    }
}
pub struct HasMany<'a, Parent, Child> {
    query: Query<'a, Child>,
    _p: PhantomData<Parent>,
}
impl<'a, Parent, Child> HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model + for<'de> Deserialize<'de>,
{
    pub fn query(&mut self) -> &mut Query<'a, Child> {
    &mut self.query
    }
    pub fn where_eq<V>(mut self, field: &str, value: V) -> Self
    where
        V: Serialize + Send + 'static,
    {
        self.query = self.query.where_eq(field, value);
        self
    }

    pub fn limit(mut self, n: u64) -> Self {
        self.query = self.query.limit(n);
        self
    }

    pub async fn all(self) -> Result<Vec<Child>, ErrorIO> {
        self.query.all().await
    }

    pub async fn one(self) -> Result<Option<Child>, ErrorIO> {
        self.query.one().await
    }
}
pub struct BelongsTo<'a, Parent, Child> {
    query: Query<'a, Parent>,
    _c: PhantomData<Child>,
}

impl<'a, Parent, Child> BelongsTo<'a, Parent, Child>
where
    Parent: Model + for<'de> Deserialize<'de>,
    Child: Model,
{
    pub fn where_eq<V>(mut self, field: &str, value: V) -> Self
    where
        V: Serialize + Send + 'static,
    {
        self.query = self.query.where_eq(field, value);
        self
    }

    pub async fn one(self) -> Result<Option<Parent>, ErrorIO> {
        self.query.one().await
    }
}
pub trait HasRelations: Model {
    fn has_many<'a, Child>(
        repo: &'a Repo,
        foreign_key: &str,
    ) -> HasMany<'a, Self, Child>
    where
        Child: Model,
    {
        let query = Query::<Child>::new(repo);

        HasMany {
            query,
            _p: PhantomData,
        }
    }
    fn belongs_to<'a, Parent>(
        repo: &'a Repo,
        owner_key: &str,
        owner_value: impl Serialize + Send + 'static,
    ) -> BelongsTo<'a, Parent, Self>
    where
        Parent: Model,
    {
        let query = Query::<Parent>::new(repo)
            .where_eq(owner_key, owner_value);

        BelongsTo {
            query,
            _c: PhantomData,
        }
    }
}
pub struct BelongsToMany<'a, Parent, Child> {
    query: Query<'a, Child>,
    _p: PhantomData<Parent>,
}
impl<'a, Parent, Child> BelongsToMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    pub async fn new(
    repo: &'a Repo,
    parent_id: impl ToString,
    pivot_table: impl ToString,
    foreign_key: impl ToString,
    related_key: impl ToString,
) -> Result<Self, ErrorIO> {
    let parent_id = parent_id.to_string();
    let pivot_table = pivot_table.to_string();
    let foreign_key = foreign_key.to_string();
    let related_key = related_key.to_string();
    let related_ids: Vec<String> = Query::<Child>::new(repo)
        .select(vec![related_key.clone()])
        .from_table(&pivot_table)
        .where_eq(&foreign_key, parent_id.clone()) // <-- owned String works
        .all_as::<String>()
        .await?;
    let query = Query::<Child>::new(repo).where_in("id", related_ids);

    Ok(Self {
        query,
        _p: PhantomData,
    })
}
    pub fn query(&mut self) -> &mut Query<'a, Child> {
        &mut self.query
    }
    pub async fn all(self) -> Result<Vec<Child>, ErrorIO> {
        self.query.all().await
    }
    pub async fn first(self) -> Result<Option<Child>, ErrorIO> {
        self.query.limit(1).all().await.map(|mut v| v.pop())
    }
}


use std::ops::{Deref, DerefMut};

impl<'a, Parent, Child> Deref for HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    type Target = Query<'a, Child>;

    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'a, Parent, Child> DerefMut for HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}
#[derive(Default)]
pub struct Relations {
    data: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Relations {
    pub fn new() -> Self {
        Self { data: HashMap::new() }
    }

    pub fn insert<T: 'static + Send + Sync>(&mut self, value: T) {
        self.data.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref::<T>())
    }
}