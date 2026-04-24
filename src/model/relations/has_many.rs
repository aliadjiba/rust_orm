use std::{
    marker::PhantomData,
    ops::{Deref, DerefMut}
};
use crate::{model::{Model, query::Query}, repository::{ErrorIO, Repo}};


pub trait IntoQuery<'a> {
    type Model: Model;
    fn into_query(self) -> Query<'a, Self::Model>;
}
impl<'a, P, C> IntoQuery<'a> for HasMany<'a, P, C>
where
    P: Model,
    C: Model,
{
    type Model = C;

    fn into_query(self) -> Query<'a, C> {
        self.query
    }
}

pub struct HasMany<'a, Parent, Child> {
    query: Query<'a, Child>,
    _p: PhantomData<Parent>,
}

impl<'a, Parent, Child> HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    pub fn new(repo: &'a Repo) -> Self {
        Self {
            query: Query::new(repo),
            _p: PhantomData,
        }
    }
    pub async fn all(self) -> Result<Vec<Child>, ErrorIO> {
        self.query.all().await
    }
    pub async fn first(self) -> Result<Option<Child>, ErrorIO> {
        self.query.first().await
    }
}


impl<'a, Parent, Child> Deref for HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    type Target = Query<'a, Child>;
    fn deref(&self) -> &Self::Target {
        &self.query
    }
}

impl<'a, Parent, Child> DerefMut for HasMany<'a, Parent, Child>
where
    Parent: Model,
    Child: Model,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.query
    }
}
