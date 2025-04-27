use crate::api::lib::{is_authorization, update_student_status};
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use serde::Deserialize;
use chrono::NaiveDate;

// 定義接收的 JSON 數據結構
#[derive(Deserialize)]
struct StudentData {
    student_id: String,
    status: String,
    correct_answers_count: i32,
    notes: String,
}

#[derive(Deserialize)]
struct ModifyRequest {
    session: String, // 場次名稱，例如 "場次1"
    students: Vec<StudentData>,
}

#[post("/api/update_exam_score")]
pub async fn update_exam_score(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
    mut data: web::Json<ModifyRequest>,
) -> HttpResponse {
    // 驗證授權
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    // 處理 session 字串，格式為 "日期,場次類型"
    let session_parts: Vec<&str> = data.session.split(',').collect();
    if session_parts.len() != 2 {
        return HttpResponse::BadRequest().body("場次格式無效，應為 '日期,場次類型'（例如 '2025-01-06,自辦'）");
    }

    let date_str = session_parts[0].trim();
    let exam_type = session_parts[1].trim();

    // 解析日期
    let exam_date = match NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        Ok(date) => date,
        Err(_) => return HttpResponse::BadRequest().body("無效的日期格式，應為 'YYYY-MM-DD'"),
    };

    // 從 session 中獲取 exam_session_sn（可選，根據需求決定是否保留）
    let exam_session_sn:i32 = match session.get("delete_exam_session_sn") {
        Ok(Some(sn)) => sn,
        Ok(None) => return HttpResponse::BadRequest().body("請先查詢後再進行修改。"),
        Err(_) => return HttpResponse::InternalServerError().body("server error"),
    };

    // 查詢 ExamSessions，獲取場次日期並驗證
    let session_info = sqlx::query!(
        "SELECT ExamDate FROM ExamSessions WHERE SN = ?",
        exam_session_sn
    )
    .fetch_optional(db_pool.as_ref())
    .await;

    let stored_exam_date: NaiveDate = match session_info {
        Ok(Some(record)) => record.ExamDate,
        _ => return HttpResponse::BadRequest().body("無效的場次"),
    };

    // 驗證日期是否匹配
    if stored_exam_date != exam_date {
        return HttpResponse::BadRequest().body("場次日期不匹配");
    }

    // 驗證場次類型是否匹配
    let session_match = sqlx::query!(
        "SELECT SN FROM ExamSessions WHERE ExamDate = ? AND ExamType = ? AND SN = ?",
        exam_date,
        exam_type,
        exam_session_sn
    )
    .fetch_optional(db_pool.as_ref())
    .await;

    match session_match {
        Ok(Some(_)) => {} // 符合，繼續執行
        _ => return HttpResponse::BadRequest().body("場次資訊與場次序號不匹配"),
    };

    // 處理學生資料並更新
    let mut updated_count = 0;

    for student in &mut data.students {
        // 根據 status 設置 IsAbsent 和 IsExcused
        let (is_absent, is_excused) = match student.status.as_str() {
            "缺考" => {
                student.correct_answers_count = 0;
                (true, false)
            },
            "請假" => {
                student.correct_answers_count = 0;
                (true, true)
            },
            _ => (false, false),
        };

        // 處理 notes，如果為空則設為 NULL
        let notes = if student.notes.trim().is_empty() {
            None
        } else {
            Some(student.notes.trim().to_string())
        };

        // 更新 ExamAttendance 表
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
            student.correct_answers_count,
            notes,
            student.student_id,
            exam_session_sn,
            is_absent,
            is_excused,
            student.correct_answers_count,
            notes
        )
        .execute(db_pool.as_ref())
        .await;

        match result {
            Ok(res) => {
                if res.rows_affected() > 0 {
                    // 如果更新成功，調用 update_student_status
                    if let Err(e) = update_student_status(db_pool.clone(), student.student_id.clone()).await {
                        println!("更新學生狀態失敗: {}", e);
                    }
                    updated_count += 1;
                }
            }
            Err(err) => {
                println!("更新學生資料失敗: {}", err);
                continue;
            }
        }
    }

    HttpResponse::Ok().body(format!("成功更新 {} 筆資料", updated_count))
}