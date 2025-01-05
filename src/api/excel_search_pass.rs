use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{post, HttpRequest, HttpResponse};
use futures_util::StreamExt as _;
use std::io::Write;

#[post("/api/excel_search_pass")]
async fn excel_search_pass(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
) -> HttpResponse {
    // 验证 CSRF Token
    let csrf_token_header = req
        .headers()
        .get("X-CSRF-Token")
        .and_then(|header| header.to_str().ok());
    let csrf_token_session: Option<String> = session.get("csrf_token").unwrap_or(None);
    if csrf_token_header != csrf_token_session.as_deref() {
        return HttpResponse::Forbidden().body("Invalid CSRF Token");
    }

    // 验证 Session
    if session
        .get::<bool>("is_logged_in")
        .unwrap_or(Some(false))
        .unwrap_or(false)
        == false
    {
        return HttpResponse::Unauthorized().body("Session invalid or expired");
    }

    let temp_filepath = "./uploads/temp_file.xlsx";

    while let Some(Ok(field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        if let Some(filename) = content_disposition.and_then(|cd| cd.get_filename()) {
            println!("Receiving file: {}", filename);
            let mut f = std::fs::File::create(temp_filepath).expect("Failed to create file");
            let mut field_stream = field;
            while let Some(chunk) = field_stream.next().await {
                let data = chunk.expect("Error reading chunk");
                println!("Writing chunk of size: {}", data.len());
                f.write_all(&data).expect("Error writing chunk");
            }
        }
    }

    if !std::path::Path::new(temp_filepath).exists() {
        println!("File does not exist: {}", temp_filepath);
        return HttpResponse::InternalServerError().body("Failed to process file");
    }
    
    if let Ok(metadata) = std::fs::metadata(temp_filepath) {
        println!("File metadata: {:?}", metadata);
    } else {
        println!("Failed to retrieve file metadata");
    }


    match std::fs::read(temp_filepath) {
        Ok(file_data) => {
            println!("Returning file of size: {}", file_data.len());
            HttpResponse::Ok()
                .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
                .append_header((
                    "Content-Disposition",
                    "attachment; filename=processed_file.xlsx",
                ))
                .body(file_data)
        },
        Err(err) => {
            println!("Error reading file: {}", err);
            HttpResponse::InternalServerError().body(format!("Error reading file: {}", err))
        }
    }
}
