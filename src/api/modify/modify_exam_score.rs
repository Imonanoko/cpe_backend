use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use actix_multipart::Multipart;
use futures_util::StreamExt as _;
use std::fs::File;
use std::io::Write;
use calamine::{DataType, Reader};
use chrono::NaiveDate;

#[post("/api/modify_exam_score")]
pub async fn modify_exam_score(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let exam_session_sn:i32 = match session.get("modify_exam_session_sn") {
        Ok(Some(sn)) => sn,
        Ok(None) => return HttpResponse::BadRequest().body("請先查詢後再上傳檔案進行修改。"),
        Err(_) => return HttpResponse::InternalServerError().body("server error"),
    };
    let temp_filepath = "./uploads/modify_exam_score.xlsx";
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

    if !std::path::Path::new(temp_filepath).exists() {
        println!("File does not exist: {}", temp_filepath);
        return HttpResponse::InternalServerError().body("Failed to process file");
    }

    let mut workbook = match calamine::open_workbook_auto(temp_filepath) {
        Ok(wb) => wb,
        Err(err) => {
            println!("Failed to open Excel file: {}", err);
            return HttpResponse::InternalServerError().body("無效的 Excel file");
        }
    };

    let range = match workbook.worksheet_range("Sheet1") {
        Ok(range) => range,
        Err(err) => {
            println!("Error reading sheet: {}", err);
            return HttpResponse::BadRequest().body("請將需要查詢的資料放入Sheet1");
        }
    };
    let exam_date: NaiveDate = match range.get((0, 0)).and_then(|cell| cell.get_string()) {
        Some(s) if s.starts_with("考試日期: ") => {
            let date_str = s.trim_start_matches("考試日期: ").trim();
            match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                Ok(date) => date,
                Err(_) => return HttpResponse::BadRequest().body("無效的考試日期"),
            }
        }
        _ => return HttpResponse::BadRequest().body("無效的考試日期"),
    };
    // 查詢 ExamSessions，確保 exam_date 與 session_sn 相符
    let session_match = sqlx::query!(
        "SELECT SN FROM ExamSessions WHERE ExamDate = ? AND SN = ?",
        exam_date,
        exam_session_sn
    )
    .fetch_optional(db_pool.as_ref())
    .await;

    match session_match {
        Ok(Some(_)) => {} // 符合，繼續執行
        _ => return HttpResponse::BadRequest().body("請先查詢後再上傳檔案進行修改。查詢的日期與修改的考試日期必須一樣"),
    };
    let mut updated_count = 0;

    // 讀取 Excel 資料並更新 `ExamAttendance`
    for row in range.rows().skip(2) {
        if row.len() < 4 {
            continue;
        }
    
        let student_id = row.get(0)
            .and_then(|cell| cell.get_string())
            .map(|s| s.trim().to_string());
        
        if student_id.is_none() {
            continue;
        }
        let student_id = student_id.unwrap();
    
        let attendance_status = row.get(1)
            .and_then(|cell| cell.get_string())
            .map(|s| s.trim().to_string());
    
        let (is_absent, is_excused) = match attendance_status.as_deref() {
            Some("缺考") => (true, false),
            Some("請假") => (true, true),
            _ => (false, false),
        };
    
        let correct_answers_count = row.get(2)
            .and_then(|cell| cell.get_float())
            .map(|n| n as i32)
            .unwrap_or(0);
    
        let notes = row.get(3)
            .and_then(|cell| cell.get_string())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
    
        // 更新資料庫，僅在有變更時更新
        let result = sqlx::query!(
            r#"
            UPDATE ExamAttendance
            SET 
                IsAbsent = ?, 
                IsExcused = ?, 
                CorrectAnswersCount = ?, 
                Notes = ?
            WHERE 
                StudentID = ? 
                AND ExamSession_SN = ?
                AND (IsAbsent <> ? OR IsExcused <> ? OR CorrectAnswersCount <> ? OR COALESCE(Notes, '') <> COALESCE(?, ''))
            "#,
            is_absent,
            is_excused,
            correct_answers_count,
            notes,
            student_id,
            exam_session_sn,
            is_absent,
            is_excused,
            correct_answers_count,
            notes
        )
        .execute(db_pool.as_ref())
        .await;
    
        if let Ok(res) = result {
            if res.rows_affected() > 0 {
                updated_count += 1;
            }
        }
    }

    HttpResponse::Ok().body(format!("成功更新 {} 筆資料", updated_count))    
}