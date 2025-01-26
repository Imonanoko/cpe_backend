use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{get, web, HttpRequest, HttpResponse};
use serde::Serialize;
use sqlx::MySqlPool;
use sqlx::Row;
use chrono::NaiveDate;
#[derive(Serialize, Debug)]
struct ExamSessionsInfo {
    info: Vec<String>
}
#[get("/api/get_exam_session_info")]
async fn get_exam_session_info(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let query = r#"
    SELECT 
        ExamDate,
        ExamType
    from 
        ExamSessions
    "#;
    let rows = match sqlx::query(query).fetch_all(db_pool.get_ref()).await {
        Ok(rows) => rows,
        Err(err) => {
            return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
        }
        
    };
    let mut info: Vec<String> = Vec::new();
    for row in rows {
        let date: NaiveDate = row.try_get("ExamDate").unwrap();
        let exam_type: String = row.try_get("ExamType").unwrap();
        info.push(format!("{},{}", date, exam_type));
    }
    info.reverse();
    HttpResponse::Ok().json(ExamSessionsInfo { info })
}
