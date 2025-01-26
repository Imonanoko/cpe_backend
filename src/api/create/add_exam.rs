use actix_web::{post, web, HttpResponse, HttpRequest};
use actix_session::Session;
use crate::api::lib::is_authorization;
use serde::Deserialize;
use sqlx::MySqlPool;
use chrono::NaiveDate;
#[derive(Deserialize, Debug)]
enum ExamType {
    #[serde(rename = "official")]
    Official,
    #[serde(rename = "school")]
    School,
}
impl ExamType {
    fn to_string(&self) -> String {
        match self {
            ExamType::Official => "官辦".to_string(),
            ExamType::School => "自辦".to_string(),
        }
    }
}
#[derive(Deserialize, Debug)]
struct AddExam {
    date: NaiveDate,
    #[serde(rename = "type")]
    exam_type: ExamType,
    notes: String,    
}
#[post("/api/add_exam")]
async fn add_exam(
    data: web::Json<AddExam>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let date = data.date;
    let exam_type = data.exam_type.to_string();
    let notes = &data.notes;
    let byte_count = notes.as_bytes().len();
    if byte_count > 255 {
        return HttpResponse::BadRequest().body("Notes 長度過長");
    }
    let query = r#"
    INSERT INTO ExamSessions (ExamDate, ExamType, Notes) VALUES (?, ?, ?)
    "#;
    match sqlx::query(query)
        .bind(date)
        .bind(exam_type)
        .bind(notes)
        .execute(db_pool.get_ref())
        .await 
    {
        Ok(_) => (),
        Err(sqlx::Error::Database(err)) if err.code() == Some(std::borrow::Cow::Borrowed("23000")) => {
            return HttpResponse::Conflict().body("已經新增過此場次的考試");
        }
        Err(err) => {
            return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
        }
        
    }
    HttpResponse::Ok().body("")
}