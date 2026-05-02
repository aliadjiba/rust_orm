mod belongs_to;
mod has_many;
mod belongs_to_many;
mod eager_load;
mod pivot;
use std::{
    collections::HashMap,
    sync::{LazyLock, RwLock, RwLockReadGuard},
};
pub use belongs_to::*;
pub use belongs_to_many::*;
pub use has_many::*;
pub use eager_load::*;
pub use pivot::*;


type ModelName = String;
type RelationName = String;

type RelationRegistry =
    HashMap<ModelName, HashMap<RelationName, Relation>>;

static RELATIONS: LazyLock<RwLock<RelationRegistry>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone)]
pub enum RelationType{
    HasMany,
    BelongsTo,
    BelongsToMany,
}

#[derive(Clone)]
pub struct Relation{
    pub relation_type:RelationType,
    // pub parent_id: String,
    // pub child_id: String,
    // pub parent_table: String,
    // pub child_table: String,
    pub pivot:Option<String>
}


impl Relation {
    pub fn read(
    ) -> RwLockReadGuard<'static, RelationRegistry> {
        RELATIONS.read().unwrap()
    }

    pub fn get(model: &str, relation: &str) -> Option<Relation> {
        RELATIONS
            .read()
            .unwrap()
            .get(model)
            .and_then(|relations| relations.get(relation))
            .cloned()
    }

    pub fn push(model: String, relation_name: String, relation: Relation) {
        let mut relations = RELATIONS.write().unwrap();

        relations
            .entry(model.to_string())
            .or_insert_with(HashMap::new)
            .insert(relation_name.to_string(), relation);
    }
}