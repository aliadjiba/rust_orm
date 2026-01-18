use std::sync::Arc;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use super::errors::{Error as ErrorIO};

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
