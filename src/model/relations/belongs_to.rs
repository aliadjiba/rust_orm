use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::SurrealValue;
use std::marker::PhantomData;
use crate::{model::{Model, query::{Filtered, QueryBuilder as Query, Select}}, repository::Repo};
use crate::error::ErrorIO;

/* ===========================
   BELONGS TO
=========================== */

pub struct BelongsTo<'a, Parent> {
    query: Query<'a, Parent,Select<Filtered>>, // ✅ QUERY CHILD
    _p: PhantomData<Parent>,
}
impl<'a, Parent> BelongsTo<'a, Parent>
where
    Parent: Model+ Serialize+DeserializeOwned,
{
    pub fn new(
        repo: &'a Repo,
        child_value: impl Serialize +SurrealValue,
    ) -> Self {
        Self {
            query: Query::<Parent,Select>::new(&repo)
                .filter("id", child_value),
            _p: PhantomData,
        }
    }

    pub async fn one_as<R>(self) -> Result<Option<R>, ErrorIO> 
    where R:  DeserializeOwned + SurrealValue {
        self.query.first::<R>().await
    }
    pub async fn one<R>(self)-> Result<Option<R>, ErrorIO>
    where R:  DeserializeOwned + SurrealValue
    {
        self.query.first::<R>().await
    }
}


