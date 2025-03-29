use actix_web::{post, web, HttpRequest, HttpResponse};
use actix_session::Session;
use actix_multipart::Multipart;
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;
use std::fs::File;
use std::io::Write;
use calamine::{Reader, DataType,Data as calamineData};
use chrono::NaiveDate;
use futures_util::StreamExt as _;

#[post("/api/mutiple_add_scholarship")]
async fn mutiple_add_scholarship(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    let temp_filepath = "./uploads/scholarship.xlsx";

    // 儲存上傳的檔案
    while let Some(Ok(field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        if let Some(filename) = content_disposition.and_then(|cd| cd.get_filename()) {
            let file_ext = std::path::Path::new(filename)
                .extension()
                .and_then(|s| s.to_str());
            if file_ext != Some("xlsx") {
                return HttpResponse::BadRequest().body("請上傳 xlsx 檔案");
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
        return HttpResponse::InternalServerError().body("檔案處理失敗");
    }

    let mut workbook = match calamine::open_workbook_auto(temp_filepath) {
        Ok(wb) => wb,
        Err(err) => {
            println!("無法開啟 Excel：{}", err);
            return HttpResponse::InternalServerError().body("無效的 Excel 檔案");
        }
    };

    let range = match workbook.worksheet_range("工作表1") {
        Ok(r) => r,
        Err(_) => return HttpResponse::BadRequest().body("請將資料放在名稱為『工作表1』的頁籤中"),
    };

    // 開啟交易
    let mut tx = match db_pool.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            println!("交易開始失敗: {}", e);
            return HttpResponse::InternalServerError().body("交易開始失敗");
        }
    };

    for (i, row) in range.rows().skip(1).enumerate() {
        // println!("{:?} ", row);
        let line_num = i + 2;

        let student_id = match row.get(0).and_then(|c| c.get_string()) {
            Some(sid) => sid.trim().to_ascii_uppercase(),
            None => {
                tx.rollback().await.ok();
                return HttpResponse::BadRequest().body(format!("第 {} 列 學號為空或格式錯誤", line_num));
            }
        };

        let correct_count = match row.get(1) {
            Some(cell) => {
                match cell.get_float() {
                    Some(f) => {
                        // 如果小數部分為 0，代表是「整數」，可以轉成 i32
                        if f.fract() == 0.0 {
                            f as i32
                        } else {
                            tx.rollback().await.ok();
                            return HttpResponse::BadRequest().body(format!(
                                "第 {} 列 答對題數必須為整數，但發現小數：{}",
                                line_num, f
                            ));
                        }
                    }
                    None => {
                        // 表示不是 float/int 類型 (比如 string)
                        tx.rollback().await.ok();
                        return HttpResponse::BadRequest().body(format!(
                            "第 {} 列 答對題數格式錯誤 (非數值或非整數)",
                            line_num
                        ));
                    }
                }
            }
            None => {
                tx.rollback().await.ok();
                return HttpResponse::BadRequest().body(format!(
                    "第 {} 列 缺少答對題數欄位",
                    line_num
                ));
            }
        };

        let received_date = match row.get(2) {
            Some(cell) => {
                match cell {
                    calamineData::DateTime(dt) => {
                        let days = dt.as_f64().trunc() as i64;
                        let base_date = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
                        match base_date.checked_add_days(chrono::Days::new(days as u64)) {
                            Some(date) => date,
                            None => {
                                tx.rollback().await.ok();
                                return HttpResponse::BadRequest()
                                    .body(format!("第 {} 列 日期超出範圍", line_num));
                            }
                        }
                    }
                    calamineData::String(s) => {
                        match NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d") {
                            Ok(d) => d,
                            Err(_) => {
                                tx.rollback().await.ok();
                                return HttpResponse::BadRequest()
                                    .body(format!("第 {} 列 日期格式錯誤（非 YYYY-MM-DD）", line_num));
                            }
                        }
                    }
                    _ => {
                        tx.rollback().await.ok();
                        return HttpResponse::BadRequest()
                            .body(format!("第 {} 列 日期格式無效", line_num));
                    }
                }
            }
            None => {
                tx.rollback().await.ok();
                return HttpResponse::BadRequest()
                    .body(format!("第 {} 列 缺少領獎日期欄位", line_num));
            }
        };
        let amount = match row.get(3) {
            Some(cell) => match cell.get_float() {
                Some(f) => {
                    if f >= 0.0 {
                        f
                    } else {
                        tx.rollback().await.ok();
                        return HttpResponse::BadRequest()
                            .body(format!("第 {} 列 金額不得為負：{}", line_num, f));
                    }
                }
                None => {
                    tx.rollback().await.ok();
                    return HttpResponse::BadRequest()
                        .body(format!("第 {} 列 領取金額格式錯誤（非數值）", line_num));
                }
            },
            None => {
                tx.rollback().await.ok();
                return HttpResponse::BadRequest()
                    .body(format!("第 {} 列 缺少領取金額欄位", line_num));
            }
        };

        let notes = row.get(3).and_then(|c| c.get_string()).unwrap_or("").to_string();

        // 寫入 DB（若重複，回滾整批）
        let query = r#"
            INSERT INTO ScholarshipRecord (StudentID, CorrectAnswersCount, ReceivedDate, ScholarshipAmount, Notes)
            VALUES (?, ?, ?, ?, ?)
        "#;

        match sqlx::query(query)
            .bind(&student_id)
            .bind(correct_count)
            .bind(received_date)
            .bind(amount)
            .bind(&notes)
            .execute(&mut *tx)
            .await
        {
            Ok(_) => (),
            Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
                tx.rollback().await.ok();
                return HttpResponse::BadRequest()
                    .body(format!("第 {} 列的學生， 學號:{} 已存在於資料表中", line_num, student_id));
            }
            Err(e) => {
                tx.rollback().await.ok();
                return HttpResponse::InternalServerError()
                    .body(format!("第 {} 列寫入錯誤: {}", line_num, e));
            }
        }
    }

    // 提交交易
    if let Err(e) = tx.commit().await {
        println!("交易提交失敗: {}", e);
        return HttpResponse::InternalServerError().body("資料儲存失敗，交易未完成");
    }

    HttpResponse::Ok().body("成功新增獎學金資料")
}
