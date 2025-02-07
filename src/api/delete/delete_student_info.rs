use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use calamine::DataType;
use calamine::Reader;
use futures_util::StreamExt as _;
use sqlx::{MySqlPool, Transaction};
use std::fs::File;
use std::io::Write;
use crate::api::lib::is_authorization;

#[post("/api/delete_student_info")]
async fn delete_student_info(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    let temp_filepath = "./uploads/temp_file.xlsx";
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
    //解析上傳的檔案，取得學號的list
    let mut workbook = match calamine::open_workbook_auto(temp_filepath) {
        Ok(wb) => wb,
        Err(err) => {
            println!("Failed to open Excel file: {}", err);
            return HttpResponse::InternalServerError().body("無效的 Excel file");
        }
    };

    let range = match workbook.worksheet_range("工作表1") {
        Ok(range) => range,
        Err(err) => {
            println!("Error reading sheet: {}", err);
            return HttpResponse::BadRequest().body("請將需要查詢的資料放入工作表1");
        }
    };
    let header_row = range.rows().next().unwrap();
    let id_col_index = header_row.iter().position(|cell| {
        if let Some(value) = cell.get_string() {
            value == "學號"
        } else {
            false
        }
    });

    let mut student_ids: Vec<String> = Vec::new();
    if let Some(col_index) = id_col_index {
        // 遍歷資料列，篩選符合條件的學號
        student_ids = range
            .rows()
            .skip(1) // 跳過標題列
            .filter_map(|row| row.get(col_index)) // 取出對應列的資料
            .filter_map(|cell| cell.get_string()) // 只取字串
            .map(|s| s.to_string().to_ascii_uppercase())
            .collect();
    } else {
        println!("未找到 '學號' 標題");
        return HttpResponse::BadRequest()
            .body("請將學號那欄(column)的第一列(row)的標題改為 '學號'");
    }

    if student_ids.is_empty() {
        return HttpResponse::BadRequest().body("沒有找到任何有效的學號");
    }

    let mut transaction: Transaction<'_, sqlx::MySql> = match db_pool.begin().await {
        Ok(tx) => tx,
        Err(err) => return HttpResponse::InternalServerError().body(err.to_string()),
    };

    let query_attendance = format!(
        "DELETE FROM ExamAttendance WHERE StudentID IN ({})",
        vec!["?"; student_ids.len()].join(",") // 生成 (?, ?, ?) 避免 SQL 注入
    );

    let mut query = sqlx::query(&query_attendance);
    for id in &student_ids {
        query = query.bind(id);
    }

    let result_attendance = query.execute(&mut *transaction).await;
    if let Err(err) = result_attendance {
        let _ = transaction.rollback().await;
        return HttpResponse::InternalServerError().body(err.to_string());
    }

    // 再刪除 StudentInfo
    let query_students = format!(
        "DELETE FROM StudentInfo WHERE StudentID IN ({})",
        vec!["?"; student_ids.len()].join(",") 
    );

    let mut query = sqlx::query(&query_students);
    for id in &student_ids {
        query = query.bind(id);
    }

    match query.execute(&mut *transaction).await {
        Ok(result) => {
            if let Err(err) = transaction.commit().await {
                return HttpResponse::InternalServerError().body(format!("SQL交易失敗: {}", err));
            }
            HttpResponse::Ok().body(format!("成功刪除 {} 筆學生資訊", result.rows_affected()))
        }
        Err(err) => {
            let _ = transaction.rollback().await;
            HttpResponse::InternalServerError().body(format!("刪除學生資訊失敗: {}", err))
        }
    }
}