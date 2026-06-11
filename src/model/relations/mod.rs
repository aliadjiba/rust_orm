mod belongs_to;
mod has_many;
mod belongs_to_many;
// mod eager_load;
mod pivot;
pub mod relation_tree;
use std::{
    collections::HashMap,
    sync::{LazyLock, RwLock, RwLockReadGuard},
};
pub use belongs_to::*;
pub use belongs_to_many::*;
pub use has_many::*;
// pub use eager_load::*;
pub use pivot::*;
pub use relation_tree::*;


type ModelName = String;
type RelationName = String;

type RelationRegistry =
    HashMap<ModelName, HashMap<RelationName, Relation>>;

static RELATIONS: LazyLock<RwLock<RelationRegistry>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

#[derive(Clone,Debug)]
pub enum RelationType{
    HasMany,
    BelongsTo,
    BelongsToMany,
}

#[derive(Clone,Debug)]
pub struct Relation{
    pub relation_type:RelationType,
    pub pivot:Option<String>,
    pub child_table: Option<String>,
    pub pivot_left_key: Option<String>,
    pub pivot_right_key: Option<String>,
    pub is_left: Option<bool>,
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

    pub fn has_many(model: &str, relation_name: &str, child_table: &str) {
        Self::push(
            model.to_string(),
            relation_name.to_string(),
            Relation {
                relation_type: RelationType::HasMany,
                pivot: None,
                child_table: Some(child_table.to_string()),
                pivot_left_key: None,
                pivot_right_key: None,
                is_left: None,
            },
        );
    }

    pub fn belongs_to(model: &str, relation_name: &str, parent_table: &str) {
        Self::push(
            model.to_string(),
            relation_name.to_string(),
            Relation {
                relation_type: RelationType::BelongsTo,
                pivot: None,
                child_table: Some(parent_table.to_string()),
                pivot_left_key: None,
                pivot_right_key: None,
                is_left: None,
            },
        );
    }

    pub fn belongs_to_many(
        model: &str,
        relation_name: &str,
        child_table: &str,
        pivot_table: &str,
        pivot_left_key: &str,
        pivot_right_key: &str,
        is_left: bool,
    ) {
        Self::push(
            model.to_string(),
            relation_name.to_string(),
            Relation {
                relation_type: RelationType::BelongsToMany,
                pivot: Some(pivot_table.to_string()),
                child_table: Some(child_table.to_string()),
                pivot_left_key: Some(pivot_left_key.to_string()),
                pivot_right_key: Some(pivot_right_key.to_string()),
                is_left: Some(is_left),
            },
        );
    }
}