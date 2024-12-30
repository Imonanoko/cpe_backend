use actix_web::{App, HttpServer,web::Data};
use actix_cors::Cors;
// use actix_session::{SessionMiddleware, storage::RedisSessionStore};
use sqlx::mysql::MySqlPool;
mod api;
use api::login::login;
use api::create_user::create_user;
#[actix_web::main]
async fn main()-> Result<(), std::io::Error>{
    dotenv::dotenv().ok();
    let datacase_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set.");
    let db_pool = MySqlPool::connect(&datacase_url).await.expect("Failed to connect to the database.");
    let ip = std::env::var("IP").expect("IP must be set.");
    let port = std::env::var("PORT").expect("PORT must be set.");

    // let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL must be set.");
    // let redis_store = RedisSessionStore::new(&redis_url)
    //     .await
    //     .expect("Failed to connect to Redis.");
    println!("Starting server at http://{}:{}...", ip, port);
    HttpServer::new(move || {
        App::new()
        .wrap(
            Cors::default()
                .allowed_origin("http://140.128.101.24:8080") // 允許前端的域名
                .allowed_methods(vec!["GET", "POST", "OPTIONS"]) // 允許的方法
                .allowed_headers(vec!["Content-Type", "Authorization"]) // 允許的請求頭
                .supports_credentials() // 支持附帶 Cookie
        )
        // .wrap(SessionMiddleware::builder(redis_store.clone(), actix_web::cookie::Key::generate())
        //         .cookie_secure(false) // 本地測試時禁用 HTTPS 要求
        //         .cookie_http_only(true) // 確保 Cookie 不可被 JS 訪問
        //         .cookie_same_site(SameSite::None) // 允許跨域 Cookie
        //         .build(),
        //     )
        .app_data(Data::new(db_pool.clone()))
        .service(login)
        .service(create_user)//要創建新使用者在打開
    })
    .bind(format!("{}:{}",ip,port))?
    .run()
    .await
}
