use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Deref, DerefMut}, sync::Arc,
};
use surrealdb::sql::Thing;

use crate::{model::{Model, query::Query}, repository::{ErrorIO, Repo}};
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

// impl<'a, Parent, Child> QueryLike for HasMany<'a, Parent, Child>
// where
//     Parent: Model,
//     Child: Model,
// {
//     type Model = Child;

//     fn with_query<F>(mut self, f: F) -> Self
//     where
//         F: FnOnce(Query<'_, Child>) -> Query<'_, Child>,
//     {
//         self.query = f(self.query);
//         self
//     }
// }

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


// pub struct BelongsToMany<'a, Parent, Child> {
//     repo: &'a Repo,
//     pivot_table: String,
//     parent_id: Thing,
//     _p: PhantomData<Parent>,
//     _c: PhantomData<Child>,
// }

// fn pivot_table<P: Model, C: Model>() -> String {
//     // Alphabetical order: "category_post" instead of "post_category"
//     let mut names = vec![P::table_name().to_lowercase(), C::table_name().to_lowercase()];
//     names.sort();
//     names.join("_")
// }
pub struct BelongsToMany<'a, P>
where
    P: Pivot,
{
    repo: &'a Repo,
    parent_id: Thing,
    is_left: bool, // 🔥 this decides direction
    _pivot: PhantomData<P>,
}


impl<'a, P> BelongsToMany<'a, P>
where
    P: Pivot + Serialize  + 'static,
{
    pub fn new(repo: &'a Repo, parent_id: Thing, is_left: bool) -> Self {
        Self {
            repo,
            parent_id,
            is_left,
            _pivot: PhantomData,
        }
    }

    /// Attach single relation
pub async fn attach(&self, related_id: Thing) -> Result<(), ErrorIO> {
    let pivot = if self.is_left {
        P::new(self.parent_id.clone(), related_id)
    } else {
        P::new(related_id, self.parent_id.clone())
    };

    Query::<P>::new(self.repo)
        .insert()
        .values(pivot)
        .await?;

    Ok(())
}

pub async fn attach_with<F>(&self, related_id: Thing, builder: F) -> Result<(), ErrorIO>
where
    F: FnOnce(P) -> P,
{
    let pivot = P::new(self.parent_id.clone(), related_id);
    let pivot = builder(pivot);

    Query::<P>::new(self.repo)
        .insert()
        .values(pivot)
        .await?;

    Ok(())
}

    /// Detach relation
pub async fn detach(&self, related_id: Thing) -> Result<(), ErrorIO> {
    let mut query = Query::<P>::new(self.repo);

    if self.is_left {
        query = query
            .where_eq(P::left_key(), self.parent_id.clone())
            .where_eq(P::right_key(), related_id);
    } else {
        query = query
            .where_eq(P::right_key(), self.parent_id.clone())
            .where_eq(P::left_key(), related_id);
    }

    let q =query.first().await?;
    match q  {
        Some(p)=>{P::delete(self.repo).by_id(p.id());},
        None=>{}
    }
    Ok(())
}


    /// Load related IDs
async fn existing_related_ids(&self) -> Result<Vec<Thing>, ErrorIO> {
    let pivots = if self.is_left {
        Query::<P>::new(self.repo)
            .where_eq(P::left_key(), self.parent_id.clone())
            .all()
            .await?
    } else {
        Query::<P>::new(self.repo)
            .where_eq(P::right_key(), self.parent_id.clone())
            .all()
            .await?
    };

    Ok(pivots
        .into_iter()
        .map(|p| {
            if self.is_left {
                p.right_id().clone()
            } else {
                p.left_id().clone()
            }
        })
        .collect())
}


    /// sync() — Laravel style
    pub async fn sync(&self, ids: Vec<Thing>) -> Result<(), ErrorIO> {
        let existing = self.existing_related_ids().await?;

        let to_attach: Vec<_> = ids
            .iter()
            .filter(|id| !existing.contains(id))
            .cloned()
            .collect();

        let to_detach: Vec<_> = existing
            .into_iter()
            .filter(|id| !ids.contains(id))
            .collect();

        for id in to_attach {
            self.attach(id).await?;
        }

        for id in to_detach {
            self.detach(id).await?;
        }

        Ok(())
    }

    /// sync_without_detach()
    pub async fn sync_without_detach(&self, ids: Vec<Thing>) -> Result<(), ErrorIO> {
        let existing = self.existing_related_ids().await?;

        let to_attach: Vec<_> = ids
            .into_iter()
            .filter(|id| !existing.contains(id))
            .collect();

        for id in to_attach {
            self.attach(id).await?;
        }

        Ok(())
    }
    pub async fn load<R>(&self) -> Result<Vec<R>, ErrorIO>
where
    R: Model + Clone,
{
    let pivots = if self.is_left {
        Query::<P>::new(self.repo)
            .where_eq(P::left_key(), self.parent_id.clone())
            .all()
            .await?
    } else {
        Query::<P>::new(self.repo)
            .where_eq(P::right_key(), self.parent_id.clone())
            .all()
            .await?
    };

    if pivots.is_empty() {
        return Ok(vec![]);
    }

    let related_ids: Vec<Thing> = pivots
        .into_iter()
        .map(|p| {
            if self.is_left {
                p.right_id().clone()
            } else {
                p.left_id().clone()
            }
        })
        .collect();

    let related = Query::<R>::new(self.repo)
        .where_in("id", related_ids)
        .all()
        .await?;

    Ok(related)
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
// pub trait Pivot: Model {
//     type Parent: Model;
//     type Child: Model;

//     fn parent_id(&self) -> &Thing;
//     fn child_id(&self) -> &Thing;

//     fn parent_key() -> &'static str {
//         "parent"
//     }

//     fn child_key() -> &'static str {
//         "child"
//     }
//     // fn new(parent: Thing, related: Thing) -> Self;
// }

pub trait Pivot: Model + Send + Sync + Clone {
    fn left_key() -> &'static str;
    fn right_key() -> &'static str;
    // fn id(&self) -> &Thing;
    fn left_id(&self) -> &Thing;
    fn right_id(&self) -> &Thing;

    fn new(left: Thing, right: Thing) -> Self;
}