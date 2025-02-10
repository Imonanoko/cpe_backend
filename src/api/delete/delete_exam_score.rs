use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct StudentData {
    student_id: String,
    correct_answers_count: i32, 
}

#[derive(Debug, Deserialize)]
struct StudentSelection {
    students: Vec<StudentData>,
}

#[post("/api/delete_exam_score")]
async fn delete_exam_score(
    data: web::Json<StudentSelection>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let exam_session_sn = match session.get::<i32>("delete_exam_session_sn") {
        Ok(Some(sn)) => sn,
        Ok(None) => return HttpResponse::BadRequest().body("請先選擇要查詢的場次再刪除成績"),
        Err(_) => return HttpResponse::InternalServerError().body("伺服器錯誤，無法解析 session。"),
    };

    let ids: Vec<(String, i32)> = data.students.iter()
        .map(|s| (s.student_id.clone(), s.correct_answers_count))
        .collect();
    if ids.is_empty() {
        return HttpResponse::BadRequest().body("未選擇任何學生");
    }
    let mut transaction = match db_pool.begin().await {
        Ok(tx) => tx,
        Err(_) => return HttpResponse::InternalServerError().body("無法啟動資料庫交易"),
    };
    let query = r#"
        DELETE FROM ExamAttendance 
        WHERE ExamSession_SN = (?) 
        AND StudentID = (?) 
        AND CorrectAnswersCount = (?)
    "#;
    let mut delete_number = 0;

    for (id, count) in &ids {
        match sqlx::query(query)
            .bind(exam_session_sn)
            .bind(id)
            .bind(count)
            .execute(&mut *transaction)
            .await
        {
            Ok(result) => {
                if result.rows_affected() > 0 {
                    delete_number += 1;
                }
            }
            Err(e) => {
                eprintln!("刪除失敗: {:?}", e);
                let _ = transaction.rollback().await;
                return HttpResponse::InternalServerError().body("刪除失敗，所有變更已回滾");
            }
        }
    }

    // 提交交易
    match transaction.commit().await {
        Ok(_) => HttpResponse::Ok().body(format!("成功刪除 {} 筆記錄", delete_number)),
        Err(_) => {
            HttpResponse::InternalServerError().body("刪除失敗請重新提交刪除")
        }
    }
}