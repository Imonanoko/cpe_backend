use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use calamine::DataType;
use calamine::Reader;
use futures_util::StreamExt as _;
use sqlx::MySqlPool;
use sqlx::Row;
use std::fs::File;
use std::io::Write;
use xlsxwriter::Workbook;
use super::lib::is_authorization;

#[post("/api/excel_search_pass")]
async fn excel_search_pass(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    // let csrf_token_header = req
    //     .headers()
    //     .get("X-CSRF-Token")
    //     .and_then(|header| header.to_str().ok());
    // let csrf_token_session: Option<String> = session.get("csrf_token").unwrap_or(None);
    // if csrf_token_header != csrf_token_session.as_deref() {
    //     return HttpResponse::Forbidden().body("無效的 CSRF Token");
    // }

    // if session
    //     .get::<bool>("is_logged_in")
    //     .unwrap_or(Some(false))
    //     .unwrap_or(false)
    //     == false
    // {
    //     return HttpResponse::Unauthorized().body("Session 無效或過期");
    // }

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
    SELECT 
        si.StudentID AS StudentID, 
        si.Name AS Name,
        CAST(COALESCE(SUM(ea.CorrectAnswersCount), 0) AS UNSIGNED INTEGER) AS TotalCorrectAnswers, 
        CAST(COALESCE(MAX(ea.CorrectAnswersCount), 0) AS UNSIGNED INTEGER) AS MaxCorrectAnswers
    FROM 
        StudentInfo si
    LEFT JOIN 
        ExamAttendance ea ON si.StudentID = ea.StudentID
    WHERE 
        si.StudentID = (?)
    GROUP BY 
        si.StudentID, si.Name;
    "#;

    let mut result: Vec<(String, String, u16, u8)> = Vec::new();
    for student_id in student_ids.iter() {
        let rows = match sqlx::query(query)
            .bind(student_id)
            .fetch_all(db_pool.get_ref())
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                println!("Database query failed: {}", err);
                continue;
            }
        };
        // println!("rows: {:#?}", rows);
        for row in rows {
            let student_id: String = row.try_get("StudentID").expect("Failed to get StudentID");
            let name: String = row
                .try_get("Name")
                .unwrap_or_else(|_| "Unknown".to_string());
            let total_correct_answers: u16 = row
                .try_get("TotalCorrectAnswers")
                .expect("Failed to get TotalCorrectAnswers");
            let max_correct_answers: u8 = row
                .try_get("MaxCorrectAnswers")
                .expect("Failed to get MaxCorrectAnswers");

            result.push((student_id, name, total_correct_answers, max_correct_answers));
        }
    }

    let output_filepath = "./uploads/result_file.xlsx";
    let workbook = Workbook::new(output_filepath).expect("Failed to create workbook");
    let mut worksheet = workbook.add_worksheet(None).unwrap();

    worksheet.write_string(0, 0, "學號", None).unwrap();
    worksheet.write_string(0, 1, "姓名", None).unwrap();
    worksheet.write_string(0, 2, "累計題數", None).unwrap();
    worksheet.write_string(0, 3, "最高題數", None).unwrap();
    worksheet.write_string(0, 4, "是否通過", None).unwrap();
    for (i, (student_id, name, total_correct_answers, max_correct_answers)) in
        result.iter().enumerate()
    {
        worksheet
            .write_string(i as u32 + 1, 0, student_id, None)
            .unwrap();
        worksheet
            .write_string(i as u32 + 1, 1, &name, None)
            .unwrap();
        worksheet
            .write_number(i as u32 + 1, 2, *total_correct_answers as f64, None)
            .unwrap();
        worksheet
            .write_number(i as u32 + 1, 3, *max_correct_answers as f64, None)
            .unwrap();
        let pass = *total_correct_answers >= 3 || *max_correct_answers >= 2;
        worksheet
            .write_string(i as u32 + 1, 4, if pass { "通過" } else { "不通過" }, None)
            .unwrap();
    }

    workbook.close().unwrap();

    match std::fs::read(output_filepath) {
        Ok(file_data) => HttpResponse::Ok()
            .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .append_header((
                "Content-Disposition",
                "attachment; filename=result_file.xlsx",
            ))
            .body(file_data),
        Err(err) => {
            println!("Error reading generated file: {}", err);
            HttpResponse::InternalServerError()
                .body("Failed to generate or retrieve result Excel file")
        }
    }
}
