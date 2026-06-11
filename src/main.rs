use orm::{error::ErrorIO, model::{BelongsTo, BelongsToMany, HasMany, Model, Pivot}, repository::Repo};
use serde::{Deserialize, Serialize};
// use serde_json::Value;
use surrealdb::types::{RecordId, SurrealValue};

#[derive(Clone, Debug,Serialize,Deserialize,SurrealValue)]
struct Person {
    id: RecordId,
    name: String,
    address: String,
    phone: Vec<String>,
}

impl Model for Person {
    fn table_name() -> &'static str  {
        "person"
    }

    fn id(&self) -> RecordId {
        self.id.clone()
    }

    fn schema() -> String {
        r#"
        {
            "name": "string",
            "address": "string",
            "phone": ["string"]
        }
        "#.to_string()
    }
    
}

#[derive(Clone, Debug,Serialize,Deserialize,SurrealValue)]
struct Post {
    id: RecordId,
    title: String
}

impl Model for Post {
    fn table_name() -> &'static str  {
        "post"
    }

    fn id(&self) -> RecordId {
        self.id.clone()
    }

    fn schema() -> String {
        r#"
        {
            "title": "string",
        }
        "#.to_string()
    }
    
}

#[derive(Clone, Debug,Serialize,Deserialize,SurrealValue)]
struct Comment {
    id: RecordId,
    person:RecordId,
    content: String
}

impl Model for Comment {
    fn table_name() -> &'static str  {
        "comment"
    }

    fn id(&self) -> RecordId {
        self.id.clone()
    }

    fn schema() -> String {
        r#"
        {
            "content": "string",
            "person": "record<person>"

        }
        "#.to_string()
    }
    
}

impl Comment{
    async fn person<'a>(&self, repo: &'a Repo)->Result<Option<Person>, ErrorIO>
    {
        let rel: BelongsTo<'a, Person> = BelongsTo::new(&repo,self.person.clone());
        rel.one().await
    }
}
impl Person{
    async fn comments<'a>(&self, repo: &'a Repo)->Result<Vec<Comment>, ErrorIO>
    {
        let rel:HasMany<'a, Comment> = HasMany::new(&repo,"person",self.id.clone());
        rel.all().await
    }
}


#[derive(Clone, Debug,Serialize,Deserialize,SurrealValue)]
struct Role {
    id: RecordId,
    name: String,
}

impl Model for Role {
    fn table_name() -> &'static str  {
        "role"
    }

    fn id(&self) -> RecordId {
        self.id.clone()
    }

    fn schema() -> String {
        r#"
        {
            "name": "string",
        }
        "#.to_string()
    }
    
}
#[derive(Clone, Debug,Serialize,Deserialize,SurrealValue)]
struct PersonRolePivot {
    id: RecordId,
    person: RecordId,
    role:RecordId
}

impl Model for PersonRolePivot {
    fn table_name() -> &'static str {
        "person_role_pivot"
    }

    fn id(&self) -> RecordId {
        self.id.clone()
    }

    fn schema() -> String {
        r#"
        {
            "person": "record<person>",
            "role": "record<role>",
        }
        "#.to_string()
    }
    
}
impl Pivot for PersonRolePivot {
    type Extra = ();
    fn left_key<'a>() -> &'static str{
        Person::table_name()
    }
    fn right_key() -> &'static str{
        Role::table_name()
    }
    fn left_id(&self) -> RecordId{
        self.person.clone()
    }
    fn right_id(&self) -> RecordId{
        self.role.clone()
    }
    fn new(left: RecordId, right: RecordId) -> Self {
        Self::new_with(left, right, Self::Extra::default())
    }
    fn new_with(left: RecordId, right: RecordId, _extra: Self::Extra) -> Self{
        Self {
            id: RecordId{
                table: Self::table_name().into(),
                key: surrealdb::types::RecordIdKey::String(surrealdb::types::Uuid::new_v4().to_string())
            },
            person: left,
            role: right,
        }
    }
}



 impl Person
{
    pub fn roles<'a>(
        &self,
        repo: &'a Repo,
    ) -> BelongsToMany<'a, PersonRolePivot, Role, Person>
    {
        BelongsToMany::new(
            repo,
            self.id.clone(),
            true
        )
    }
}
 impl Role
{
    pub fn persons<'a>(
        &self,
        repo: &'a Repo,
    ) -> BelongsToMany<'a, PersonRolePivot, Role, Person>
    {
        BelongsToMany::new(
            repo,
            self.id.clone(),
            false
        )
    }
}











async fn sub_main<'a>(repo: &'a Repo)-> Result<(), ErrorIO> {
    let person = Person::insert(&repo).values(Person{
        id: RecordId::new("person", "1"),
        name: "John Doe".to_string(),
        address: "123 Main St".to_string(),
        phone: vec!["555-1234".to_string(), "555-5678".to_string()],
    }).exec::<Person>().await?;
    let person2 = Person::insert(&repo).values(Person{
        id: RecordId::new("person", "2"),
        name: "bahi med".to_string(),
        address: "alg".to_string(),
        phone: vec!["555-12034".to_string(), "55215-56780".to_string()],
    }).exec::<Person>().await?;
    dbg!(&person);
    let post = Post::insert(&repo).values(Post{
        id: RecordId::new("post", "1"),
        title:"The New Order".to_string()
    }).exec::<Post>().await?;
    dbg!(&post);
    let comment = Comment::insert(&repo).values(Comment{
        id: RecordId::new("post", "1"),
        person:person.id.clone(),
        content:"The New Order".to_string()
    }).exec::<Comment>().await?;
    let _comment2 = Comment::insert(&repo).values(Comment{
        id: RecordId::new("post", "2"),
        person:person2.id.clone(),
        content:"The New Order".to_string()
    }).exec::<Comment>().await?;

    dbg!(&comment);
    dbg!(&comment.person(&repo).await);
    dbg!(&person.comments(&repo).await);

    let role = Role::insert(&repo).values(Role{
        id: RecordId::new("role", "1"),
        name:"admin".to_string()
    }).exec::<Role>().await?;
    dbg!(&role);
    let _ = person.roles(&repo).attach(role.id.clone()).await;
    dbg!(&person.roles(&repo).load().all::<Role>().await);
    dbg!(&role.persons(&repo).load().all::<Person>().await);


    Ok(())
}

// fn d()->impl Future<Output = Result<Person, ErrorIO>>{
//     todo!()
// }

#[tokio::main]
async fn main() -> Result<(),ErrorIO> {
    let repo = Repo::connect(
        "127.0.0.1:8000", // url
        "namespace", // ns
        "database", // db
        "root", // user
        "root", // pass
    ).await;
    return match repo {
        Ok(repo) => {
            sub_main(&repo).await
                
        },
        Err(e) => {
            eprintln!("Failed to connect to the database: {}", e);
            Ok(())
        }
    };
}