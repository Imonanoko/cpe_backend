use actix_cors::Cors;
use actix_session::{storage::RedisSessionStore, SessionMiddleware, config::PersistentSession};
use actix_web::{web::Data, App, HttpServer, cookie::{Key, time::Duration}};
use sqlx::mysql::MySqlPool;
mod api;
use rustls::{Certificate, PrivateKey, ServerConfig};
use std::fs::File;
use std::io::BufReader;
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
        query_exam_info::query_exam_info,
        query_score_info::query_score_info,
        unclaimed_scholarship::unclaimed_scholarship,
        unclaimed_scholarship_json::unclaimed_scholarship_json,
        claimed_scholarship_json::claimed_scholarship_json,
        claimed_scholarship_excel::claimed_scholarship_excel,
    },
    create::{
        add_exam::add_exam,
        get_students_info_template::get_students_info_template,
        mutiple_add_student_info::mutiple_add_student_info,
        get_exam_score_template::get_exam_score_template,
        mutiple_add_exam_score::mutiple_add_exam_score,
        single_add_student::single_add_student,
        single_add_exam_score::single_add_exam_score,
        get_scholarship_template::get_scholarship_template,
        mutiple_add_scholarship::mutiple_add_scholarship,
    },
    modify::{
        modify_student_info::modify_student_info,
        modify_exam_info::modify_exam_info,
        modify_exam_score::modify_exam_score
    },
    delete::{
        delete_student_info::delete_student_info,
        delete_exam_info::delete_exam_info,
        delete_exam_score::delete_exam_score,
    }
};

/// 從指定路徑讀取憑證（.pem 格式）
fn load_certs(path: &str) -> Vec<Certificate> {
    let cert_file = File::open(path).expect("無法開啟憑證檔案");
    let mut reader = BufReader::new(cert_file);
    rustls_pemfile::certs(&mut reader)
        .expect("無法讀取憑證")
        .into_iter()
        .map(Certificate)
        .collect()
}

/// 從指定路徑讀取私鑰（.pem 格式）
fn load_private_key(path: &str) -> PrivateKey {
    let key_file = File::open(path).expect("無法開啟私鑰檔案");
    let mut reader = BufReader::new(key_file);
    let keys = rustls_pemfile::pkcs8_private_keys(&mut reader)
        .expect("無法讀取私鑰");
    if keys.is_empty() {
        panic!("找不到私鑰");
    }
    PrivateKey(keys[0].clone())
}
#[actix_web::main]
async fn main() -> Result<(), std::io::Error> {
    dotenv::dotenv().ok();
    let datacase_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set.");
    let redis_url = std::env::var("REDIS_URL").expect("REDIS_URL must be set");
    let ip = std::env::var("IP").expect("IP must be set.");
    let port = std::env::var("PORT").expect("PORT must be set.");
    let cert_path = std::env::var("CERT").expect("CERT_PATH must be set.");
    let key_path = std::env::var("KEY").expect("KEY_PATH must be set.");
    let redis_store = RedisSessionStore::new(&redis_url)
        .await
        .expect("Failed to connect to Redis");
    let db_pool = MySqlPool::connect(&datacase_url)
        .await
        .expect("Failed to connect to the database.");
    
    // 讀取證書與私鑰檔案（請確保 cert.pem 與 key.pem 存在）
    let certs = load_certs(&cert_path);
    let key = load_private_key(&key_path);

    // 建立 Rustls 的 ServerConfig
    let config = ServerConfig::builder()
        .with_safe_defaults()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .expect("建立 rustls config 失敗");

    println!("Server is running at https://{}:{}...", ip, port);
    HttpServer::new(move || {
        App::new()
            .wrap(
                Cors::default()
                    .allowed_origin("https://140.128.101.24:8080") // 允許前端的域名
                    .allowed_methods(vec!["GET", "POST", "OPTIONS"]) // 允許的方法
                    .allowed_headers(vec!["Content-Type", "Authorization", "X-CSRF-Token"]) // 允許的請求頭
                    .expose_headers(vec!["X-CSRF-Token"]) //沒有允許暴露的話前端是無法讀取的
                    .supports_credentials(), // 支持附帶 Cookie
            )
            .wrap(
                SessionMiddleware::builder(redis_store.clone(), get_secret_key())
                    .cookie_http_only(true)
                    .cookie_secure(true) //限制https
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
            .service(query_exam_info)
            .service(modify_exam_info)
            .service(query_score_info)
            .service(modify_exam_score)
            .service(delete_student_info)
            .service(delete_exam_info)
            .service(delete_exam_score)
            .service(unclaimed_scholarship)
            .service(get_scholarship_template)
            .service(mutiple_add_scholarship)
            .service(unclaimed_scholarship_json)
            .service(claimed_scholarship_json)
            .service(claimed_scholarship_excel)
            // .service(create_user) //要創建新使用者在打開
    })
    .bind_rustls(format!("{}:{}", ip, port), config)?
    .run()
    .await
}

fn get_secret_key() -> Key {
    let key = std::env::var("SESSION_SECRET_KEY").expect("SESSION_SECRET_KEY must be set");
    Key::from(key.as_bytes())
}
