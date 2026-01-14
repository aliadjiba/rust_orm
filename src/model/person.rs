use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use crate::{model::{HasMany, HasRelations, Post, Relations}, repository::Repo};
use super::Model;
use surrealdb::sql::Thing;

#[derive(Serialize, Deserialize)]
pub struct Person {
    pub id: Thing,
    pub name: String,
    pub address: String,
    pub phone: Vec<String>,
    #[serde(skip)]
    relations: Relations,
}
impl Clone for Person {
    fn clone(&self) -> Self {
        Self {
            id: self.id.clone(),
            name: self.name.clone(),
            address:self.address.clone(),
            phone:self.phone.clone(),
            relations: Relations::new(), // 💡 reset cache
        }
    }
}
impl Model for Person {
    fn table_name() -> &'static str {
        "person"
    }
    fn relations(&self) -> &Relations {
        &self.relations
    }

    fn relations_mut(&mut self) -> &mut Relations {
        &mut self.relations
    }
}
impl HasRelations for Person {}

impl Person {
    pub fn posts<'a>(&self, repo: &'a Repo) -> HasMany<'a, Person, Post> {
        Person::has_many::<Post>(repo, "person_id")
            .where_eq("person_id", self.id.clone())
    }
}