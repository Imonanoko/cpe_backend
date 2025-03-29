use actix_web::{post, web, HttpRequest, HttpResponse};
use actix_session::Session;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;

#[derive(Deserialize)]
pub struct ClaimedScholarshipForm {
    academic_year: Option<u32>, // 113 → 2024/09~2025/06
}

#[derive(Serialize)]
pub struct ClaimedScholarshipRow {
    student_id: String,
    name: String,
    correct_answers_count: i32,
    exam_date: String,
    scholarship_amount: i32,
    notes: Option<String>,
}

#[post("/api/claimed_scholarship_json")]
pub async fn claimed_scholarship_json(
    req: HttpRequest,
    session: Session,
    db: web::Data<MySqlPool>,
    form: web::Form<ClaimedScholarshipForm>,
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

    let rows = match sqlx::query!(
        r#"
        SELECT sr.StudentID, si.Name, sr.CorrectAnswersCount, sr.ReceivedDate, sr.Notes, sr.ScholarshipAmount
        FROM ScholarshipRecord sr
        JOIN StudentInfo si ON sr.StudentID = si.StudentID
        WHERE (? IS NULL OR sr.ReceivedDate >= ?)
        AND (? IS NULL OR sr.ReceivedDate <= ?)
        ORDER BY sr.ReceivedDate DESC
        "#,
        start_date, start_date,
        end_date, end_date
    )
    .fetch_all(db.get_ref())
    .await
    {
        Ok(r) => r,
        Err(e) => {
            println!("查詢失敗: {}", e);
            return HttpResponse::InternalServerError().body("查詢失敗");
        }
    };

    let result: Vec<ClaimedScholarshipRow> = rows
        .into_iter()
        .map(|row| ClaimedScholarshipRow {
            student_id: row.StudentID,
            name: row.Name,
            correct_answers_count: row.CorrectAnswersCount,
            exam_date: row.ReceivedDate.format("%Y-%m-%d").to_string(),
            scholarship_amount: row.ScholarshipAmount,
            notes: row.Notes,
        })
        .collect();

    HttpResponse::Ok().json(result)
}

