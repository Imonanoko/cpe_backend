use crate::api::lib::is_authorization;
use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use calamine::DataType;
use calamine::Reader;
use chrono::NaiveDate;
use futures_util::StreamExt as _;
use sqlx::MySqlPool;
use sqlx::Row;
use std::fs::File;
use std::io::Write;
use xlsxwriter::Workbook;
use base64::Engine as _;
use serde::Serialize;

#[derive(Serialize)]
struct AbsentResult {
    student_id: String,
    absent_status: String,
    exam_date: String, // NaiveDate 轉為 String 以便序列化
    exam_type: String,
    notes: Option<String>,
}

#[derive(Serialize)]
struct ApiResponse {
    results: Vec<AbsentResult>,
    excel_file: String, // base64 編碼的 Excel 檔案
}
#[post("/api/excel_search_absent")]
async fn excel_search_absent(
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
            .map(|s| s.to_string())
            .collect();
    } else {
        println!("未找到 '學號' 標題");
        return HttpResponse::BadRequest()
            .body("請將學號那欄(column)的第一列(row)的標題改為 '學號'");
    }

    if student_ids.is_empty() {
        return HttpResponse::BadRequest().body("沒有找到任何有效的學號");
    }
    let query = r#"
    SELECT ea.IsExcused , es.ExamDate, es.ExamType, ea.Notes
    FROM ExamAttendance ea
    JOIN ExamSessions es ON ea.ExamSession_SN = es.SN
    WHERE ea.StudentID = (?)
    AND ea.IsAbsent = TRUE
    ORDER BY es.ExamDate DESC
    LIMIT 1;
    "#;

    let mut results: Vec<AbsentResult> = Vec::new();
    for student_id in student_ids.iter() {
        match sqlx::query(query)
            .bind(student_id)
            .fetch_optional(db_pool.get_ref()) // 改成 fetch_optional
            .await
        {
            Ok(Some(row)) => {
                // 如果查到資料
                let is_excused: bool = row.try_get("IsExcused").expect("Failed to get IsExcused");
                let absent_status = if is_excused { "請假".to_string() } else { "缺考".to_string()};
                let exam_date: NaiveDate = row.try_get("ExamDate").expect("Failed to get ExamDate");
                let exam_type: String = row.try_get("ExamType").expect("Failed to get ExamType");
                let notes: Option<String> = row.try_get("Notes").expect("Failed to get Notes");
                results.push(AbsentResult {
                    student_id: student_id.to_string(),
                    absent_status,
                    exam_date: exam_date.to_string(), // 轉為 String
                    exam_type,
                    notes,
                });
            }
            Ok(None) => {
                ()
            }
            Err(err) => {
                // 如果查詢過程中發生錯誤
                return HttpResponse::InternalServerError().body(format!("查詢失敗: {}", err));
            }
        }
    }

    let output_filepath = "./uploads/result_file.xlsx";
    let workbook = Workbook::new(output_filepath).expect("Failed to create workbook");
    let mut worksheet = workbook.add_worksheet(None).unwrap();

    worksheet.write_string(0, 0, "學號", None).unwrap();
    worksheet.write_string(0, 1, "缺考/請假", None).unwrap();
    worksheet.write_string(0, 2, "考試日期", None).unwrap();
    worksheet.write_string(0, 3, "考試種類", None).unwrap();
    worksheet.write_string(0, 4, "備註", None).unwrap();

    for (i, result) in results.iter().enumerate() {
        worksheet
            .write_string(i as u32 + 1, 0, &result.student_id, None)
            .unwrap();
        worksheet
            .write_string(i as u32 + 1, 1, &result.absent_status, None)
            .unwrap();
        worksheet
            .write_string(i as u32 + 1, 2, &result.exam_date, None)
            .unwrap();
        worksheet
            .write_string(i as u32 + 1, 3, &result.exam_type, None)
            .unwrap();
        worksheet
            .write_string(i as u32 + 1, 4, &result.notes.clone().unwrap_or_default(), None)
            .unwrap();
    }

    workbook.close().unwrap();

    // 讀取 Excel 檔案並轉為 base64
    let excel_file_data = match std::fs::read(output_filepath) {
        Ok(data) => data,
        Err(err) => {
            println!("Error reading generated file: {}", err);
            return HttpResponse::InternalServerError()
                .body("Failed to generate or retrieve result Excel file");
        }
    };
    let excel_file_base64 = base64::engine::general_purpose::STANDARD.encode(&excel_file_data);

    // 構建 JSON 響應
    let response = ApiResponse {
        results,
        excel_file: excel_file_base64,
    };

    HttpResponse::Ok()
        .content_type("application/json")
        .json(response)
}
