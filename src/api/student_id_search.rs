use std::clone;
use std::result;

use super::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use sqlx::Row;
#[derive(Deserialize)]
struct FromData {
    student_id: String,
}
#[derive(Serialize, Debug)]
struct QueryResult {
    student_id: String,
    name: String,
    enrollment_status: String,
    student_attribute: String,
    is_passed: bool,
    passing_criteria: Option<String>,
    notes: Option<String>,
}
#[post("/api/student_id_search")]
async fn student_id_search(
    from_data: web::Form<FromData>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let query = r#"
    SELECT 
        si.StudentID, 
        si.Name, 
        es.Status AS EnrollmentStatus, 
        sa.Attribute AS StudentAttribute, 
        si.IsPassed, 
        si.PassingCriteria, 
        si.Notes
    FROM 
        StudentInfo si
    LEFT JOIN 
        EnrollmentStatus es 
    ON 
        si.EnrollmentStatus_SN = es.SN
    LEFT JOIN 
        StudentAttributes sa 
    ON 
        si.StudentAttribute_SN = sa.SN
    WHERE 
        si.StudentID = (?);
    "#;
    let info = match sqlx::query(query)
        .bind(from_data.student_id.clone())
        .fetch_one(db_pool.get_ref())
        .await
    {
        Ok(info) => info,
        Err(sqlx::Error::RowNotFound) => {
            return HttpResponse::NotFound().body("學號不存在");
        }
        Err(e) => {
            println!("查詢學號時發生錯誤: {}", e);
            return HttpResponse::InternalServerError().body("查詢學號時發生錯誤");
        }
    };
    let result = QueryResult {
        student_id: info.try_get(0).unwrap(),
        name: info.try_get(1).unwrap(),
        enrollment_status: info.try_get(2).unwrap(),
        student_attribute: info.try_get(3).unwrap(),
        is_passed: info.try_get(4).unwrap(),
        passing_criteria: info.try_get(5).unwrap(),
        notes: info.try_get(6).unwrap(),
    };
    println!("result: {:#?}", result);
    HttpResponse::Ok().json(result)
}
