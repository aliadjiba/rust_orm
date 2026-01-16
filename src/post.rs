// use orm_macros::Model;
use serde::{Deserialize, Serialize};
use crate::{category::Category, model::{BelongsTo, BelongsToMany, HasParent, Model}, person::Person, repository::Repo}; //Relations, HasMany
use surrealdb::sql::Thing;

#[derive(Serialize,Deserialize,Debug)]
pub struct Post {
    pub id: Thing,
    pub title: String,
    pub person_id: Thing,
    // #[serde(skip)]
    // pub relations: Relations,
}

impl Model for Post {
    fn table_name() -> &'static str {
        "post"
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
impl HasParent<Person> for Post {
   fn parent_id(&self) -> &Thing {
        &self.person_id
    }
}
impl Post {
    pub fn person<'a>(&self, repo: &'a Repo) -> BelongsTo<'a, Post, Person> {
        BelongsTo::new(repo, "person_id", self.person_id.clone())
    }

    pub fn categories<'a>(&self, repo: &'a Repo) -> BelongsToMany<'a, Post, Category> {
        BelongsToMany::new(repo,self.id.clone())
    }
}