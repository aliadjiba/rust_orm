use serde::de::DeserializeOwned;
use crate::{model::{delete::Delete, insert::Insert, query::Query, upsert::Update}, repository::Repo};
use surrealdb::sql::Thing;
pub trait Model: Sized + DeserializeOwned {
    fn table_name() -> String;
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
    fn id(&self) -> Thing ;
}