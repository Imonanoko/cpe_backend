use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use serde::Deserialize;
use crate::api::lib::is_authorization;

#[derive(Deserialize)]
struct DeleteStudentRequest {
    student_id: String,
}

#[post("/api/delete_student")]
async fn delete_student(
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
    data: web::Json<DeleteStudentRequest>,
) -> HttpResponse {
    // 驗證授權
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }


    // 獲取學號
    let student_id = data.student_id.trim().to_ascii_uppercase();
    if student_id.is_empty() {
        return HttpResponse::BadRequest().body("學號不得為空");
    }
    println!("學號: {}", student_id);
    // 開始交易
    let mut tx = match db_pool.begin().await {
        Ok(t) => t,
        Err(e) => return HttpResponse::InternalServerError().body(format!("啟動交易失敗: {}", e)),
    };

    // 檢查學號是否存在
    let exists = sqlx::query!(
        "SELECT COUNT(*) as count FROM StudentInfo WHERE StudentID = ?",
        student_id
    )
    .fetch_one(&mut *tx)
    .await
    .map(|record| record.count > 0)
    .unwrap_or(false);

    if !exists {
        let _ = tx.rollback().await;
        return HttpResponse::BadRequest().body(format!("學號 {} 不存在", student_id));
    }

    // 刪除 ExamAttendance 表中的記錄
    let result_attendance = sqlx::query!(
        "DELETE FROM ExamAttendance WHERE StudentID = ?",
        student_id
    )
    .execute(&mut *tx)
    .await;

    if let Err(e) = result_attendance {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("學號 {}：刪除考試記錄失敗: {}", student_id, e));
    }

    // 刪除 StudentInfo 表中的記錄
    let result_student = sqlx::query!(
        "DELETE FROM StudentInfo WHERE StudentID = ?",
        student_id
    )
    .execute(&mut *tx)
    .await;

    match result_student {
        Ok(res) => {
            if res.rows_affected() == 0 {
                let _ = tx.rollback().await;
                return HttpResponse::InternalServerError().body(format!("學號 {}：刪除學生資訊失敗", student_id));
            }
            // 提交交易
            match tx.commit().await {
                Ok(_) => HttpResponse::Ok().body(format!("成功刪除學號 {}", student_id)),
                Err(e) => HttpResponse::InternalServerError().body(format!("提交交易失敗: {}", e)),
            }
        }
        Err(e) => {
            let _ = tx.rollback().await;
            HttpResponse::InternalServerError().body(format!("學號 {}：刪除學生資訊失敗: {}", student_id, e))
        }
    }
}