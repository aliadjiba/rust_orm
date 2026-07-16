use std::fmt::Debug;

use surrealdb::types::RecordId;

use crate::model::Model;



/* ===========================
   PIVOT
=========================== */

pub trait Pivot: Model + Send + Sync + Clone + Debug {
        type Extra: Default + Clone;
    fn left_key() -> &'static str;
    fn right_key() -> &'static str;
    // fn id(&self) -> &RecordId;
    fn left_id(&self) -> RecordId;
    fn right_id(&self) ->RecordId;

    fn new(left: RecordId, right: RecordId, extra: Self::Extra) -> Self;
    // fn new(left: RecordId, right: RecordId) -> Self {
    //     Self::new_with(left, right, Self::Extra::default())
    // }
}