// use orm_macros::Model;
use serde::{Deserialize, Serialize};
use crate::{category::Category, model::{BelongsTo, BelongsToMany, HasParent, Model}, person::Person, repository::Repo};
use surrealdb::sql::Thing;

#[derive(Serialize,Deserialize,Debug)]
pub struct Post {
    pub id: Thing,
    pub title: String,
    pub person_id: Thing,
}

impl Model for Post {
    fn table_name() -> &'static str {
        "post"
    }
    fn id(&self) -> Thing {
        self.id.clone()
    }
}
impl HasParent<Person> for Post {
   fn parent_id(&self) -> &Thing {
        &self.person_id
    }
}
impl Post {
    pub fn person<'a>(&self, repo: &'a Repo) -> BelongsTo<'a, Post, Person> {
        BelongsTo::new(
            repo,
            "id",                 // Person.id
            self.person_id.clone() // Post.person_id
        )
    }

    pub fn categories<'a>(&self, repo: &'a Repo) -> BelongsToMany<'a, Post, Category> {
        BelongsToMany::new(repo,self.id.clone())
    }
}