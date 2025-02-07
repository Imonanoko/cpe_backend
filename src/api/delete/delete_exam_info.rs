use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;
use serde::Deserialize;
use chrono::NaiveDate;
#[derive(Deserialize,Debug)]
struct DeleteExamInfo {
    date:NaiveDate,
    exam_type: String,
}

#[post("/api/delete_exam_info")]
async fn delete_exam_info(
    data: web::Json<DeleteExamInfo>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let exam_date = data.date;
    let exam_type = &data.exam_type;

    let mut tx = match db_pool.begin().await {
        Ok(tx) => tx,
        Err(err) => return HttpResponse::InternalServerError().body(format!("無法開始交易: {}", err)),
    };

    let exam_session = sqlx::query!(
        "SELECT SN FROM ExamSessions WHERE ExamDate = ? AND ExamType = ?",
        exam_date,
        exam_type
    )
    .fetch_optional(&mut *tx)
    .await;

    let exam_session_sn = match exam_session {
        Ok(Some(record)) => record.SN,
        Ok(None) => return HttpResponse::NotFound().body(format!("找不到{},{}的考試記錄",exam_date,exam_type)),
        Err(err) => return HttpResponse::InternalServerError().body(format!("查詢失敗: {}", err)),
    };

    let delete_attendance_result = sqlx::query!(
        "DELETE FROM ExamAttendance WHERE ExamSession_SN = ?",
        exam_session_sn
    )
    .execute(&mut *tx)
    .await;

    if let Err(err) = delete_attendance_result {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("刪除考試參與記錄失敗: {}", err));
    }

    let delete_exam_result = sqlx::query!(
        "DELETE FROM ExamSessions WHERE SN = ?",
        exam_session_sn
    )
    .execute(&mut *tx)
    .await;

    if let Err(err) = delete_exam_result {
        let _ = tx.rollback().await;
        return HttpResponse::InternalServerError().body(format!("刪除考試記錄失敗: {}", err));
    }

    if let Err(err) = tx.commit().await {
        return HttpResponse::InternalServerError().body(format!("提交交易失敗: {}", err));
    }

    HttpResponse::Ok().body("考試記錄刪除成功")
}