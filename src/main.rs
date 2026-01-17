mod model;
mod person;
mod category;
mod post;
mod repository;
use serde::Serialize;
use crate::category::Category;
// use crate::model::HasRelations;
use crate::model::*;
use crate::person::Person;
use crate::post::Post;
// use crate::model::Person;
// use crate::model::Post;
use crate::repository::Repo;
use surrealdb::sql::Thing;
#[derive(Serialize)]
struct NewPerson{
    name:String
}

#[derive(Serialize)]
struct NewPost{
    title:String,
    person_id:Thing,
}
#[derive(Serialize)]
struct NewCategory{
    name:String,
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let res = Repo::connect(
        "localhost:8000", // url
        "namespace", // ns
        "database", // db
        "root", // user
        "root", // pass
    ).await;
    println!("hello am working");
    match res {
        Ok(repo)=>{
            println!("connected");
            //-------CREATE PERSON
                let person = Person::insert(&repo)
                    .values::<NewPerson>(NewPerson {
                        name: "ali".to_string(),
                    })
                    .await.unwrap();
                print!("THE PERSON: \n{:#?}",person);
            //-------CREATE POST
                let post = Post::insert(&repo)
                    .values::<NewPost>(NewPost {
                        title: "title of the post".to_string(),
                        person_id: person.id.clone(),
                    })
                    .await.unwrap();
                print!("THE POST: \n{:#?}",post);
            //-------EDIT PERSON
            let updated_person = Person::update(&repo)
                .where_eq("id", person.id.clone())
                .values(NewPerson {
                    name: "ali edited".to_string(),
                })
                .await
                .unwrap();

            println!("{:#?}", updated_person);
            //-------CREATE CATEGORY
                let category = Category::insert(&repo)
                    .values::<NewCategory>(NewCategory {
                        name: "Tech".to_string(),
                    })
                    .await.unwrap();
                print!("THE CATEGORY: \n{:#?}",category);
            //-------LINK CATEGORY TO POST
                    post.categories(&repo)
                    .attach(&category)
                    .await
                    .unwrap();
            //-------SELECT POSTS OF A PERSON
                let posts = person.posts(&repo).all().await;
                    print!("THE POSTS OF A PERSON: \n{:#?}",posts.unwrap());
            //-------SELECT PERSON OF A POST
                let person_of_post = post.person(&repo).one().await;
                    print!("THE PERSON OF A POST: \n{:#?}",person_of_post.unwrap());
            //-------SELECT POSTS OF A CATEGORY
                let posts_of_category = category.posts(&repo).all().await;
                    print!("THE POSTS OF A CATEGORY: \n{:#?}",posts_of_category.unwrap());
            //-------SELECT CATEGORIES OF A POST
                let categories_of_post = post.categories(&repo)
                    .load().await
                    .unwrap().all().await;
                    print!("THE CATEGORIES OF A POST: \n{:#?}",categories_of_post.unwrap());

        }
        Err(e)=>{
            println!("exit with error: {}",e);
        }
    }
    Ok(())
}



// #[derive(Serialize,Deserialize)]
// struct NewPerson {
//     pub name: String,
//     pub address: String,
//     pub phone: Vec<String>,
// }

// #[derive(Deserialize,Debug)] 
// struct PersonContact {
//     id: Option<Thing>,
//     address: String,
//     phone: Vec<String>
// }

// #[derive(Debug, Serialize, Deserialize)]
// pub struct NewPost {
//     pub title: String,
//     pub content: String,
//     pub person_id: Thing, // 🔑 foreign key
// }

// #[actix_web::main]
// async fn main() -> std::io::Result<()> {
//     // database_starter().await;
//     let res = Repo::connect(
//         "localhost:8000", // url
//         "namespace", // ns
//         "database", // db
//         "root", // user
//         "root", // pass
//     ).await;

//         match res {
//         Ok(repo)=>{
//             println!("connected");
//             // //-------CREATE PERSON
//                 let person = Person::insert(&repo)
//                     .values::<NewPerson>(NewPerson {
//                         name: "ali".to_string(),
//                         address: "ded@mail.co".to_string(),
//                         phone: vec!["06123123".to_string()],
//                     })
//                     .await.unwrap();

//             // //-------CREATE POST
//                 let post = Post::insert(&repo)
//                     .values::<NewPost>(NewPost {
//                         title: "Rust Lang ORM".to_string(),
//                         content: "Rust lang is a powerfull lang".to_string(),
//                         person_id: person.id.clone(),
//                     })
//                     .await.unwrap();
//                 //     print!("{:#?}",post);
//                 let post = Post::query(&repo)
//                         .latest()
//                         .limit(10)
//                         .all()
//                     .await.unwrap();
//                     // print!("{}",post);
//             //-------SELECT * POSTS OF A PERSON
//                 // let posts = person.posts(&repo).all().await;
//                 //     print!("{:#?}",posts);

//             //-------SELECT *
//                 // let res: Result<Vec<Person>, repository::ErrorIO>  = Person::query(&repo)
//                 // .select(["id", "email","name","address","phone"])
//                 // .all().await;

//             //-------SELECT COSTUM
//                 // let res: Result<Vec<PersonContact>, repository::ErrorIO>  = Person::query(&repo)
//                 // .select(["id", "address","phone"])
//                 // .all_as::<PersonContact>().await;

//             //-------SELECT WHERE
//                 // let res= Person::query(&repo).where_eq("phone", vec!["06123123".to_string()]).all().await; //.all_as::<PersonContact>()
//                 // print!("{:#?}",res.unwrap());

//             //-------UPDATE
//                 // let res: Result<Person, repository::ErrorIO> = Person::update(&repo)
//                 // .set("name", "ded")
//                 // .where_eq("address", "ded@mail.co")
//                 // .update_as()
//                 // .await;

//             //-------DELETE
//             // let res = Person::delete(&repo)
//             //     .where_eq("address", "ali@mail.co")
//             //     .exec()
//             //     .await;

//             //SELECT ONE
//                 // let res: Result<Option<Person>, repository::ErrorIO>  = Person::query(&repo)
//                 //     .where_eq("address", "ded@mail.co")
//                 //     .one()
//                 //     .await;

//                 // let person=res.unwrap().unwrap();
//             // let res = Person::has_many(&repo, "person_id", person.id.clone())
//             //     .where_eq("active", true)
//             //     .all()
//             //     .await
//             //     .expect("Failed to fetch contacts");
//                 // print!("{:#?}",res);
//              Ok(())
//         },
//         Err(e)=>{
//             println!("couldn't connect {:?}",e);
//             Ok(())
//         }
//     }

// }



//--------------------------------------------------------



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