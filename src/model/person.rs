use serde::{Deserialize, Serialize};
use crate::{model::{HasMany, HasRelations, Post}, repository::Repo};
use super::Model;
use surrealdb::sql::Thing;

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct Person {
    pub id: Thing,
    pub name: String,
    pub address: String,
    pub phone: Vec<String>,
}

impl Model for Person {
    fn table_name() -> &'static str {
        "person"
    }
}
impl HasRelations for Person {}

impl Person {
    pub fn posts<'a>(&self, repo: &'a Repo) -> HasMany<'a, Person, Post> {
        Person::has_many::<Post>(repo, "person_id")
            .where_eq("person_id", self.id.clone())
    }
}