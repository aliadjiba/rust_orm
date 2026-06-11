use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::SurrealValue;

use crate::{model::{Model, query::{Filtered, QueryBuilder as Query, Select}}, repository::Repo};
use crate::error::ErrorIO;


pub trait IntoQuery<'a> {
    type Model: Model;
    fn into_query(self) -> Query<'a, Self::Model, Select<Filtered>>;
}
impl<'a, C> IntoQuery<'a> for HasMany<'a, C>
where
    C: Model,
{
    type Model = C;

    fn into_query(self) -> Query<'a, C, Select<Filtered>> {
        self.query
    }
}

#[derive(Debug)]
pub struct HasMany<'a, Child> {
    query: Query<'a, Child, Select<Filtered>>,
}

impl<'a, Child> HasMany<'a, Child>
where
    Child: Model,
{
    pub fn new(repo: &'a Repo,field:&'a str,value:impl SurrealValue + Serialize) -> Self {
        Self {
            query: Query::<Child, Select<Filtered>>::new(repo).filter(field, value),
        }
    }
    pub async fn all<R>(self) -> Result<Vec<R>, ErrorIO>
    where 
        R: SurrealValue + DeserializeOwned
    {
        self.query.all::<R>().await
    }
    pub async fn first<R>(self) -> Result<Option<R>, ErrorIO>
    where 
    R: SurrealValue + DeserializeOwned
    {
        self.query.first::<R>().await
    }
}


// impl<'a, Parent, Child> Deref for HasMany<'a, Parent, Child>
// where
//     Parent: Model,
//     Child: Model,
// {
//     type Target = Query<'a, Child, Select<Filtered>>;
//     fn deref(&self) -> &Self::Target {
//         &self.query
//     }
// }

// impl<'a, Parent, Child> DerefMut for HasMany<'a, Parent, Child>
// where
//     Parent: Model,
//     Child: Model,
// {
//     fn deref_mut(&mut self) -> &mut Self::Target {
//         &mut self.query
//     }
// }
