use serde::de::DeserializeOwned;
use crate::{model::{delete::Delete, insert::Insert, query::Query, relations::Relations, upsert::Update}, repository::Repo};

pub trait Model: Sized + DeserializeOwned {
    fn table_name() -> &'static str;
    fn relations(&self) -> &Relations;
    fn relations_mut(&mut self) -> &mut Relations;

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
}


// pub trait HasRelations {}
/*

🔥 Next logical upgrades

If you want to go further:

JoinLike trait

Scope trait (active(), published())

#[derive(Model)]

Query caching per model

*/