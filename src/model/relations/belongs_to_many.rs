use serde::{Deserialize, Serialize};
use surrealdb::types::RecordId;
use std::fmt::Debug;
use std::marker::PhantomData;
use crate::error::ErrorIO;
use crate::model::query::{self, Delete, Filtered, Insert, Select};
use crate::{model::{Model, Pivot, query::QueryBuilder as Query}, repository::{Repo}};

/* ===========================
   BELONGS TO MANY
=========================== */

pub struct BelongsToMany<'a, P, Child, Parent>
where
    P: Pivot,
{
    repo: &'a Repo,
    parent_id: RecordId,
    is_left: bool, // 🔥 this decides direction
    _pivot: PhantomData<P>,
    _parent: PhantomData<Parent>, // Placeholder for potential future use
    _child: PhantomData<Child>, // Placeholder for potential future use
}


impl<'a, P, Child, Parent> BelongsToMany<'a, P, Child, Parent>
where
    P: Pivot ,
    Child: Model,
    Parent: Model ,
{
    pub fn new(repo: &'a Repo, parent_id: RecordId, is_left: bool) -> Self {
        Self {
            repo,
            parent_id,
            is_left,
            _pivot: PhantomData,
            _parent: PhantomData,
            _child: PhantomData,
        }
    }
}


impl<'a, P, Child, Parent> BelongsToMany<'a, P, Child, Parent>
where
    P: Pivot ,
    Child: Model,
    Parent: Model,
{
    /// Attach single relation
pub async fn attach(&self, related_id: RecordId) -> Result<(), ErrorIO> {
    let pivot = if self.is_left {
        P::new(self.parent_id.clone(), related_id)
    } else {
        P::new(related_id, self.parent_id.clone())
    };

    Query::<P,Insert>::new(self.repo)
        .values(pivot)
        .exec::<P>()
        .await?;

    Ok(())
}

pub async fn attach_with<F>(&self, related_id: RecordId, builder: F) -> Result<(), ErrorIO>
where
    F: FnOnce(P) -> P,
{
    let pivot = if self.is_left {
        P::new(self.parent_id.clone(), related_id)
    } else {
        P::new(related_id, self.parent_id.clone())
    };

    let pivot = builder(pivot);

    Query::<P,Insert>::new(self.repo)
        .values(pivot)
        .exec::<P>()
        .await?;

    Ok(())
}

}

impl<'a, P, Child, Parent> BelongsToMany<'a, P, Child, Parent>
where
    P: Pivot,
    Child: Model,
    Parent: Model,
{
    /// Detach relation
pub async fn detach(&self, related_id: RecordId) -> Result<usize, ErrorIO> {
    let init = Query::<P,Delete>::new(self.repo);
    let query:Query<'_, _, query::Delete<Filtered>>;
    if self.is_left {
        query = init
            .filter(P::left_key(), self.parent_id.clone())
            .filter(P::right_key(), related_id.clone());
    } else {
        query = init
            .filter(P::right_key(), self.parent_id.clone())
            .filter(P::left_key(), related_id.clone());
    }

   query.exec().await
}

    pub fn pivot(&self) -> Query<'a, P, query::Select<Filtered>>
    where
    {
        let query = Query::<P,Select>::new(self.repo);
        if self.is_left {
            query
                .filter(P::left_key(), self.parent_id.clone())
                .with(Child::table_name())
                .with(Parent::table_name())
        } else {
            query
                .filter(P::right_key(), self.parent_id.clone())
                .with(Child::table_name())
                .with(Parent::table_name())
        }
    }
    pub fn load(&self) -> Query<'a, P, query::Select<Filtered>>
    where
    {
        let query = Query::<P,Select>::new(self.repo);
        if self.is_left {
            query
                .filter(P::left_key(), self.parent_id.clone())
                .with(Child::table_name())
                .value(Child::table_name())

        } else {
            query
                .filter(P::right_key(), self.parent_id.clone())
                .with(Parent::table_name())
                .value(Parent::table_name())
        }
    }
}



#[derive(Serialize,Deserialize,Debug)]
pub struct BelongsToManyType{
    id: RecordId,
    parent: RecordId,
    child: RecordId,
}
