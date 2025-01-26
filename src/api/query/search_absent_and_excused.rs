use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Serialize,Deserialize};
use sqlx::MySqlPool;
use sqlx::Row;
use chrono::NaiveDate;
#[derive(Deserialize)]
struct Data {
    date: NaiveDate,
}
#[derive(Serialize)]
struct SearchAbsentAndExcused {
    student_id: String,
    status: String,
    notes: Option<String>,
}
#[post("/api/search_absent_and_excused")]
async fn search_absent_and_excused(
    data: web::Form<Data>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse{
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let query = r#"
        select
            ea.StudentID,
            ea.IsExcused,
            ea.Notes
        from
            ExamAttendance ea
        left join
            ExamSessions es
        on
            ea.ExamSession_SN = es.SN
        where
            es.ExamDate = (?)
            and
            ea.IsAbsent = 1

    "#;
    let rows = match sqlx::query(query).bind(data.date).fetch_all(db_pool.get_ref()).await {
        Ok(rows) => rows,
        Err(err) => {
            println!("{:?}", err);
            return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
        }
    };
    let mut info: Vec<SearchAbsentAndExcused> = Vec::new();
    for row in rows {
        let student_id: String = row.try_get("StudentID").unwrap();
        let is_excused: bool = row.try_get("IsExcused").unwrap();
        let notes: Option<String> = row.try_get("Notes").unwrap();
        info.push(SearchAbsentAndExcused {
            student_id,
            status: if is_excused {"請假".to_string()} else {"缺考".to_string()},
            notes
        });
    }
    HttpResponse::Ok().json(info)
}