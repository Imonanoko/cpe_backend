use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use chrono::NaiveDate;
use xlsxwriter::Workbook;
#[derive(Deserialize, Debug)]
enum CRUD {
    #[serde(rename = "update")]
    Update,
    #[serde(rename = "query")]
    Query,
    #[serde(rename = "delete")]
    Delete,
}
#[derive(Deserialize)]
struct QueryScoreInfoForm {
    date: NaiveDate,
    exam_type: String,
    crud_type:CRUD,
}
#[derive(Serialize)]
struct ScoreInfo {
    student_id: String,
    status:String,
    correct_number: Option<i32>,
    notes: Option<String>,
}
#[post("/api/query_score_info")]
async fn query_score_info(
    pool: web::Data<MySqlPool>,
    req: HttpRequest,
    session: Session,
    data: web::Json<QueryScoreInfoForm>,
) -> HttpResponse {
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let exam_session_result = sqlx::query!(
        r#"
        SELECT SN FROM ExamSessions
        WHERE ExamDate = ? AND ExamType = ?
        "#,
        data.date, data.exam_type
    )
    .fetch_one(&**pool)
    .await;
    let exam_session_sn = match exam_session_result {
        Ok(record) => record.SN,
        Err(_) => return HttpResponse::NotFound().body("未找到對應的考試場次"),
    };
    if let CRUD::Update = data.crud_type {
        if let Err(_) = session.insert("modify_exam_session_sn", exam_session_sn) {
            return HttpResponse::InternalServerError().body("無法存入 session");
        }
    }
    let exam_attendance_result = sqlx::query!(
        r#"
        SELECT StudentID, IsAbsent, IsExcused, CorrectAnswersCount, Notes
        FROM ExamAttendance
        WHERE ExamSession_SN = ?
        "#,
        exam_session_sn
    )
    .fetch_all(&**pool)
    .await;
    let exam_attendance_records = match exam_attendance_result {
        Ok(records) => records,
        Err(_) => return HttpResponse::InternalServerError().body("查詢考試成績資料失敗"),
    };

    if let CRUD::Delete = data.crud_type {
        let mut score_info: Vec<ScoreInfo> = Vec::new();
        for record in exam_attendance_records.iter() {
            let status = match (record.IsAbsent, record.IsExcused) {
                (Some(1), Some(1)) => "請假",
                (Some(1), Some(0)) => "缺考",
                _ => "出席",       // 其他情況
            };
            score_info.push(ScoreInfo{
                student_id:record.StudentID.clone(),
                status:status.to_string(),
                correct_number:record.CorrectAnswersCount ,
                notes: record.Notes.clone(),
            });
        }
        if let Err(_) = session.insert("delete_exam_session_sn", exam_session_sn) {
            return HttpResponse::InternalServerError().body("無法存入 session，請再試一次");
        }
        return HttpResponse::Ok().json(score_info);
    };

    let output_filepath = "./uploads/exam_score_excel.xlsx";
    let workbook = Workbook::new(output_filepath).expect("Failed to create workbook");
    let mut worksheet = workbook.add_worksheet(None).unwrap();

    // 寫入第一行：傳過來的 date 和 exam_type
    worksheet.write_string(0, 0, &format!("考試日期: {}", data.date), None).unwrap();
    worksheet.write_string(0, 1, &format!("考試類別: {}", data.exam_type), None).unwrap();

    // 寫入第二行：標題
    worksheet.write_string(1, 0, "學號", None).unwrap();
    worksheet.write_string(1, 1, "請假/缺考", None).unwrap();
    worksheet.write_string(1, 2, "答對題數", None).unwrap();
    worksheet.write_string(1, 3, "備註", None).unwrap();

    // 寫入資料
    for (i, record) in exam_attendance_records.iter().enumerate() {
        let row = (i + 2) as u32; // 從第三行開始寫入資料

        // 學號
        worksheet.write_string(row, 0, &record.StudentID, None).unwrap();

        // 請假/缺考
        let status = match (record.IsAbsent, record.IsExcused) {
            (Some(1), Some(1)) => "請假",
            (Some(1), Some(0)) => "缺考",
            _ => "無",       // 其他情況
        };
        worksheet.write_string(row, 1, status, None).unwrap();

        // 答對題數
        worksheet.write_number(row, 2, record.CorrectAnswersCount.unwrap() as f64, None).unwrap();

        // 備註
        worksheet.write_string(row, 3, &<std::option::Option<std::string::String> as Clone>::clone(&record.Notes).unwrap_or_default(), None).unwrap();
    }

    // 關閉並保存 Excel 文件
    workbook.close().unwrap();

    match std::fs::read(output_filepath) {
        Ok(file_data) => HttpResponse::Ok()
            .content_type("application/vnd.openxmlformats-officedocument.spreadsheetml.sheet")
            .append_header((
                "Content-Disposition",
                "attachment; filename=exam_score_excel.xlsx",
            ))
            .body(file_data),
        Err(err) => {
            println!("Error reading generated file: {}", err);
            HttpResponse::InternalServerError()
                .body("Failed to generate or retrieve result Excel file")
        }
    }
}