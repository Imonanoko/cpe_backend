use actix_web::{post, web, HttpResponse, HttpRequest};
use actix_session::Session;
use crate::api::lib::is_authorization;
use serde::Deserialize;
use sqlx::MySqlPool;

#[derive(Deserialize, Debug)]
enum EnrollmentStatus {
    #[serde(rename = "currentlyEnrolled")]
    CurrentlyEnrolled,//在學
    #[serde(rename = "onALeaveOfAbsence")]
    OnALeaveOfAbsence,//休學
    #[serde(rename = "droppedOut")]
    DroppedOut,//退學
}
impl EnrollmentStatus {
    fn to_sn(&self) -> i32 {
        match self {
            EnrollmentStatus::CurrentlyEnrolled => 1,
            EnrollmentStatus::OnALeaveOfAbsence => 2,
            EnrollmentStatus::DroppedOut => 3,
        }
    }
}
#[derive(Deserialize, Debug)]
enum StudentAttribute {
    #[serde(rename = "departmental")]
    Departmental,//本系
    #[serde(rename = "interdepartmental")]
    Interdepartmental,//外系
    #[serde(rename = "externalStudents")]
    ExternalStudents,//外校
}
impl StudentAttribute {
    fn to_sn(&self) -> i32 {
        match self {
            StudentAttribute::Departmental => 1,
            StudentAttribute::Interdepartmental => 2,
            StudentAttribute::ExternalStudents => 3,
        }
    }
}
#[derive(Deserialize, Debug)]
struct AddExam {
    #[serde(rename = "studentID")]
    student_id: String,
    name: String,
    #[serde(rename = "enrollmentStatus")]
    enrollment_status: EnrollmentStatus,
    #[serde(rename = "studentAttribute")]
    student_attribute: StudentAttribute,
    notes: String,    
}
#[post("/api/single_add_student")]
async fn single_add_student(
    data: web::Json<AddExam>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let query = r#"
        INSERT INTO StudentInfo (
            StudentID,
            Name,
            EnrollmentStatus_SN,
            StudentAttribute_SN,
            Notes
        ) VALUES (?, ?, ?, ?, ?)
    "#;

    match sqlx::query(query)
        .bind(&data.student_id.to_ascii_uppercase())
        .bind(&data.name)
        .bind(data.enrollment_status.to_sn())
        .bind(data.student_attribute.to_sn())
        .bind(&data.notes)
        .execute(db_pool.get_ref())
        .await
    {
        Ok(_) => (),
        Err(sqlx::Error::Database(err)) if err.code() == Some(std::borrow::Cow::Borrowed("23000")) => {
            return HttpResponse::Conflict().body("此學號已經被新增過。");
        }
        Err(err) => {
            return HttpResponse::InternalServerError().body(err.to_string());
        }
    }

    HttpResponse::Ok().body("")
}