use std::{
    collections::HashMap,
    sync::Arc,
};
use surrealdb::sql::Thing;

use crate::{model::{HasMany, Model, query::Query}, repository::{ErrorIO, Repo}};
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