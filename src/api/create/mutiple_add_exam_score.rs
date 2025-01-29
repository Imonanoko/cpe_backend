use actix_web::{post, web, HttpResponse, HttpRequest};
use actix_session::Session;
use actix_multipart::Multipart;
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;
use std::fs::File;
use std::io::Write;
use calamine::DataType;
use calamine::Reader;
use futures_util::StreamExt as _;

#[post("/api/mutiple_add_exam_score")]
async fn mutiple_add_exam_score(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let temp_filepath = "./uploads/exam_score.xlsx";
    //儲存上傳的檔案
    while let Some(Ok(field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        if let Some(filename) = content_disposition.and_then(|cd| cd.get_filename()) {
            let file_ext = std::path::Path::new(filename)
                .extension()
                .and_then(|s| s.to_str());
            if file_ext != Some("xlsx") {
                return HttpResponse::BadRequest().body("請上傳xlsx檔案");
            }
            let mut f = File::create(temp_filepath).expect("Failed to create file");
            let mut field_stream = field;
            while let Some(chunk) = field_stream.next().await {
                let data = chunk.expect("Error reading chunk");
                f.write_all(&data).expect("Error writing chunk");
            }
        }
    }
}