use actix_web::{App, HttpServer,web::Data};
use sqlx::mysql::MySqlPool;
mod api;
use api::login::login;
#[actix_web::main]
async fn main()-> Result<(), std::io::Error>{
    dotenv::dotenv().ok();
    let datacase_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set.");
    let db_pool = MySqlPool::connect(&datacase_url).await.expect("Failed to connect to the database.");
    let ip = std::env::var("IP").expect("IP must be set.");
    let port = std::env::var("PORT").expect("PORT must be set.");
    println!("Starting server at http://{}:{}...", ip, port);
    HttpServer::new(move || {
        App::new()
        .app_data(Data::new(db_pool.clone()))
        .service(login)
    })
    .bind(format!("{}:{}",ip,port))?
    .run()
    .await
}
