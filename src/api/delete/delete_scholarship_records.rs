use actix_multipart::Multipart;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use futures_util::StreamExt as _;
use sqlx::MySqlPool;
use std::fs::File;
use std::io::Write;
use calamine::{Reader, DataType};
use crate::api::lib::is_authorization;

#[post("/api/delete_scholarship_records")]
async fn delete_scholarship_records(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    let temp_filepath = "./uploads/delete_scholarship.xlsx";

    // 儲存上傳檔案
    while let Some(Ok(field)) = payload.next().await {
        if let Some(filename) = field.content_disposition().and_then(|cd| cd.get_filename()) {
            if !filename.ends_with(".xlsx") {
                return HttpResponse::BadRequest().body("請上傳 xlsx 檔案");
            }
            let mut f = File::create(temp_filepath).expect("建立檔案失敗");
            let mut field_stream = field;
            while let Some(chunk) = field_stream.next().await {
                let data = chunk.expect("讀取檔案錯誤");
                f.write_all(&data).expect("寫入檔案錯誤");
            }
        }
    }

    // 開啟 Excel 並解析
    let mut workbook = match calamine::open_workbook_auto(temp_filepath) {
        Ok(wb) => wb,
        Err(err) => {
            println!("開啟 Excel 錯誤: {}", err);
            return HttpResponse::InternalServerError().body("無法解析 Excel 檔案");
        }
    };

    let range = match workbook.worksheet_range("工作表1") {
        Ok(r) => r,
        Err(_) => return HttpResponse::BadRequest().body("請將資料放在名稱為『工作表1』的頁籤中"),
    };

    let header_row = range.rows().next().unwrap();
    let id_col_index = header_row.iter().position(|cell| {
        if let Some(value) = cell.get_string() {
            value == "學號"
        } else {
            false
        }
    });

    let mut student_ids = Vec::new();
    if let Some(col_index) = id_col_index {
        student_ids = range
            .rows()
            .skip(1)
            .filter_map(|row| row.get(col_index))
            .filter_map(|cell| cell.get_string())
            .map(|s| s.to_ascii_uppercase())
            .collect();
    } else {
        return HttpResponse::BadRequest().body("請將學號欄位名稱設定為『學號』");
    }

    if student_ids.is_empty() {
        return HttpResponse::BadRequest().body("未提供任何有效學號");
    }

    let mut tx = match db_pool.begin().await {
        Ok(t) => t,
        Err(e) => return HttpResponse::InternalServerError().body(format!("啟動交易失敗: {}", e)),
    };

    let query = format!(
        "DELETE FROM ScholarshipRecord WHERE StudentID IN ({})",
        vec!["?"; student_ids.len()].join(",")
    );

    let mut sql = sqlx::query(&query);
    for id in &student_ids {
        sql = sql.bind(id);
    }

    match sql.execute(&mut *tx).await {
        Ok(result) => {
            if let Err(e) = tx.commit().await {
                return HttpResponse::InternalServerError().body(format!("提交交易失敗: {}", e));
            }
            HttpResponse::Ok().body(format!("成功刪除 {} 筆獎學金紀錄", result.rows_affected()))
        }
        Err(e) => {
            let _ = tx.rollback().await;
            HttpResponse::InternalServerError().body(format!("刪除失敗: {}", e))
        }
    }
}
