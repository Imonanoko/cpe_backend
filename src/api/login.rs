use actix_web::{post, web, HttpResponse, Responder};
use serde::Deserialize;
use sqlx::mysql::MySqlPool;
use sqlx::Error;
use bcrypt::verify;
#[derive(Deserialize)]
pub struct LoginRequest {
    username: String,
    password: String,
}

#[post("/api/login")]
async fn login(
    db_pool: web::Data<MySqlPool>,
    login_data: web::Json<LoginRequest>,
) -> impl Responder {
    let username = &login_data.username;
    let password = &login_data.password;

    // 檢查用戶是否存在並驗證密碼
    match validate_user(db_pool.get_ref(), username, password).await {
        Ok(valid) if valid => HttpResponse::Ok().body("Login successful!"),
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



