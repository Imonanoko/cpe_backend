use actix_web::{post, web, HttpRequest, HttpResponse};
use actix_session::Session;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;

#[derive(Deserialize)]
pub struct ScholarshipQueryForm {
    status: String,                  // all | claimed | unclaimed
    academic_year: Option<u32>,     // e.g., 113 → 2024/09~2025/08
}

#[derive(Serialize)]
pub struct ScholarshipRow {
    student_id: String,
    name: String,
    correct_answers_count: i32,
    exam_date: String,
    scholarship_amount: Option<i32>, // None for unclaimed
    notes: Option<String>,
    claimed: bool,                   // true = 已領, false = 未領
    received_date: Option<String>,  // Some for claimed, None for unclaimed
}

#[post("/api/query_scholarship_json")]
pub async fn query_scholarship_json(
    req: HttpRequest,
    session: Session,
    db: web::Data<MySqlPool>,
    form: web::Form<ScholarshipQueryForm>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期");
    }

    let (start_date, end_date) = match form.academic_year {
        Some(year) => {
            let start = NaiveDate::from_ymd_opt((year as i32) + 1911, 9, 1).unwrap();
            let end = NaiveDate::from_ymd_opt((year as i32) + 1912, 8, 31).unwrap();
            (Some(start), Some(end))
        }
        None => (None, None),
    };

    let mut result: Vec<ScholarshipRow> = Vec::new();

    // === 已領獎學金 ===
    if form.status == "all" || form.status == "claimed" {
        let claimed_rows = sqlx::query!(
            r#"
            SELECT 
                sr.StudentID, 
                si.Name, 
                sr.CorrectAnswersCount, 
                sr.ReceivedDate, 
                sr.Notes, 
                sr.ScholarshipAmount,
                (
                    SELECT es.ExamDate 
                    FROM ExamAttendance ea
                    JOIN ExamSessions es ON ea.ExamSession_SN = es.SN
                    WHERE ea.StudentID = sr.StudentID
                      AND ea.CorrectAnswersCount = sr.CorrectAnswersCount
                      AND es.ExamDate <= sr.ReceivedDate
                    ORDER BY es.ExamDate DESC
                    LIMIT 1
                ) AS ExamDate
            FROM ScholarshipRecord sr
            JOIN StudentInfo si ON sr.StudentID = si.StudentID
            WHERE (? IS NULL OR sr.ReceivedDate >= ?)
              AND (? IS NULL OR sr.ReceivedDate <= ?)
            "#,
            start_date, start_date,
            end_date, end_date
        )
        .fetch_all(db.get_ref())
        .await;

        match claimed_rows {
            Ok(rows) => {
                for row in rows {
                    let exam_date = match row.ExamDate {
                        Some(d) => d.format("%Y-%m-%d").to_string(),
                        None => row.ReceivedDate.format("%Y-%m-%d").to_string(), // fallback
                    };
                    result.push(ScholarshipRow {
                        student_id: row.StudentID,
                        name: row.Name,
                        correct_answers_count: row.CorrectAnswersCount,
                        exam_date,
                        scholarship_amount: Some(row.ScholarshipAmount),
                        notes: row.Notes,
                        claimed: true,
                        received_date: Some(row.ReceivedDate.format("%Y-%m-%d").to_string()),
                    });
                }
            }
            Err(err) => {
                println!("查詢已領獎學金錯誤: {}", err);
                return HttpResponse::InternalServerError().body("查詢已領資料時發生錯誤");
            }
        }
    }

    // === 未領獎學金 ===
    if form.status == "all" || form.status == "unclaimed" {
        let unclaimed_rows = sqlx::query!(
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
            SELECT StudentID, Name, CorrectAnswersCount, ExamDate
            FROM RankedResults
            WHERE rn = 1
            ORDER BY StudentID
            "#,
        )
        .fetch_all(db.get_ref())
        .await;

        match unclaimed_rows {
            Ok(rows) => {
                for row in rows {
                    result.push(ScholarshipRow {
                        student_id: row.StudentID,
                        name: row.Name,
                        correct_answers_count: row.CorrectAnswersCount.unwrap_or(0),
                        exam_date: row.ExamDate.format("%Y-%m-%d").to_string(),
                        scholarship_amount: None,
                        notes: None,
                        claimed: false,
                        received_date: None,
                    });
                }
            }
            Err(err) => {
                println!("查詢未領獎學金錯誤: {}", err);
                return HttpResponse::InternalServerError().body("查詢未領資料時發生錯誤");
            }
        }
    }

    HttpResponse::Ok().json(result)
}
