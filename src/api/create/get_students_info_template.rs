use actix_web::{get, HttpResponse, HttpRequest};
use actix_session::Session;
use crate::api::lib::is_authorization;

#[get("/api/get_students_info_template")]
async fn get_students_info_template(
    req: HttpRequest,
    session: Session,
) -> HttpResponse {
    if !is_authorization(req, session){
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let students_info_template = "./uploads/students_info_template.xlsx";
    match std::fs::read(students_info_template) {
        Ok(file_data) => 
            HttpResponse::Ok()
            .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .append_header((
                "Content-Disposition",
                "attachment; filename=students_info_template.xlsx",
            ))
            .body(file_data),
        Err(err) => {
            println!("Error reading generated file: {}", err);
            HttpResponse::InternalServerError()
                .body("Failed to generate or retrieve result Excel file")
        }
    }
}