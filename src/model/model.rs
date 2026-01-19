use serde::de::DeserializeOwned;
use crate::{model::{delete::Delete, insert::Insert, query::Query, upsert::Update}, repository::Repo};
use surrealdb::sql::Thing;
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct QueryAst {
    pub table: String,
    pub selects: Vec<SelectItem>,
    pub joins: Vec<Join>,
    pub filters: Vec<Filter>,
    pub orders: Vec<Order>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
    pub sets: Vec<SetExpr>,   // UPDATE
}

#[derive(Debug, Clone)]
pub enum SelectItem {
    All,
    Column(String),
}
#[derive(Debug, Clone)]
pub enum Filter {
    Compare {
        column: String,
        op: CompareOp,
        value: ValueRef,
    },
    In {
        column: String,
        values: Vec<ValueRef>,
    },
    Or(Vec<Filter>),
    And(Vec<Filter>),
}
#[derive(Debug, Clone)]
pub enum CompareOp {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    Like,
}
#[derive(Debug, Clone)]
pub enum ValueRef {
    Bound(usize),      // index into bindings vector
    Literal(Value),
}
#[derive(Debug, Clone)]
pub struct Bindings {
    pub values: Vec<Value>,
}
#[derive(Debug, Clone)]
pub struct Join {
    pub table: String,
    pub on: (String, String), // (left, right)
    pub kind: JoinKind,
}
#[derive(Debug, Clone)]
pub enum JoinKind {
    Inner,
    Left,
}
#[derive(Debug, Clone)]
pub struct Order {
    pub column: String,
    pub direction: OrderDir,
}

#[derive(Debug, Clone)]
pub enum OrderDir {
    Asc,
    Desc,
}
#[derive(Debug, Clone)]
pub struct SetExpr {
    pub column: String,
    pub value: ValueRef,
} 
#[derive(Debug, Clone)]
pub struct InsertAst {
    pub table: String,
    pub columns: Vec<String>,
    pub values: Vec<ValueRef>,
}
#[derive(Debug, Clone)]
pub struct DeleteAst {
    pub table: String,
    pub filters: Vec<Filter>,
}
#[derive(Debug, Clone)]
pub enum Ast {
    Select(QueryAst),
    Insert(InsertAst),
    Update(QueryAst),
    Delete(DeleteAst),
}   



pub trait Model: Sized + DeserializeOwned {
    fn table_name() -> &'static str;
    fn query<'a>(repo: &'a Repo) -> Query<'a, Self> {
        Query::new(repo)
    }
    fn insert<'a>(repo: &'a Repo) -> Insert<'a, Self> {
        Insert::new(repo)
    }
    fn update<'a>(repo: &'a Repo) -> Update<'a, Self> {
        Update::new(repo)
    }
    fn delete<'a>(repo: &'a Repo) -> Delete<'a, Self> {
        Delete::new(repo)
    }
    fn id(&self) -> Thing ;
}