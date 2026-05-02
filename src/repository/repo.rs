use std::sync::Arc;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use surrealdb::opt::auth::Root;
use super::errors::{Error as ErrorIO};


pub type DbClient = Arc<Surreal<Client>>;
#[derive(Clone)]
pub struct Repo { pub db: DbClient, }
impl Repo{
    pub async fn connect(url: &str, ns: &str, db: &str, user: &str, pass: &str)->Result<Self, ErrorIO>{
        let _db = Surreal::new::<Ws>(url).await?;
        let token = _db.signin(Root { username: user.into(), password: pass.into() }).await;
        match token {
            Ok(_)=>{
                let ns_db =_db.use_ns(ns).use_db(db)
                    .await;
                match ns_db {
                    Ok(_)=>Ok(Self { db: Arc::new(_db) }),
                    Err(e)=>{
                        dbg!(&e);
                        Err(ErrorIO::from(e))
                    }
                }
            },
            Err(e)=>{
                dbg!(&e);
                Err(ErrorIO::from(e))
            }
        }
    }
}

/*
use std::sync::Arc;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws};
use super::errors::{Error as ErrorIO, * };

pub type DbClient = Arc<Surreal<Client>>;
#[derive(Clone)]
enum Database {
    Surreal(DbClient),
    SQLite,
    MySQL,
    Postgress,
    None
}

#[derive(Clone)]
pub struct Repo {
    // pub db: DbClient,
    pub database:Database,
    pub url: String,
    pub ns: String,
    pub db_n: String,
    pub user: String,
    pub pass: String,
}

impl Repo{
    pub fn new(url: &str, ns: &str, db_n: &str, user: &str, pass: &str)->Self {
        Self{
            url:url.to_string(),
            database:Database::None,
            db_n:db_n.to_string(),
            ns:ns.to_string(),
            pass:pass.to_string(),
            user:user.to_string(),
        }
    }
    pub fn surreal(&self) -> Result<&Surreal<Client>, ErrorIO> {
        match &self.database {
            Database::Surreal(db) => Ok(db),
            _ => Err(ErrorIO::Db("Database Not Connected".into())),
        }
    }
    pub async fn connect(&self)->Result<Self, ErrorIO>{
        let client = Surreal::new::<Ws>(self.url.as_str()).await.map_err(ErrorIO::from)?;
        // client.connect::<Ws>(url).await.map_err(ErrorIO::from)?;
        client.signin(surrealdb::opt::auth::Root { username: self.user.as_str(), password: self.pass.as_str()})
            .await.map_err(ErrorIO::from)?;
        client.use_ns(self.ns.as_str()).use_db(self.db_n.as_str())
            .await.map_err(ErrorIO::from)?;
        Ok(Self { 
            database:Database::Surreal(Arc::new(client)),
            ..self.clone()
         })
    }
}

*/
