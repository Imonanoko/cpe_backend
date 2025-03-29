use actix_web::{post, web, HttpRequest, HttpResponse};
use actix_session::Session;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use xlsxwriter::Workbook;
use crate::api::lib::is_authorization;

#[derive(Deserialize)]
pub struct ClaimedScholarshipForm {
    academic_year: Option<u32>,
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

#[post("/api/claimed_scholarship_excel")]
pub async fn claimed_scholarship_excel(
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

    let filepath = "./uploads/claimed_scholarship.xlsx";
    let workbook = Workbook::new(filepath).unwrap();
    let mut sheet = workbook.add_worksheet(None).unwrap();

    // 標題
    sheet.write_string(0, 0, "學號", None).unwrap();
    sheet.write_string(0, 1, "姓名", None).unwrap();
    sheet.write_string(0, 2, "答對題數", None).unwrap();
    sheet.write_string(0, 3, "領取日期", None).unwrap();
    sheet.write_string(0, 4, "領取金額", None).unwrap();
    sheet.write_string(0, 5, "備註", None).unwrap();

    for (i, row) in rows.iter().enumerate() {
        let i = (i + 1) as u32;
        sheet.write_string(i, 0, &row.StudentID, None).unwrap();
        sheet.write_string(i, 1, &row.Name, None).unwrap();
        sheet.write_number(i, 2, row.CorrectAnswersCount as f64, None).unwrap();
        sheet.write_string(i, 3, &row.ReceivedDate.format("%Y-%m-%d").to_string(), None).unwrap();
        sheet.write_number(i, 4, row.ScholarshipAmount as f64, None).unwrap();
        sheet.write_string(i, 5, &row.Notes.clone().unwrap_or_default(), None).unwrap();
    }

    workbook.close().unwrap();

    match std::fs::read(filepath) {
        Ok(content) => HttpResponse::Ok()
            .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .append_header(("Content-Disposition", "attachment; filename=claimed_scholarship.xlsx"))
            .body(content),
        Err(e) => {
            println!("讀檔失敗: {}", e);
            HttpResponse::InternalServerError().body("Excel 下載失敗")
        }
    }
}
