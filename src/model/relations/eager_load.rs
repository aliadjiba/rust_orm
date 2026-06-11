use std::{
    collections::HashMap,
    sync::Arc,
};
use surrealdb::types::{SurrealValue,RecordId};

use crate::{model::{HasMany, Model, query::{Filtered, QueryBuilder as Query, Select}}, repository::Repo};
use crate::error::ErrorIO;
use async_trait::async_trait;

#[async_trait]
pub trait EagerLoad<P>: Send + Sync {
    type Child: Model + SurrealValue + Send + Sync + 'static;

    async fn load(
        &self,
        parents: &[P],
        repo: &Repo,
    ) -> Result<HashMap<RecordId, Arc<Vec<Self::Child>>>, ErrorIO>;
}



#[async_trait]
impl<'a, Parent, Child> EagerLoad<Parent> for HasMany<'a, Child>
where
    Child: Model + HasParent<Parent> + SurrealValue + Send + Sync + 'static,
{
    type Child = Child;
    
    async fn load(
        &self,
        parents: &[Parent],
        repo: &Repo,
    ) -> Result<HashMap<RecordId, Arc<Vec<Self::Child>>>, ErrorIO> {
        let ids: Vec<RecordId> =
            parents.iter().map(|p| p.id()).collect();

        let children = Query::<Child,Select<Filtered>>::new(repo)
            .filter("parent_id", ids)
            .all::<Child>()
            .await?;

        let mut map: HashMap<RecordId, Vec<Child>> = HashMap::new();

        for child in children {
            map.entry(child.parent_id().clone())
                .or_insert_with(Vec::new)
                .push(child);
        }

        Ok(map
            .into_iter()
            .map(|(k, v)| (k, Arc::new(v)))
            .collect())
            }
}

pub trait HasParent<Parent: Model + SurrealValue> {
    fn parent_id(&self) -> &RecordId;
}