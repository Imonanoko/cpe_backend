use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use sqlx::Row;
use chrono::NaiveDate;

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
    exam_attendance: Vec<ExamAttendance>,
}
#[derive(Serialize, Debug)]
struct ExamAttendance{
    exam_date: Option<NaiveDate>,
    exam_type: String,
    session_notes: Option<String>,
    is_absent: bool,
    is_excused: bool,
    correct_answers_count: u16,
    exam_notes: Option<String>,
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
    let student_id = from_data.student_id.clone();
    //查詢學生資料
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
        .bind(student_id.clone())
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
    let mut result = QueryResult {
        student_id: info.try_get(0).unwrap(),
        name: info.try_get(1).unwrap(),
        enrollment_status: info.try_get(2).unwrap(),
        student_attribute: info.try_get(3).unwrap(),
        is_passed: info.try_get(4).unwrap(),
        passing_criteria: info.try_get(5).unwrap(),
        notes: info.try_get(6).unwrap(),
        exam_attendance: Vec::new()
    };
    //查詢此學生的考試紀錄
    let query = r#"
        SELECT
            es.Examdate,
            es.ExamType,
            es.Notes as SessionNotes,
            ea.isAbsent,
            ea.IsExcused,
            CAST(ea.CorrectAnswersCount AS UNSIGNED) AS CorrectAnswersCount,
            ea.Notes as ExamNotes
        FROM
            ExamAttendance ea
        LEFT JOIN
            ExamSessions es
        ON
            ea.ExamSession_SN = es.SN
        WHERE
            ea.StudentID = (?);
    "#;
    let exam_attendance_rows = match sqlx::query(query)
        .bind(student_id)
        .fetch_all(db_pool.get_ref())
        .await
    {
        Ok(exam_attendance) => exam_attendance,
        Err(e) => {
            return HttpResponse::InternalServerError().body(format!("查詢考試紀錄時發生錯誤:{}",e));
        }
    };
    for exam_attendance_row in exam_attendance_rows.iter() {
        result.exam_attendance.push( ExamAttendance{
            exam_date: exam_attendance_row.try_get(0).expect("無法讀取 exam_date"),
            exam_type: exam_attendance_row.try_get(1).expect("無法讀取 exam_type"),
            session_notes:exam_attendance_row.try_get(2).expect("無法讀取 session_notes"),
            is_absent:exam_attendance_row.try_get(3).expect("無法讀取 is_absent"),
            is_excused:exam_attendance_row.try_get(4).expect("無法讀取 is_excused"),
            correct_answers_count:exam_attendance_row.try_get(5).expect("無法讀取 correct_answers_count"),
            exam_notes:exam_attendance_row.try_get(6).expect("無法讀取 exam_notes"),
        });
    }
    
    println!("result: {:#?}", result);
    HttpResponse::Ok().json(result)
}
