use actix_web::HttpRequest;
use actix_session::Session;
use actix_web::web;
use sqlx::{MySqlPool, Error};
use sqlx::Row;
pub fn is_authorization(
    req: HttpRequest,
    session: Session,
) -> bool {
    let csrf_token_header = req
        .headers()
        .get("X-CSRF-Token")
        .and_then(|header| header.to_str().ok());
    let csrf_token_session: Option<String> = session.get("csrf_token").unwrap_or(None);
    if csrf_token_header != csrf_token_session.as_deref() {
        return false;
    }

    if session
        .get::<bool>("is_logged_in")
        .unwrap_or(Some(false))
        .unwrap_or(false)
        == false
    {
        return false;
    }
    true
}

pub async fn update_student_status(
    db_pool: web::Data<MySqlPool>,
    student_id: String,
) -> Result<(), Error> {
    // 1. 查詢 ExamAttendance 表中該學生的 CorrectAnswersCount 資料
    let exam_rows = sqlx::query(
        r#"
        SELECT CorrectAnswersCount 
        FROM ExamAttendance 
        WHERE StudentID = ?
        "#
    )
    .bind(&student_id)
    .fetch_all(db_pool.get_ref())
    .await?;

    // 2. 計算累計答對題數和最大一次答對題數
    let mut total_correct_answers = 0;
    let mut max_correct_answers = 0;
    for row in exam_rows {
        let correct_answers_count: i32 = row.get("CorrectAnswersCount");
        total_correct_answers += correct_answers_count;
        max_correct_answers = max_correct_answers.max(correct_answers_count);
    }
    // 3. 判斷 is_passed 與 passing_criteria
    let mut is_passed = false;
    let mut conditions = Vec::new();

    if max_correct_answers >= 2 {
        is_passed = true;
        conditions.push("一次兩題");
    }
    if total_correct_answers >= 3 {
        is_passed = true;
        conditions.push("累計3題");
    }

    let passing_criteria = if is_passed {
        Some(conditions.join("且"))
    } else {
        None
    };

    // 4. 更新 StudentInfo 表中的資料
    sqlx::query!(
        r#"
        UPDATE StudentInfo
        SET IsPassed = ?, PassingCriteria = ?
        WHERE StudentID = ?
        "#,
        is_passed,
        passing_criteria,
        student_id
    )
    .execute(db_pool.get_ref())
    .await?;

    Ok(())
}