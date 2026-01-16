// use orm_macros::Model;
use serde::{Deserialize, Serialize};
use crate::{model::{HasMany, Model}, post::Post, repository::Repo}; // //Relations
use surrealdb::sql::Thing;

#[derive(Serialize,Deserialize,Debug)]
pub struct Person {
    pub id: Thing,
    pub name: String,
    // #[serde(skip)]
    // pub relations: Relations,
}

impl Model for Person {
    fn table_name() -> &'static str {
        "person"
    }
    // fn relations(&self) -> &Relations {
    //     &self.relations
    // }
    // fn relations_mut(&mut self) -> &mut Relations {
    //     &mut self.relations
    // }
    fn id(&self) -> Thing {
        self.id.clone()
    }
    
}
impl Person {
    pub fn posts<'a>(&self, repo: &'a Repo) -> HasMany<'a, Person, Post> {
        HasMany::new(repo)
    }
}