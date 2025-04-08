use actix_web::{post, web, HttpRequest, HttpResponse};
use actix_session::Session;
use actix_multipart::Multipart;
use sqlx::{MySqlPool, Row};
use crate::api::lib::is_authorization;
use crate::api::lib::update_student_status;
use std::fs::File;
use std::io::Write;
use calamine::{Reader, DataType,Data as calamineData};
use chrono::NaiveDate;
use futures_util::StreamExt as _;
use std::collections::HashSet;

#[post("/api/mutiple_add_exam_score")]
pub async fn mutiple_add_exam_score(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    let filepath = "./uploads/exam_score.xlsx";

    // 儲存上傳的檔案
    while let Some(Ok(mut field)) = payload.next().await {
        if let Some(filename) = field.content_disposition().and_then(|cd| cd.get_filename()) {
            if !filename.ends_with(".xlsx") {
                return HttpResponse::BadRequest().body("請上傳 .xlsx 檔案");
            }

            let mut f = File::create(filepath).expect("create file failed");
            while let Some(chunk) = field.next().await {
                let data = chunk.expect("chunk error");
                f.write_all(&data).expect("write chunk error");
            }
        }
    }

    let mut workbook = match calamine::open_workbook_auto(filepath) {
        Ok(wb) => wb,
        Err(e) => {
            println!("開啟 Excel 錯誤: {}", e);
            return HttpResponse::InternalServerError().body("開啟 Excel 檔案失敗");
        }
    };

    let range = match workbook.worksheet_range("工作表1") {
        Ok(r) => r,
        Err(_) => return HttpResponse::BadRequest().body("請確認檔案中有名為 '工作表1' 的工作表"),
    };

    let mut exam_sn = Vec::new();
    let headers = range.rows().next().unwrap();
    for cell in headers.iter().skip(1).step_by(2) {
        let Some(info) = cell.get_string() else {
            return HttpResponse::BadRequest().body("欄位標題格式錯誤");
        };
        let parts: Vec<&str> = info.split(',').collect();
        if parts.len() != 2 {
            return HttpResponse::BadRequest().body("請使用 'YYYY-MM-DD,官辦/自辦' 作為欄位標題格式");
        }

        let date = match NaiveDate::parse_from_str(parts[0], "%Y-%m-%d") {
            Ok(d) => d,
            Err(_) => return HttpResponse::BadRequest().body("日期格式錯誤，請使用 YYYY-MM-DD"),
        };

        let exam_type = parts[1];
        if exam_type != "官辦" && exam_type != "自辦" {
            return HttpResponse::BadRequest().body("考試類型需為 '官辦' 或 '自辦'");
        }

        let row = match sqlx::query("SELECT SN FROM ExamSessions WHERE ExamDate = ? AND ExamType = ?")
            .bind(date)
            .bind(exam_type)
            .fetch_one(db_pool.get_ref())
            .await
        {
            Ok(r) => r,
            Err(_) => {
                return HttpResponse::BadRequest().body(format!("找不到場次: {},{}", parts[0], parts[1]));
            }
        };
        exam_sn.push(row.get::<i32, _>("SN"));
    }

    let mut student_ids_in_excel = HashSet::new();
    for row in range.rows().skip(1) {
        if let Some(cell) = row.get(0) {
            if let Some(id) = cell.get_string() {
                student_ids_in_excel.insert(id.to_ascii_uppercase());
            }
        }
    }

    let placeholders = student_ids_in_excel.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let query = format!("SELECT StudentID FROM StudentInfo WHERE StudentID IN ({})", placeholders);

    let mut query_builder = sqlx::query(&query);
    for id in &student_ids_in_excel {
        query_builder = query_builder.bind(id);
    }

    let result = match query_builder.fetch_all(db_pool.get_ref()).await {
        Ok(rows) => rows.into_iter().map(|r| r.get::<String, _>("StudentID")).collect::<HashSet<_>>(),
        Err(e) => {
            println!("查詢學生清單失敗: {}", e);
            return HttpResponse::InternalServerError().body("查詢學生資料錯誤");
        }
    };

    let missing_students: Vec<_> = student_ids_in_excel.difference(&result).cloned().collect();
    if !missing_students.is_empty() {
        return HttpResponse::BadRequest().json(missing_students);
    }

    // SQL Transaction
    let mut tx = match db_pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            println!("無法開始交易: {}", e);
            return HttpResponse::InternalServerError().body("系統錯誤");
        }
    };
    let mut update_list = Vec::new();
    for row in range.rows().skip(1) {
        let Some(student_id_raw) = row.get(0) else {
            return HttpResponse::BadRequest().body("缺少學號");
        };

        let Some(student_id) = student_id_raw.get_string() else {
            return HttpResponse::BadRequest().body("學號格式錯誤");
        };
        let student_id = student_id.to_ascii_uppercase();

        for i in (1..headers.len()).step_by(2) {
            let cell = row.get(i);
            let note = row.get(i + 1).and_then(|c| c.get_string()).unwrap_or("").to_string();

            let (mut is_absent, mut is_excused, mut score) = (false, false, 0);

            match cell {
                Some(calamineData::String(s)) => {
                    match s.trim() {
                        "請假" => { is_absent = true; is_excused = true; }
                        "缺考" => { is_absent = true; }
                        other => {
                            return HttpResponse::BadRequest()
                                .body(format!("第 {} 欄格式錯誤: {}", i + 1, other));
                        }
                    }
                }
                Some(calamineData::Float(f)) => {
                    if f.fract() == 0.0 {
                        score = *f as i32;
                    } else {
                        return HttpResponse::BadRequest()
                            .body(format!("第 {} 欄成績應為整數", i + 1));
                    }
                }
                _ => continue,
            }

            let insert = sqlx::query(
                r#"
                INSERT INTO ExamAttendance (ExamSession_SN, StudentID, IsAbsent, IsExcused, CorrectAnswersCount, Notes)
                VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(exam_sn[i / 2])
            .bind(&student_id)
            .bind(is_absent)
            .bind(is_excused)
            .bind(score)
            .bind(&note)
            .execute(&mut *tx)
            .await;

            if let Err(e) = insert {
                if let sqlx::Error::Database(db_err) = &e {
                    if db_err.is_unique_violation() {
                        continue;
                    }
                }
                return HttpResponse::InternalServerError().body(format!("寫入資料失敗: {}", e));
            }else {
                update_list.push(student_id.clone());
            }
            
        }
    }

    if let Err(e) = tx.commit().await {
        println!("交易提交失敗: {}", e);
        return HttpResponse::InternalServerError().body("交易失敗");
    }
    for student_id in update_list {
        if let Err(e) = update_student_status(db_pool.clone(), student_id).await {
            println!("更新學生狀態失敗: {}", e);
        }
    }
    HttpResponse::Ok().body("成功新增學生考試資料")
}
