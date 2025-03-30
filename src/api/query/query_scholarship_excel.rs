use actix_web::{post, web, HttpRequest, HttpResponse};
use actix_session::Session;
use chrono::NaiveDate;
use serde::Deserialize;
use sqlx::MySqlPool;
use xlsxwriter::Workbook;
use crate::api::lib::is_authorization;

#[derive(Deserialize)]
pub struct ScholarshipExcelForm {
    academic_year: Option<u32>,
    status: String, // all | claimed | unclaimed
}

#[post("/api/query_scholarship_excel")]
pub async fn query_scholarship_excel(
    req: HttpRequest,
    session: Session,
    db: web::Data<MySqlPool>,
    form: web::Form<ScholarshipExcelForm>,
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

    let mut all_records: Vec<(String, String, i32, String, Option<i32>, Option<String>, bool, Option<String>)> = Vec::new();

    // 查已領
    if form.status == "all" || form.status == "claimed" {
        let claimed = sqlx::query!(
            r#"
            SELECT sr.StudentID, si.Name, sr.CorrectAnswersCount, sr.ReceivedDate, sr.Notes, sr.ScholarshipAmount,
                (
                    SELECT es.ExamDate
                    FROM ExamAttendance ea
                    JOIN ExamSessions es ON ea.ExamSession_SN = es.SN
                    WHERE ea.StudentID = sr.StudentID
                    AND ea.CorrectAnswersCount = sr.CorrectAnswersCount
                    AND es.ExamDate <= sr.ReceivedDate
                    AND ea.IsAbsent = FALSE
                    AND ea.IsExcused = FALSE
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

        if let Ok(rows) = claimed {
            for row in rows {
                all_records.push((
                    row.StudentID,
                    row.Name,
                    row.CorrectAnswersCount,
                    row.ExamDate.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "".into()),
                    Some(row.ScholarshipAmount),
                    row.Notes,
                    true,
                    Some(row.ReceivedDate.format("%Y-%m-%d").to_string()),
                ));
            }
        }
    }

    // 查未領
    if form.status == "all" || form.status == "unclaimed" {
        let unclaimed = sqlx::query!(
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
            "#
        )
        .fetch_all(db.get_ref())
        .await;

        if let Ok(rows) = unclaimed {
            for row in rows {
                all_records.push((
                    row.StudentID,
                    row.Name,
                    row.CorrectAnswersCount.unwrap_or(0),
                    row.ExamDate.format("%Y-%m-%d").to_string(),
                    None,
                    None,
                    false,
                    None,
                ));
            }
        }
    }

    // 寫入 Excel
    let filepath = "./uploads/scholarship_result.xlsx";
    let workbook = Workbook::new(filepath).unwrap();
    let mut sheet = workbook.add_worksheet(None).unwrap();

    sheet.write_string(0, 0, "學號", None).unwrap();
    sheet.write_string(0, 1, "姓名", None).unwrap();
    sheet.write_string(0, 2, "答對題數", None).unwrap();
    sheet.write_string(0, 3, "考試日期", None).unwrap();
    sheet.write_string(0, 4, "獎學金金額", None).unwrap();
    sheet.write_string(0, 5, "備註", None).unwrap();
    sheet.write_string(0, 6, "是否領獎", None).unwrap();
    sheet.write_string(0, 7, "領獎日期", None).unwrap();

    for (i, row) in all_records.iter().enumerate() {
        let i = (i + 1) as u32;
        sheet.write_string(i, 0, &row.0, None).unwrap();
        sheet.write_string(i, 1, &row.1, None).unwrap();
        sheet.write_number(i, 2, row.2 as f64, None).unwrap();
        sheet.write_string(i, 3, &row.3, None).unwrap();
        if let Some(amount) = row.4 {
            sheet.write_number(i, 4, amount as f64, None).unwrap();
        }
        sheet.write_string(i, 5, &row.5.clone().unwrap_or_default(), None).unwrap();
        sheet.write_string(i, 6, if row.6 { "是" } else { "否" }, None).unwrap();
        sheet.write_string(i, 7, &row.7.clone().unwrap_or_default(), None).unwrap();
    }

    workbook.close().unwrap();

    match std::fs::read(filepath) {
        Ok(data) => HttpResponse::Ok()
            .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .append_header(("Content-Disposition", "attachment; filename=scholarship_result.xlsx"))
            .body(data),
        Err(e) => {
            println!("讀取 Excel 失敗: {}", e);
            HttpResponse::InternalServerError().body("讀取 Excel 錯誤")
        }
    }
}
