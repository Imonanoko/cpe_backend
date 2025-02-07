use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;
use serde::Deserialize;
use chrono::NaiveDate;


#[post("/api/delete_exam_score")]
async fn delete_exam_score(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    HttpResponse::Ok().body("ok")
}