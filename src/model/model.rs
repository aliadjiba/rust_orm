use serde::{Serialize, de::DeserializeOwned};
use surrealdb::types::{RecordId, SurrealValue};
use crate::{error::ErrorIO, model::{Upsert, query::{Delete, Insert, QueryBuilder, Select, Update}}, repository::Repo};

pub trait Model: Sized + DeserializeOwned + Sync + SurrealValue + Serialize {
    fn table_name() -> &'static str;
    // fn query<'a>(repo: &'a Repo) ->surrealdb::method::Query<'a, surrealdb::engine::remote::ws::Client> {
    //     QueryBuilder::<Self, Query>::sql(repo, s)
    // }
    fn insert<'a>(repo: &'a Repo) -> QueryBuilder<'a, Self, Insert> {
        QueryBuilder::<Self, Insert>::new(repo)
    }
    fn upsert<'a>(repo: &'a Repo) -> QueryBuilder<'a, Self, Upsert> {
        QueryBuilder::<Self, Upsert>::new(repo)
    }
    fn select<'a>(repo: &'a Repo) -> QueryBuilder<'a, Self, Select> {
        QueryBuilder::<Self, Select>::new(repo)
    }
    fn update<'a>(repo: &'a Repo) -> QueryBuilder<'a, Self, Update> {
        QueryBuilder::<Self, Update>::new(repo)
    }
    fn destroy<'a>(repo: &'a Repo) -> QueryBuilder<'a, Self, Delete> {
        QueryBuilder::<Self, Delete>::new(repo)
    }
    fn save<'a>(self,repo: &'a Repo) -> impl Future<Output = Result<Self, ErrorIO>> {
        let query = QueryBuilder::<Self, Upsert>::new(repo);
        query.find(self.id()).values(self).exec::<Self>()
    }
    fn delete<'a>(self,repo: &'a Repo) -> impl Future<Output = Result<usize, ErrorIO>> {
        let query = QueryBuilder::<Self, Delete>::new(repo);
        query.find(self.id()).exec()
    }
    fn soft_delete() -> bool { false }

    fn id(&self) -> RecordId ;
    fn schema()-> String;
    fn check_no_dependents<'a>(repo: &'a Repo, id: &'a RecordId) ->  impl std::future::Future<Output = Result<(), ErrorIO>> + 'a;
}

