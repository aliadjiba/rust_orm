use serde::Serialize;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};
use crate::{model::{Model, query::{Query, QueryLike}}, repository::{ErrorIO, Repo}};

#[derive(Default)]
pub struct Relations {
    data: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl Relations {
    pub fn insert<T: 'static + Send + Sync>(&mut self, value: T) {
        self.data.insert(TypeId::of::<T>(), Box::new(value));
    }

    pub fn get<T: 'static>(&self) -> Option<&T> {
        self.data
            .get(&TypeId::of::<T>())
            .and_then(|v| v.downcast_ref())
    }
}

/* ===========================
   HAS MANY
=========================== */

impl<'a, Parent, Child> QueryLike for HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    type Model = Child;

    fn with_query<F>(mut self, f: F) -> Self
    where
        F: FnOnce(Query<'_, Child>) -> Query<'_, Child>,
    {
        self.query = f(self.query);
        self
    }
}

pub struct HasMany<'a, Parent, Child> {
    query: Query<'a, Child>,
    _p: PhantomData<Parent>,
}

impl<'a, Parent, Child> HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    pub fn new(repo: &'a Repo) -> Self {
        Self {
            query: Query::new(repo),
            _p: PhantomData,
        }
    }
    pub async fn all(self) -> Result<Vec<Child>, ErrorIO> {
        self.query.all().await
    }
    pub async fn first(self) -> Result<Option<Child>, ErrorIO> {
        self.query.first().await
    }
}


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





/* ===========================
   BELONGS TO
=========================== */

pub struct BelongsTo<'a, Parent, Child> {
    query: Query<'a, Parent>,
    _c: PhantomData<Child>,
}

impl<'a, Parent, Child> BelongsTo<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    pub fn new(
        repo: &'a Repo,
        owner_key: &str,
        owner_value: impl Serialize + Send + Sync + 'static,
    ) -> Self {
        Self {
            query: Query::new(repo).where_eq(owner_key, owner_value),
            _c: PhantomData,
        }
    }

    pub async fn one(self) -> Result<Option<Parent>, ErrorIO> {
        self.query.first().await
    }
}





/* ===========================
   BELONGS TO MANY
=========================== */

pub struct BelongsToMany<'a, Parent, Child> {
    repo: &'a Repo,
    parent_id: String,
    pivot_table: String,
    foreign_key: String,
    related_key: String,
    _p: PhantomData<Parent>,
    _c: PhantomData<Child>,
}

impl<'a, Parent, Child> BelongsToMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
        pub(super)  fn new(
        repo: &'a Repo,
        parent_id: impl ToString,
        pivot_table: impl ToString,
        foreign_key: impl ToString,
        related_key: impl ToString,
    ) -> Self {
        Self {
            repo,
            parent_id: parent_id.to_string(),
            pivot_table: pivot_table.to_string(),
            foreign_key: foreign_key.to_string(),
            related_key: related_key.to_string(),
            _p: PhantomData,
            _c: PhantomData, // ✅ FIX
        }
    }
    pub async fn load(self) -> Result<Query<'a, Child>, ErrorIO> {
        let related_ids: Vec<String> = Query::<Child>::new(self.repo)
            .select([self.related_key.clone()])
            .from_table(&self.pivot_table)
            .where_eq(&self.foreign_key, self.parent_id)
            .all_as::<String>()
            .await?;

        Ok(Query::new(self.repo).where_in("id", related_ids))
    }
}
