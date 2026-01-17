// use orm_macros::Model;
use serde::{Deserialize, Serialize};
use crate::{model::{HasMany, Model}, post::Post, repository::Repo};
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
    
}
impl Person {
    pub fn posts<'a>(&self, repo: &'a Repo) -> HasMany<'a, Person, Post> {
        HasMany::new(repo)
    }
}