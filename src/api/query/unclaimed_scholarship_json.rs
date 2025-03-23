use actix_web::{get, web, HttpRequest, HttpResponse};
use actix_session::Session;
use sqlx::MySqlPool;
use chrono::NaiveDate;
use serde::Serialize;

use crate::api::lib::is_authorization;

#[derive(Serialize)]
struct UnclaimedScholar {
    student_id: String,
    name: String,
    correct_answers: i32,
    exam_date: String,
}

#[get("/api/unclaimed_scholarship_json")]
async fn unclaimed_scholarship_json(
    req: HttpRequest,
    db_pool: web::Data<MySqlPool>,
    session: Session,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    let query_result = sqlx::query!(
        r#"
        WITH RankedResults AS (
            SELECT
                si.StudentID,
                si.Name,
                ea.CorrectAnswersCount,
                es.ExamDate,
                ROW_NUMBER() OVER (
                    PARTITION BY si.StudentID
                    ORDER BY ea.CorrectAnswersCount DESC, es.ExamDate DESC
                ) AS rn
            FROM ExamAttendance ea
            JOIN ExamSessions es ON ea.ExamSession_SN = es.SN
            JOIN StudentInfo si ON ea.StudentID = si.StudentID
            LEFT JOIN ScholarshipRecord sr ON si.StudentID = sr.StudentID
            WHERE sr.StudentID IS NULL
              AND es.ExamType = '官辦'
              AND ea.CorrectAnswersCount >= 3
              AND ea.IsAbsent = FALSE
              AND ea.IsExcused = FALSE
        )
        SELECT
            StudentID,
            Name,
            CorrectAnswersCount,
            ExamDate
        FROM RankedResults
        WHERE rn = 1
        ORDER BY StudentID;
        "#
    )
    .fetch_all(db_pool.get_ref())
    .await;

    match query_result {
        Ok(rows) => {
            let result: Vec<UnclaimedScholar> = rows
                .into_iter()
                .map(|r| UnclaimedScholar {
                    student_id: r.StudentID,
                    name: r.Name,
                    correct_answers: r.CorrectAnswersCount.unwrap_or(0),
                    exam_date: r.ExamDate.format("%Y-%m-%d").to_string(),
                })
                .collect();
            HttpResponse::Ok().json(result)
        }
        Err(err) => {
            println!("查詢錯誤: {}", err);
            HttpResponse::InternalServerError().body("查詢資料時發生錯誤")
        }
    }
}
