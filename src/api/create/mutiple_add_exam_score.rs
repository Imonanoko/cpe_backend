use actix_web::{post, web, HttpResponse, HttpRequest};
use actix_session::Session;
use actix_multipart::Multipart;
use sqlx::MySqlPool;
use sqlx::Row;
use crate::api::lib::{is_authorization, update_student_status};
use std::fs::File;
use std::io::Write;
use calamine::DataType;
use calamine::Reader;
use futures_util::StreamExt as _;
use chrono::NaiveDate;

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
    let length = header_row.len();
    let mut exam_sn:Vec<i32> = Vec::new();
    for cell in header_row.iter().skip(1).step_by(2) {
        match cell.get_string() {
            Some(value) => {
                let split = value.split(",").collect::<Vec<&str>>();
                let date = match NaiveDate::parse_from_str(split[0], "%Y-%m-%d") {
                    Ok(date) => date,
                    Err(_) => {
                        return HttpResponse::BadRequest().body("日期格式錯誤，請將第二欄(column)以後的標題格式改為YYYY-MM-DD,(官辦、自辦)");
                    }
                };
                let exam_type = match split.get(1) {
                    Some(exam_type) => {
                        let exam_type_str = exam_type.to_string();
                        if exam_type_str == "官辦" || exam_type_str == "自辦"{
                            exam_type_str
                        }else {
                            return HttpResponse::BadRequest().body("場次種類格式錯誤，請將偶數欄(column)的標題格式改為YYYY-MM-DD,(官辦、自辦)");
                        }
                    }
                    None => {
                        return HttpResponse::BadRequest().body("場次種類格式錯誤，請將偶數欄(column)以後的標題格式改為YYYY-MM-DD,(官辦、自辦)");
                    }
                };
                let query = r#"
                    select SN from ExamSessions where ExamDate= (?) and ExamType= (?);
                "#;
                let row = match sqlx::query(query).bind(&date).bind(&exam_type).fetch_one(db_pool.get_ref()).await {
                    Ok(row) => row,
                    Err(sqlx::Error::RowNotFound) => {
                        return HttpResponse::BadRequest().body(format!("日期:{}, 場次種類:{}，找不到該場次的資料。請先新增或檢查該場次的資料", date, exam_type));
                    }
                    Err(err) => {
                        return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
                    }
                };
                exam_sn.push(row.try_get("SN").unwrap());
            }
            None => {
                return HttpResponse::BadRequest().body("標題出現未預期的格式");
            },
        };
    }
    for row in range.rows().skip(1) {
        let student_id = match row.get(0) {
            Some(id) => {
                if let Some(id) = id.get_string() {
                    id.to_ascii_uppercase()
                }else {
                    return HttpResponse::BadRequest().body("學號欄位不能為空");
                }
            }
            None => {
                return HttpResponse::InternalServerError().body("server讀取excel錯誤");
            }
        };
        for i in (1..length).step_by(2) {
            let mut absent = false;
            let mut excused = false;
            let mut score = 0;
            let mut note = String::new();
            match row.get(i) {
                Some(cell) => {
                    if let Some(text) = cell.get_string() {
                        let trimmed_text = text.trim();
                        if trimmed_text == "請假" {
                            absent = true;
                            excused = true;
                            score = 0;
                        } else if trimmed_text == "缺考" {
                            absent = true;
                            score = 0;
                        } else {
                            return HttpResponse::BadRequest().body(format!(
                                "無法解析成績: {} (第 {} 欄)，請填入整數(考試題數)或者請假、缺考(備註:有可能是考試題數格式是字串，請改為浮點數)",
                                trimmed_text, i + 1
                            ));
                        }
                    } else if let Some(num) = cell.get_float() {
                        //因為excel填入數字會自動將型別設定為浮點數,所以要轉成整數
                        if num.fract() == 0.0 {
                            // 確保數字是整數
                            score = num as i32;
                        } else {
                            return HttpResponse::BadRequest().body(format!(
                                "成績應為整數，但發現小數: {} (第 {} 欄)",
                                num, i + 1
                            ));
                        }
                    } else {
                        continue;
                    }
                }
                None => {
                    return HttpResponse::InternalServerError().body("server讀取excel錯誤");
                }
            }
    
            // 讀取備註 (下一欄)
            note = row.get(i + 1)
                .and_then(|cell| cell.get_string())
                .unwrap_or("")
                .to_string();
    
            // 插入資料到 ExamAttendance
            let query = r#"
                INSERT INTO ExamAttendance (ExamSession_SN, StudentID, IsAbsent, IsExcused, CorrectAnswersCount, Notes)
                VALUES (?, ?, ?, ?, ?, ?);
            "#;
            
            match sqlx::query(query)
                .bind(&exam_sn[i / 2])  // exam_sn 存的是從 header 解析的 ExamSession_SN
                .bind(&student_id)
                .bind(absent)
                .bind(excused)
                .bind(score)
                .bind(&note)
                .execute(db_pool.get_ref())
                .await
            {
                Ok(_) => {
                    match update_student_status(db_pool.clone(), student_id.clone()).await {
                        Ok(()) => {
                            println!("學生狀態更新成功");
                        }
                        Err(e) => {
                            println!("學生狀態更新失敗: {}", e);
                        }
                    }
                },
                Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
                    // 已經新增過的場次就跳過該筆資料
                    continue; 
                }
                Err(err) => {
                    return HttpResponse::InternalServerError().body(format!(
                        "寫入 ExamAttendance 失敗: {}",
                        err
                    ));
                }
            }
        }
    
    }
    

    HttpResponse::Ok().body("成功新增學生資料")
}