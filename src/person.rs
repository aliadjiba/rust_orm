use orm_macros::Model;
use serde::{Deserialize, Serialize};

use crate::model::Relations;


#[derive(Model,Serialize,Deserialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub relations: Relations,
}
