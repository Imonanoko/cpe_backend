use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use serde::Deserialize;
use chrono::NaiveDate;
use crate::api::lib::is_authorization;

// 定義接收的 JSON 數據結構
#[derive(Deserialize)]
struct StudentData {
    student_id: String,
    correct_numbers: i32, // 對應 CorrectAnswersCount
    money: i32, // 對應 ScholarshipAmount
    note: Option<String>, // 對應 Notes
    claimed: bool,
    received_date: Option<String>, // 格式為 "YYYY-MM-DD"，可為 null
}

#[derive(Deserialize)]
struct UpdateRequest {
    students: Vec<StudentData>,
}

#[post("/api/update_scholarship")]
async fn update_scholarship(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
    data: web::Json<UpdateRequest>,
) -> HttpResponse {
    println!("update_scholarship");
    // 驗證授權
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    // 檢查是否有資料
    if data.students.is_empty() {
        return HttpResponse::BadRequest().body("未提供任何資料");
    }

    // 開始交易
    let mut tx = match db_pool.begin().await {
        Ok(t) => t,
        Err(e) => return HttpResponse::InternalServerError().body(format!("啟動交易失敗: {}", e)),
    };

    let mut processed_count = 0; // 記錄處理的筆數（更新或新增的筆數）

    // 遍歷每個學生資料
    for student in &data.students {
        // 解析 received_date
        let received_date = match &student.received_date {
            Some(date_str) => match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                Ok(date) => date,
                Err(_) => {
                    let _ = tx.rollback().await;
                    return HttpResponse::BadRequest().body(format!(
                        "學號 {}：無效的領取日期格式: {}",
                        student.student_id, date_str
                    ));
                }
            },
            None => {
                // 如果 claimed = true，但 received_date 為 null，返回錯誤
                if student.claimed {
                    let _ = tx.rollback().await;
                    return HttpResponse::BadRequest().body(format!(
                        "學號 {}：領取日期不得為空（當是否領獎為「是」時）",
                        student.student_id
                    ));
                }
                NaiveDate::from_ymd_opt(1970, 1, 1).unwrap() // 當 claimed = false 時，使用一個預設日期（因為表結構要求 NOT NULL）
            }
        };

        // 當 claimed = true 時，檢查 money、received_date 和 correct_numbers
        if student.claimed {
            if student.money == 0 {
                let _ = tx.rollback().await;
                return HttpResponse::BadRequest().body(format!(
                    "學號 {}：獎學金金額不得為 0（當是否領獎為「是」時）",
                    student.student_id
                ));
            }
            if student.correct_numbers < 3 {
                let _ = tx.rollback().await;
                return HttpResponse::BadRequest().body(format!(
                    "學號 {}：答對題數必須大於 3（當是否領獎為「是」時）",
                    student.student_id
                ));
            }
        }

        // 檢查 student_id 是否存在於 ScholarshipRecord 表中
        let exists = sqlx::query!(
            "SELECT COUNT(*) as count FROM ScholarshipRecord WHERE StudentID = ?",
            student.student_id
        )
        .fetch_one(&mut *tx)
        .await
        .map(|record| record.count > 0)
        .unwrap_or(false);

        // 如果 claimed = true，檢查 ExamAttendance 表中是否有符合條件的記錄
        if student.claimed {
            // 查詢 ExamAttendance 表，獲取該學生的所有考試記錄
            let attendance_records = sqlx::query!(
                r#"
                SELECT ea.CorrectAnswersCount, es.ExamDate
                FROM ExamAttendance ea
                JOIN ExamSessions es ON ea.ExamSession_SN = es.SN
                WHERE ea.StudentID = ? AND es.ExamDate <= ?
                "#,
                student.student_id,
                received_date
            )
            .fetch_all(&mut *tx)
            .await;

            let matches_condition = match attendance_records {
                Ok(records) => {
                    // 檢查是否存在 CorrectAnswersCount 等於 student.correct_numbers 的記錄
                    records.iter().any(|record| record.CorrectAnswersCount == Some(student.correct_numbers))
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    return HttpResponse::InternalServerError().body(format!(
                        "學號 {}：查詢考試記錄失敗: {}",
                        student.student_id, e
                    ));
                }
            };

            if !matches_condition {
                let _ = tx.rollback().await;
                return HttpResponse::BadRequest().body(format!(
                    "學號 {}：在 {} 之前的考試記錄中，找不到答對題數等於 {} 的記錄",
                    student.student_id, received_date, student.correct_numbers
                ));
            }
        }

        if exists {
            // 學號存在於表中
            if student.claimed {
                // claimed = true，更新記錄
                let result = sqlx::query!(
                    "UPDATE ScholarshipRecord SET CorrectAnswersCount = ?, ScholarshipAmount = ?, Notes = ?, ReceivedDate = ? WHERE StudentID = ?",
                    student.correct_numbers,
                    student.money,
                    student.note,
                    received_date,
                    student.student_id
                )
                .execute(&mut *tx)
                .await;

                match result {
                    Ok(res) => {
                        if res.rows_affected() > 0 {
                            processed_count += 1;
                        }
                    }
                    Err(e) => {
                        let _ = tx.rollback().await;
                        return HttpResponse::InternalServerError().body(format!(
                            "學號 {}：更新失敗: {}",
                            student.student_id, e
                        ));
                    }
                }
            } else {
                // claimed = false，刪除記錄
                let result = sqlx::query!(
                    "DELETE FROM ScholarshipRecord WHERE StudentID = ?",
                    student.student_id
                )
                .execute(&mut *tx)
                .await;

                match result {
                    Ok(res) => {
                        if res.rows_affected() > 0 {
                            processed_count += 1; // 刪除也算處理一筆
                        }
                    }
                    Err(e) => {
                        let _ = tx.rollback().await;
                        return HttpResponse::InternalServerError().body(format!(
                            "學號 {}：刪除失敗: {}",
                            student.student_id, e
                        ));
                    }
                }
            }
        } else {
            // 學號不存在於表中
            if student.claimed {
                // claimed = true，新增記錄
                let result = sqlx::query!(
                    "INSERT INTO ScholarshipRecord (StudentID, CorrectAnswersCount, ScholarshipAmount, Notes, ReceivedDate) VALUES (?, ?, ?, ?, ?)",
                    student.student_id,
                    student.correct_numbers,
                    student.money,
                    student.note,
                    received_date
                )
                .execute(&mut *tx)
                .await;

                match result {
                    Ok(res) => {
                        if res.rows_affected() > 0 {
                            processed_count += 1;
                        }
                    }
                    Err(e) => {
                        let _ = tx.rollback().await;
                        return HttpResponse::InternalServerError().body(format!(
                            "學號 {}：新增失敗: {}",
                            student.student_id, e
                        ));
                    }
                }
            } else {
                // claimed = false，跳過
                continue;
            }
        }
    }

    // 提交交易
    match tx.commit().await {
        Ok(_) => HttpResponse::Ok().body(format!("成功處理 {} 筆獎學金紀錄", processed_count)),
        Err(e) => HttpResponse::InternalServerError().body(format!("提交交易失敗: {}", e))
    }
}