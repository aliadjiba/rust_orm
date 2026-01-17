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

pub struct BelongsToMany<'a, Parent, Child> {
    repo: &'a Repo,
    pivot_table: String,
    parent_id: Thing,
    _p: PhantomData<Parent>,
    _c: PhantomData<Child>,
}

fn pivot_table<P: Model, C: Model>() -> String {
    // Alphabetical order: "category_post" instead of "post_category"
    let mut names = vec![P::table_name().to_lowercase(), C::table_name().to_lowercase()];
    names.sort();
    names.join("_")
}

impl<'a, Parent, Child> BelongsToMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
        pub fn new(
        repo: &'a Repo,
        id: Thing,
    ) -> Self {
        let pivot_table:String = pivot_table::<Parent, Child>();
        Self {
            repo,
            parent_id: id,
            pivot_table: pivot_table,
            _p: PhantomData,
            _c: PhantomData,
        }
    }
    /// Attach a child to the parent (insert into pivot table)
    pub async fn attach(&self, child: &impl Model) -> Result<(), ErrorIO> {
        self.repo.db
            .query(&format!(
                "INSERT INTO {} (parent, child) VALUES ($parent, $child)",
                self.pivot_table
            ))
            .bind(("parent", self.parent_id.clone()))  // parent_id column
            .bind(("child", child.id()))       // child_id column
            .await?;
        Ok(())
    }

    /// Detach a child from the parent (delete from pivot table)
    pub async fn detach(&self, child_id: &Thing) -> Result<(), ErrorIO> {
        self.repo.db
            .query(&format!(
                "DELETE FROM {} WHERE parent = $parent AND child = $child",
                 self.pivot_table
            ))
            .bind(("parent", self.parent_id.clone()))
            .bind(("child", child_id.clone()))
            .await?;
    Ok(())
    }

    pub async fn load(self) -> Result<Query<'a, Child>, ErrorIO> {
        let res = Query::<Child>::new(self.repo)
            .from_table(&self.pivot_table)
            .where_eq(
                "parent",
                self.parent_id.clone()
            )
            .all_as::<BelongsToManyType>()
            .await;
        match res {
            Ok(related_ids) => {
                let related_ids: Vec<Thing> = related_ids.into_iter().map(|r| r.child).collect();
                Ok(Query::new(self.repo).where_in("id", related_ids))
            },
            Err(e) => {
                println!("Error loading BelongsToMany: {:?}", e);
                Err(e)
            },
        }
        
    }
}
#[derive(Serialize,Deserialize,Debug)]
pub struct BelongsToManyType{
    id: Thing,
    parent: Thing,
    child: Thing,
}