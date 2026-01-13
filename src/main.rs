mod model;
mod repository;
use actix_web::App;
use actix_web::HttpServer;
use actix_web::Responder;
use actix_web::web;
use serde::Deserialize;
use serde_json::Value;
use crate::model::HasRelations;
use crate::model::Model;
use crate::model::Person;
use crate::repository::Repo;
use crate::repository::database_starter;
use surrealdb::sql::Thing;

use actix_web::{HttpResponse, get, post};

#[derive(serde::Serialize,Deserialize)]
struct NewPerson {
    pub name: String,
    pub address: String,
    pub phone: Vec<String>,
}

// #[post("/hello")] async fn hello(repo: web::Data<Repo>, payload: web::Json<Person>) -> impl Responder {
//     let data = payload.into_inner();
//     let res = Person::query(&repo)
//     .insert(NewPerson {
//         name: data.name,
//         address: data.address,
//         phone: data.phone,
//     })
//     .await;
//     match res {
//         Ok(result) => HttpResponse::Ok().json(result),
//         Err(e) => HttpResponse::InternalServerError() .body(format!("DB error: {:?}", e)),
//     }
// }
// #[get("/hellos")] async fn hellos(repo: web::Data<Repo>) -> impl Responder {
//     let res  = Person::query(&repo)
//     .all().await;
//     match res {
//         Ok(result) => HttpResponse::Ok().json(result),
//         Err(e) => HttpResponse::InternalServerError() .body(format!("DB error: {:?}", e)),
//     }
// }
#[derive(Deserialize,Debug)] 
struct PersonContact {
    id: Option<Thing>,
    address: String,
    phone: Vec<String>
}
#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // database_starter().await;
    let res = Repo::connect(
        "localhost:8000", // url
        "namespace", // ns
        "database", // db
        "root", // user
        "root", // pass
    ).await;

        match res {
        Ok(repo)=>{
            println!("connected");

            //CREATE
                // let _res = Person::insert(&repo)
                //     .values::<NewPerson>(NewPerson {
                //         name: "ali".to_string(),
                //         address: "ded@mail.co".to_string(),
                //         phone: vec!["06123123".to_string()],
                //     })
                //     .await;

            //SELECT *
                let res: Result<Vec<Person>, repository::ErrorIO>  = Person::query(&repo)
                .select(["id", "email","name","address","phone"])
                .all().await;
            //SELECT COSTUM
                // let res: Result<Vec<PersonContact>, repository::ErrorIO>  = Person::query(&repo)
                // .select(["id", "address","phone"])
                // .all_as::<PersonContact>().await;
            //SELECT WHERE
                // let res= Person::query(&repo).where_eq("phone", vec!["06123123".to_string()]).all().await; //.all_as::<PersonContact>()
                // print!("{:#?}",res.unwrap());
            //UPDATE
                // let res: Result<Person, repository::ErrorIO> = Person::update(&repo)
                // .set("name", "ded")
                // .where_eq("address", "ded@mail.co")
                // .update_as()
                // .await;
            //DELETE
            // let res = Person::delete(&repo)
            //     .where_eq("address", "ali@mail.co")
            //     .exec()
            //     .await;

            //SELECT ONE
                let res: Result<Option<Person>, repository::ErrorIO>  = Person::query(&repo)
                    .where_eq("address", "ded@mail.co")
                    .one()
                    .await;
                // let person=res.unwrap().unwrap();
            // let res = Person::has_many(&repo, "person_id", person.id.clone())
            //     .where_eq("active", true)
            //     .all()
            //     .await
            //     .expect("Failed to fetch contacts");
                print!("{:#?}",res);
             Ok(())
        },
        Err(e)=>{
            println!("couldn't connect {:?}",e);
            Ok(())
        }
    }
    // match res {
    //     Ok(repo)=>{
    //         println!("connected");
    //          HttpServer::new(move || { 
    //             App::new()
    //                 .app_data(web::Data::new(repo.clone()))
    //                 .route("/", web::get().to(|| async { "Hello from Actix + SurrealDB!" }))
    //                 .service(hellos)
    //                 .service(hello)
    //             })
    //             .bind(("127.0.0.1", 8080))?
    //             .run()
    //             .await
    //     },
    //     Err(e)=>{
    //         println!("couldn't connect {:?}",e);
    //         Ok(())
    //     }
    // }
}