use actix_cors::Cors;
use actix_session::{storage::RedisSessionStore, SessionMiddleware, config::PersistentSession};
use actix_web::{web::Data, App, HttpServer, cookie::{Key, time::Duration}};
use sqlx::mysql::MySqlPool;
mod api;
// use api::create_user::create_user;
use api::{
    login::login,
    check_session::check_session,
    query::{
        excel_search_pass::excel_search_pass,
        student_id_search::student_id_search,
        get_exam_session_info::get_exam_session_info,
        search_absent_and_excused::search_absent_and_excused,
        excel_search_absent::excel_search_absent,
    },
    create::{
        add_exam::add_exam,
        get_students_info_template::get_students_info_template,
        mutiple_add_student_info::mutiple_add_student_info,
        get_exam_score_template::get_exam_score_template,
        mutiple_add_exam_score::mutiple_add_exam_score,
        single_add_student::single_add_student,
        single_add_exam_score::single_add_exam_score,
    },
    modify::{
        modify_student_info::modify_student_info,
    }
};
#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();
    let datacase_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set.");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let ip = std::env::var("IP").expect("IP must be set.");
    let port = std::env::var("PORT").expect("PORT must be set.");
    let redis_store = RedisSessionStore::new(&redis_url)
        .await
        .expect("Failed to connect to Redis");
    let db_pool = MySqlPool::connect(&datacase_url)
        .await
        .expect("Failed to connect to the database.");
    println!("Starting server at http://{}:{}...", ip, port);
    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin("http://140.128.101.24:8080") // 允許前端的域名
                    .allowed_methods(vec!["GET", "POST", "OPTIONS"]) // 允許的方法
                    .allowed_headers(vec!["Content-Type", "Authorization", "X-CSRF-Token"]) // 允許的請求頭
                    .expose_headers(vec!["X-CSRF-Token"]) //沒有允許暴露的話前端是無法讀取的
                    .supports_credentials(), // 支持附帶 Cookie
            )
            .wrap(
                SessionMiddleware::builder(redis_store.clone(), get_secret_key())
                    .cookie_http_only(true)
                    .cookie_secure(false) //限制https
                    .cookie_same_site(actix_web::cookie::SameSite::Lax)
                    .session_lifecycle(
                        PersistentSession::default()
                            .session_ttl(Duration::seconds(3 * 60 * 60)) //配置session的TTL
                    )
                    .build(),
            )
            .app_data(Data::new(db_pool.clone()))
            .service(login)
            .service(check_session)
            .service(excel_search_pass)
            .service(student_id_search)
            .service(get_exam_session_info)
            .service(search_absent_and_excused)
            .service(excel_search_absent)
            .service(add_exam)
            .service(get_students_info_template)
            .service(mutiple_add_student_info)
            .service(get_exam_score_template)
            .service(mutiple_add_exam_score)
            .service(single_add_student)
            .service(single_add_exam_score)
            .service(modify_student_info)
            // .service(create_user) //要創建新使用者在打開
    })
    .bind(format!("{}:{}", ip, port))?
    .run()
    .await
}

fn get_secret_key() -> Key {
    let key = std::env::var("SESSION_SECRET_KEY").expect("SESSION_SECRET_KEY must be set");
    Key::from(key.as_bytes())
}
