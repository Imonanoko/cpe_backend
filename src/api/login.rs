use actix_web::{post, web, HttpResponse, Responder};
use actix_session::Session;
use serde::Deserialize;
use sqlx::mysql::MySqlPool;
use sqlx::Error;
use bcrypt::verify;
use rand::Rng;
use sha2::{Digest, Sha256};
#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[post("/api/login")]
async fn login(
    db_pool: web::Data<MySqlPool>,
    login_data: web::Json<LoginRequest>,
    session: Session,
) -> impl Responder {
    let username = &login_data.username;
    let password = &login_data.password;

    match validate_user(db_pool.get_ref(), username, password).await {
        Ok(valid) if valid => {
            session.insert("username", username).unwrap();
            session.insert("is_logged_in", true).unwrap();

            // 生成 CSRF Token 並存入會話
            let csrf_token = generate_csrf_token();
            session.insert("csrf_token", &csrf_token).unwrap();
            HttpResponse::Ok()
                .insert_header(("X-CSRF-Token", csrf_token)) // 將 Token 放入回應頭
                .body("Login successful!")
        }
        Ok(_) => HttpResponse::Unauthorized().body("Invalid username or password."),
        Err(err) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().body("Internal server error.")
        }
    }
}

// 驗證用戶是否存在並匹配密碼
async fn validate_user(
    db_pool: &MySqlPool,
    username: &str,
    password: &str,
) -> Result<bool, Error> {
    let query = r#"
        SELECT password
        FROM users
        WHERE username = ?
    "#;
    let stored_hash: Option<String> = sqlx::query_scalar(query)
        .bind(username)
        .fetch_optional(db_pool)
        .await?;
    if let Some(stored_hash) = stored_hash {
        Ok(verify(password, &stored_hash).unwrap_or(false))
    } else {
        Ok(false)
    }
}

fn generate_csrf_token() -> String {
    let mut rng = rand::thread_rng();
    let random_bytes: [u8; 32] = rng.gen();
    let hash = Sha256::digest(&random_bytes);
    hex::encode(hash) // 將 CSRF Token 轉為十六進制字符串
}