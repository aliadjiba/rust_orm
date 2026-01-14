use crate::{model::{BelongsTo, HasRelations, Person, Relations}, repository::Repo};
use super::Model;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

#[derive(Serialize, Deserialize)]
pub struct Post {
    pub id: Thing,
    pub title: String,
    pub content: String,
    pub person_id: Thing, // 🔑 foreign key
        #[serde(skip)]
    relations: Relations,
}

impl Model for Post {
    fn table_name() -> &'static str {
        "post"
    }
        fn relations(&self) -> &Relations {
        &self.relations
    }

    fn relations_mut(&mut self) -> &mut Relations {
        &mut self.relations
    }
}
impl HasRelations for Post {}

use std::any::{Any, TypeId};
use std::collections::HashMap;

impl Post {
    pub fn person<'a>(&self, repo: &'a Repo) -> BelongsTo<'a, Person, Post> {
        Post::belongs_to(
            repo,
            "id",
            self.person_id.clone(),
        )
    }
}
