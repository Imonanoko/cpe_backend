use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use sqlx::MySqlPool;
use xlsxwriter::Workbook;

#[post("/api/unclaimed_scholarship")]
async fn unclaimed_scholarship(
    req: HttpRequest,
    db_pool: web::Data<MySqlPool>,
    session: Session,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }

    // 查詢未領獎學金且符合條件的學生（官辦、答對 >=3，選最大題數）
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

    let records = match query_result {
        Ok(data) => data,
        Err(err) => {
            println!("查詢失敗: {:?}", err);
            return HttpResponse::InternalServerError().body("查詢未領獎學金資料時發生錯誤");
        }
    };

    let output_filepath = "./uploads/unclaimed_scholarship.xlsx";
    let workbook = Workbook::new(output_filepath).expect("無法建立 Excel 檔案");
    let mut worksheet = workbook.add_worksheet(None).unwrap();

    // 標題列
    worksheet.write_string(0, 0, "學號", None).unwrap();
    worksheet.write_string(0, 1, "姓名", None).unwrap();
    worksheet.write_string(0, 2, "答對題數", None).unwrap();
    worksheet.write_string(0, 3, "考試日期", None).unwrap();

    // 資料列
    for (i, row) in records.iter().enumerate() {
        let row_index = (i + 1) as u32;
        worksheet.write_string(row_index, 0, &row.StudentID, None).unwrap();
        worksheet.write_string(row_index, 1, &row.Name, None).unwrap();
        worksheet.write_number(row_index, 2, row.CorrectAnswersCount.unwrap_or(0) as f64, None).unwrap();
        worksheet
            .write_string(row_index, 3, &row.ExamDate.format("%Y-%m-%d").to_string(), None)
            .unwrap();
    }

    workbook.close().unwrap();

    match std::fs::read(output_filepath) {
        Ok(file_data) => HttpResponse::Ok()
            .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .append_header(("Content-Disposition", "attachment; filename=unclaimed_scholarship.xlsx"))
            .body(file_data),
        Err(err) => {
            println!("讀取 Excel 檔案失敗: {}", err);
            HttpResponse::InternalServerError().body("讀取結果檔案時發生錯誤")
        }
    }
}
