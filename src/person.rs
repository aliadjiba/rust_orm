// use orm_macros::Model;
use serde::{Deserialize, Serialize};
use crate::{model::{Delete, HasMany, Insert, Model, Query, Update}, post::Post, repository::Repo};
use surrealdb::sql::Thing;

#[derive(Serialize,Deserialize,Debug)]
pub struct Person {
    pub id: Thing,
    pub name: String,
}

impl Model for Person {
    fn table_name() -> &'static str {
        "person"
    }
    fn id(&self) -> Thing {
        self.id.clone()
    }
    fn query<'a>(repo: &'a Repo) -> Query<'a, Self> {
        Query::new(repo)
    }
    //     fn insert<'a>(repo: &'a Repo) -> Insert<'a, Self> {
    //     Insert::new(repo)
    // }
    // fn update<'a>(repo: &'a Repo) -> Update<'a, Self> {
    //     Update::new(repo)
    // }
    // fn delete<'a>(repo: &'a Repo) -> Delete<'a, Self> {
    //     Delete::new(repo)
    // }
    
}
impl Person {
    pub fn posts<'a>(&self, repo: &'a Repo) -> HasMany<'a, Person, Post> {
        HasMany::new(repo)
    }
}