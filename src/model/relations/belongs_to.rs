use serde::Serialize;
use surrealdb::types::SurrealValue;
use std::marker::PhantomData;
use crate::{model::{Model, query::Query}, repository::{ErrorIO, Repo}};


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
        child_value: impl Serialize + Send + SurrealValue + Sync + 'static,
    ) -> Self {
        Self {
            query: Query::<Child>::new(&repo)
                .where_eq(child_key, child_value),
            _p: PhantomData,
        }
    }

    pub async fn one(self) -> Result<Option<Child>, ErrorIO> {
        self.query.first().await
    }
}


