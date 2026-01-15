use serde::Serialize;
use std::marker::PhantomData;
use crate::{model::Model, repository::{ErrorIO, Repo}};

pub struct Insert<'a, M> {
    repo: &'a Repo,
    _m: PhantomData<M>,
}

impl<'a, M: Model> Insert<'a, M> {
    pub(super) fn new(repo: &'a Repo) -> Self {
        Self { repo, _m: PhantomData }
    }

    pub async fn values<V>(self, data: V) -> Result<M, ErrorIO>
    where
        V: Serialize + Send + 'static,
    {
        let sql = format!("CREATE {} CONTENT $data", M::table_name());

        let mut res = self.repo.db
            .query(sql)
            .bind(("data", data))
            .await
            .map_err(ErrorIO::from)?;

        let mut records: Vec<M> = res.take(0).map_err(ErrorIO::from)?;
        records.pop().ok_or_else(|| ErrorIO::Db("Insert failed".into()))
    }
}
