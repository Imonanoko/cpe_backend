use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use chrono::NaiveDate;
use sqlx::Row;

#[derive(Deserialize, Debug)]
struct ExamDate {
    date: NaiveDate,
    exam_type: String,
}

#[derive(Debug, Serialize)]
struct ExamInfo {
    sn: i32,
    exam_date: chrono::NaiveDate,
    exam_type: String,
    notes: Option<String>,
}

#[post("/api/query_exam_info")]
async fn query_exam_info(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
    data: web::Json<ExamDate>,
) -> HttpResponse {
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let query = r#"
        SELECT SN, ExamDate, ExamType, Notes
        FROM ExamSessions
        WHERE ExamDate = ? AND ExamType = ?
    "#;

    // 執行查詢
    let row = sqlx::query(query)
        .bind(data.date)
        .bind(data.exam_type.clone())
        .fetch_one(db_pool.get_ref())
        .await;
    match row {
        Ok(row) => {
            let exam_info = ExamInfo {
                sn: row.get("SN"),
                exam_date: row.get("ExamDate"),
                exam_type: row.get("ExamType"),
                notes: row.get("Notes"),
            };
            session.insert("modify_exam_sn", &exam_info.sn).unwrap();
            session.insert("modify_exam_date", &exam_info.exam_date).unwrap();
            session.insert("modify_exam_type", &exam_info.exam_type).unwrap();
            session.insert("modify_notes", &exam_info.notes).unwrap();
            return HttpResponse::Ok().json(exam_info);
        }
        Err(err) => {
            return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
        }
    }
}