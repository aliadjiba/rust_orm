use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut}, sync::Arc,
};
use surrealdb::sql::Thing;

use crate::{model::{Model, query::{Query, QueryLike}}, repository::{ErrorIO, Repo}};
use async_trait::async_trait;

#[async_trait]
pub trait EagerLoad<P>: Send + Sync {
    type Child: Model + Send + Sync + 'static;

    async fn load(
        &self,
        parents: &[P],
        repo: &Repo,
    ) -> Result<HashMap<String, Arc<Vec<Self::Child>>>, ErrorIO>;
}

pub trait IntoQuery<'a> {
    type Model: Model;
    fn into_query(self) -> Query<'a, Self::Model>;
}
impl<'a, P, C> IntoQuery<'a> for HasMany<'a, P, C>
where
    P: Model,
    C: Model,
{
    type Model = C;

    fn into_query(self) -> Query<'a, C> {
        self.query
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


#[async_trait]
impl<'a, Parent, Child> EagerLoad<Parent> for HasMany<'a, Parent, Child>
where
    Parent: Model + Send + Sync,
    Child: Model + HasParent<Parent> + Send + Sync + 'static,
{
    type Child = Child;

    async fn load(
        &self,
        parents: &[Parent],
        repo: &Repo,
    ) -> Result<HashMap<String, Arc<Vec<Self::Child>>>, ErrorIO> {
        let ids: Vec<String> =
            parents.iter().map(|p| p.id().to_string()).collect();

        let children = Query::<Child>::new(repo)
            .where_in("parent_id", ids)
            .all()
            .await?;

        let mut map: HashMap<String, Vec<Child>> = HashMap::new();

        for child in children {
            map.entry(child.parent_id().to_string())
                .or_insert_with(Vec::new)
                .push(child);
        }

        Ok(map
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect())
            }
}

pub trait HasParent<Parent: Model> {
    fn parent_id(&self) -> &Thing;
}
/* ===========================
   BELONGS TO
=========================== */

pub struct BelongsTo<'a, Parent, Child> {
    query: Query<'a, Child>, // ✅ QUERY CHILD
    _p: PhantomData<Parent>,
}
impl<'a, Parent, Child> BelongsTo<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    pub fn new(
        repo: &'a Repo,
        child_key: &str,
        child_value: impl Serialize + Send + Sync + 'static,
    ) -> Self {
        Self {
            query: Query::<Child>::new(repo)
                .where_eq(child_key, child_value),
            _p: PhantomData,
        }
    }

    pub async fn one(self) -> Result<Option<Child>, ErrorIO> {
        self.query.first().await
    }
}





/* ===========================
   BELONGS TO MANY
=========================== */

pub struct BelongsToMany<'a, P>
where
    P: Pivot,
{
    repo: &'a Repo,
    parent_id: Thing,
    _pivot: PhantomData<P>,
}
// pub struct BelongsToMany<'a, Parent, Child> {
//     repo: &'a Repo,
//     pivot_table: String,
//     parent_id: Thing,
//     _p: PhantomData<Parent>,
//     _c: PhantomData<Child>,
// }

fn pivot_table<P: Model, C: Model>() -> String {
    // Alphabetical order: "category_post" instead of "post_category"
    let mut names = vec![P::table_name().to_lowercase(), C::table_name().to_lowercase()];
    names.sort();
    names.join("_")
}

impl<'a, P> BelongsToMany<'a, P>
where
    P: Pivot,
{
       pub fn new(repo: &'a Repo, parent_id: Thing) -> Self {
        Self {
            repo,
            parent_id,
            _pivot: PhantomData,
        }
    }
    /// Attach a child to the parent (insert into pivot table)
    pub async fn attach(
        &self,
        child_id: Thing,
    ) -> Result<(), ErrorIO> {
        self.repo.db
            .query(&format!(
                "INSERT INTO {} ({}, {}) VALUES ($parent, $child)",
                P::table_name(),
                P::parent_key(),
                P::child_key(),
            ))
            .bind(("parent", self.parent_id.clone()))
            .bind(("child", child_id))
            .await?;

        Ok(())
    }

    /// Detach a child from the parent (delete from pivot table)
        pub async fn detach(
        &self,
        child_id: Thing,
    ) -> Result<(), ErrorIO> {
        self.repo.db
            .query(&format!(
                "DELETE FROM {} WHERE {} = $parent AND {} = $child",
                P::table_name(),
                P::parent_key(),
                P::child_key(),
            ))
            .bind(("parent", self.parent_id.clone()))
            .bind(("child", child_id))
            .await?;

        Ok(())
    }


        pub async fn load(
        &self,
    ) -> Result<Query<'a, P::Child>, ErrorIO> {
        let pivots: Vec<P> = Query::<P>::new(self.repo)
            .where_or_eq([ //where_eq(P::parent_key(), self.parent_id.clone())
                (P::parent_key(), self.parent_id.clone()),
                (P::child_key(), self.parent_id.clone())]
            )
            .all()
            .await?;

        let child_ids: Vec<Thing> =
            pivots.into_iter()
                  .map(|p| p.child_id().clone())
                  .collect();

        Ok(Query::<P::Child>::new(self.repo)
            .where_in("id", child_ids))
    }
    pub async fn sync(&self, child_ids: Vec<Thing>) -> Result<(), ErrorIO> {
        // Load existing children from pivot
        let existing: Vec<Thing> = Query::<P>::new(self.repo)
            .where_eq(P::parent_key(), self.parent_id.clone())
            .all()
            .await?
            .into_iter()
            .map(|p| p.child_id().clone())
            .collect();

        // Determine which to attach
        let to_attach: Vec<Thing> = child_ids
            .iter()
            .filter(|id| !existing.contains(id))
            .cloned()
            .collect();

        // Determine which to detach
        let to_detach: Vec<Thing> = existing
            .into_iter()
            .filter(|id| !child_ids.contains(id))
            .collect();

        // Attach new ones
        for id in to_attach {
            self.attach(id).await?;
        }

        // Detach missing ones
        for id in to_detach {
            self.detach(id).await?;
        }

        Ok(())
    }

    /// Sync children without detaching missing ones
    pub async fn sync_without_detach(&self, child_ids: Vec<Thing>) -> Result<(), ErrorIO> {
        // Load existing children
        let existing: Vec<Thing> = Query::<P>::new(self.repo)
            .where_eq(P::parent_key(), self.parent_id.clone())
            .all()
            .await?
            .into_iter()
            .map(|p| p.child_id().clone())
            .collect();

        // Attach only new ones
        let to_attach: Vec<Thing> = child_ids
            .iter()
            .filter(|id| !existing.contains(id))
            .cloned()
            .collect();

        for id in to_attach {
            self.attach(id).await?;
        }

        Ok(())
    }
}


#[derive(Serialize,Deserialize,Debug)]
pub struct BelongsToManyType{
    id: Thing,
    parent: Thing,
    child: Thing,
}
/* ===========================
   PIVOT
=========================== */
pub trait Pivot: Model {
    type Parent: Model;
    type Child: Model;

    fn parent_id(&self) -> &Thing;
    fn child_id(&self) -> &Thing;

    fn parent_key() -> &'static str {
        "parent"
    }

    fn child_key() -> &'static str {
        "child"
    }
    // fn new(parent: Thing, related: Thing) -> Self;
}

// pub trait Pivot: Model + Send + Sync + Clone {
//     /// Column name for parent id (example: "article_id")
//     fn parent_key() -> &'static str;

//     /// Column name for related id (example: "category_id")
//     fn related_key() -> &'static str;

//     /// Get parent id value
//     fn parent_id(&self) -> &Thing;

//     /// Get related id value
//     fn related_id(&self) -> &Thing;

//     /// Create a new pivot instance
//     fn new(parent: Thing, related: Thing) -> Self;
// }