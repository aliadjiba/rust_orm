use crate::{model::{BelongsTo, HasRelations, Person}, repository::Repo};
use super::Model;
use serde::{Deserialize, Serialize};
use surrealdb::sql::Thing;

#[derive(Debug, Serialize, Deserialize,Clone)]
pub struct Post {
    pub id: Thing,
    pub title: String,
    pub content: String,
    pub person_id: Thing, // 🔑 foreign key
}

impl Model for Post {
    fn table_name() -> &'static str {
        "post"
    }
}
impl HasRelations for Post {}

impl Post {
    pub fn person<'a>(&self, repo: &'a Repo) -> BelongsTo<'a, Person, Post> {
        Post::belongs_to(
            repo,
            "id",
            self.person_id.clone(),
        )
    }
}
