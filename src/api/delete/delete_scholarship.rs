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
    received_date: String, // 格式為 "YYYY-MM-DD"
}

#[derive(Deserialize)]
struct DeleteRequest {
    students: Vec<StudentData>,
}

#[post("/api/delete_scholarship")]
async fn delete_scholarship(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
    data: web::Json<DeleteRequest>,
) -> HttpResponse {
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

    let mut deleted_count = 0;

    // 遍歷每個學生資料，直接根據 student_id 和 exam_date 刪除
    for student in &data.students {
        // 解析 exam_date 為 NaiveDate
        let exam_date = match NaiveDate::parse_from_str(&student.received_date, "%Y-%m-%d") {
            Ok(date) => date,
            Err(_) => {
                let _ = tx.rollback().await; // 明確回滾
                return HttpResponse::BadRequest().body(format!("無效的日期格式: {}", student.received_date));
            }
        };

        // 刪除 ScholarshipRecord 記錄
        let result = sqlx::query!(
            "DELETE FROM ScholarshipRecord WHERE StudentID = ? AND ReceivedDate  = ?",
            student.student_id,
            exam_date
        )
        .execute(&mut *tx)
        .await;

        match result {
            Ok(res) => {
                deleted_count += res.rows_affected() as usize;
            }
            Err(e) => {
                println!("刪除記錄失敗: {}", e);
                let _ = tx.rollback().await; // 明確回滾
                return HttpResponse::InternalServerError().body(format!("刪除失敗: {}", e));
            }
        }
    }

    // 提交交易
    match tx.commit().await {
        Ok(_) => HttpResponse::Ok().body(format!("成功刪除 {} 筆獎學金紀錄", deleted_count)),
        Err(e) => {
            // 這裡不需要回滾，因為 tx 銷毀時會自動回滾
            HttpResponse::InternalServerError().body(format!("提交交易失敗: {}", e))
        }
    }
}