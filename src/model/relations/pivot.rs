use surrealdb::sql::Thing;
use crate::model::Model;



/* ===========================
   PIVOT
=========================== */

pub trait Pivot: Model + Send + Sync + Clone {
        type Extra: Default+ Clone;
    fn left_key() -> &'static str;
    fn right_key() -> &'static str;
    // fn id(&self) -> &Thing;
    fn left_id(&self) -> &Thing;
    fn right_id(&self) -> &Thing;

    fn new_with(left: Thing, right: Thing, extra: Self::Extra) -> Self;
    fn new(left: Thing, right: Thing) -> Self {
        Self::new_with(left, right, Self::Extra::default())
    }
}