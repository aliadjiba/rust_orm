use std::sync::Arc;
use serde::de::DeserializeOwned;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use super::errors::{Error as ErrorIO};

#[async_trait::async_trait]
pub trait DbExecutor {
    async fn query<T: DeserializeOwned + Send>(
        &self,
        sql: &str,
        bindings: &[(String, serde_json::Value)],
    ) -> Result<Vec<T>, ErrorIO>;

    async fn execute_scalar<T: DeserializeOwned + Default + Send>(
        &self,
        sql: &str,
        bindings: &[(String, serde_json::Value)],
    ) -> Result<T, ErrorIO>;
}

pub type DbClient = Arc<Surreal<Client>>;
#[derive(Clone)]
pub struct Repo { pub db: DbClient, }
impl Repo{
    pub async fn connect(url: &str, ns: &str, db: &str, user: &str, pass: &str)->Result<Self, ErrorIO>{
        let client = Surreal::new::<Ws>(url).await.map_err(ErrorIO::from)?;
        // client.connect::<Ws>(url).await.map_err(ErrorIO::from)?;
        client.signin(surrealdb::opt::auth::Root { username: user, password: pass })
            .await.map_err(ErrorIO::from)?;
        client.use_ns(ns).use_db(db)
            .await.map_err(ErrorIO::from)?;
        Ok(Self { db: Arc::new(client) })
    }
}

#[async_trait::async_trait]
impl DbExecutor for DbClient {
    async fn query<T: DeserializeOwned + Send>(
        &self,
        sql: &str,
        bindings: &[(String, serde_json::Value)],
    ) -> Result<Vec<T>, ErrorIO> {
        let mut q = self.query(sql.to_string());
        for (k, v) in bindings {
            q = q.bind((k.clone(), v.clone()));
        }
        let mut res = q.await.map_err(ErrorIO::from)?;
        Ok(res.take(0).map_err(ErrorIO::from)?)
    }

    async fn execute_scalar<T: DeserializeOwned + Default + Send>(
        &self,
        sql: &str,
        bindings: &[(String, serde_json::Value)],
    ) -> Result<T, ErrorIO> {
        let mut q = self.query(sql.to_string());
        for (k, v) in bindings {
            q = q.bind((k.clone(), v.clone()));
        }
        let mut res = q.await.map_err(ErrorIO::from)?;
        Ok(res.take::<Option<T>>(0).map_err(ErrorIO::from)?.unwrap_or_default())
    }
}
