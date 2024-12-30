use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use bcrypt::{hash, DEFAULT_COST};
use serde::Deserialize;
use sqlx::mysql::MySqlPool;
use sqlx::Error;
//使用curl創建，請先將server ip 綁定到127.0.0.1才能使用
// curl -X POST http://localhost:8888/api/users \
// -H "Content-Type: application/json" \
// -d '{"username": "newuser", "password": "securepassword"}'
// 定義接收的用戶請求結構
#[derive(Deserialize)]
pub struct CreateUserRequest {
    username: String,
    password: String,
}

// 創建用戶的 API
#[post("/api/users")]
async fn create_user(
    db_pool: web::Data<MySqlPool>,
    user_data: web::Json<CreateUserRequest>,
    req: HttpRequest,
) -> impl Responder {
    if !is_request_from_localhost(&req) {
        return HttpResponse::Forbidden().body("Access denied: Only localhost is allowed to create users.");
    }

    let username = &user_data.username;
    let password = &user_data.password;

    match add_user_to_db(db_pool.get_ref(), username, password).await {
        Ok(_) => HttpResponse::Ok().body("User created successfully."),
        Err(sqlx::Error::Database(err)) if err.constraint() == Some("users.username") => {
            HttpResponse::Conflict().body("Username already exists.")
        }
        Err(err) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().body("Internal server error.")
        }
    }
}

// 將新用戶添加到資料庫
async fn add_user_to_db(
    db_pool: &MySqlPool,
    username: &str,
    plain_password: &str,
) -> Result<(), Error> {
    let hashed_password = hash(plain_password, DEFAULT_COST)
        .expect("Failed to hash password");

    let query = r#"
        INSERT INTO users (username, password)
        VALUES (?, ?)
    "#;

    sqlx::query(query)
        .bind(username)
        .bind(hashed_password)
        .execute(db_pool)
        .await?;

    Ok(())
}

// 確認請求是否來自 localhost
fn is_request_from_localhost(req: &HttpRequest) -> bool {
    if let Some(peer_addr) = req.peer_addr() {
        return peer_addr.ip().is_loopback();
    }
    false
}