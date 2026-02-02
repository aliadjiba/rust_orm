
use serde::{Serialize, de::DeserializeOwned};
use crate::{model::Model, repository::{ErrorIO, Repo}};

pub struct Edge<'a> {
    repo: &'a Repo,
    from: String,
    name: String,
    to: String,
    data: Option<serde_json::Value>,
}

impl<'a> Edge<'a> {

    pub fn new(
        repo: &'a Repo,
        from: impl Into<String>,
        name: impl Into<String>,
        to: impl Into<String>,
    ) -> Self {
        Self {
            repo,
            from: from.into(),
            name: name.into(),
            to: to.into(),
            data: None,
        }
    }

    pub fn set<V: Serialize>(mut self, data: V) -> Self {
        self.data = Some(serde_json::to_value(data).unwrap());
        self
    }

    pub async fn exec(self) -> Result<(), ErrorIO> {
        let sql = format!(
            "RELATE {} -> {} -> {}",
            self.from, self.name, self.to
        );

        let mut query = self.repo.db.query(sql);

        if let Some(data) = self.data {
            query = query.bind(("data", data));
            query = self.repo.db.query(format!(
                "RELATE {} -> {} -> {} CONTENT $data",
                self.from, self.name, self.to
            ));
        }

        query.await.map_err(ErrorIO::from)?;
        Ok(())
    }
    pub async fn neighbors<T>(
    &self,
    id: &str,
    edge: &str,
    ) -> Result<Vec<T>, ErrorIO>
    where
        T: DeserializeOwned+Model,
    {
        let sql = format!(
            "SELECT ->{}->{} FROM {}",
            edge,
            T::table_name(),
            id
        );

        let mut res = self.repo.db.query(sql)
            .await
            .map_err(ErrorIO::from)?;

        Ok(res.take(0).map_err(ErrorIO::from)?)
    }

}