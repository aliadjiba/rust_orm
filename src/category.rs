use serde::{Deserialize, Serialize};
use crate::{model::{HasMany, Model}, post::Post, repository::Repo};
use surrealdb::sql::Thing;

#[derive(Serialize,Deserialize,Debug)]
pub struct Category {
    pub id: Thing,
    pub name: String,
}

impl Model for Category {
    fn table_name() -> &'static str {
        "category"
    }
    fn id(&self) -> Thing {
        self.id.clone()
    }
}
impl Category {
    pub fn posts<'a>(&self, repo: &'a Repo) -> HasMany<'a, Category, Post> {
        HasMany::new(repo)
    }
}