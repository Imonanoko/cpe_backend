use actix_web::{post, web, HttpResponse, HttpRequest};
use actix_session::Session;
use crate::api::lib::is_authorization;
use serde::Deserialize;
use sqlx::MySqlPool;
use sqlx::Row;
use chrono::NaiveDate;

#[derive(Deserialize, Debug)]
struct AddExamScore {
    session: String,
    #[serde(rename = "studentID")]
    student_id: String,
    num: String,
    notes: String,
}
#[post("/api/single_add_exam_score")]
async fn single_add_exam_score(
    data: web::Json<AddExamScore>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let split = data.session.split(",").collect::<Vec<&str>>();
    let date = match NaiveDate::parse_from_str(split[0], "%Y-%m-%d") {
        Ok(date) => date,
        Err(_) => {
            return HttpResponse::BadRequest().body("日期格式錯誤，請將第二欄(column)以後的標題格式改為YYYY-MM-DD,(官辦、自辦)");
        }
    };
    let exam_type = match split.get(1) {
        Some(exam_type) => {
            let exam_type_str = exam_type.to_string();
            if exam_type_str == "官辦" || exam_type_str == "自辦"{
                exam_type_str
            }else {
                return HttpResponse::BadRequest().body("場次種類格式錯誤，請將偶數欄(column)的標題格式改為YYYY-MM-DD,(官辦、自辦)");
            }
        }
        None => {
            return HttpResponse::BadRequest().body("場次種類格式錯誤，請將偶數欄(column)以後的標題格式改為YYYY-MM-DD,(官辦、自辦)");
        }
    };
    let query = r#"
        select SN from ExamSessions where ExamDate= (?) and ExamType= (?);
    "#;
    let row = match sqlx::query(query).bind(&date).bind(&exam_type).fetch_one(db_pool.get_ref()).await {
        Ok(row) => row,
        Err(sqlx::Error::RowNotFound) => {
            return HttpResponse::BadRequest().body(format!("日期:{}, 場次種類:{}，找不到該場次的資料。請先新增或檢查該場次的資料", date, exam_type));
        }
        Err(err) => {
            return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
        }
    };
    let exam_session_sn: i32 = row.try_get("SN").unwrap();
    let id = data.student_id.to_ascii_uppercase();
    let mut absent = false;
    let mut excused = false;
    let mut score = 0;
    let note = data.notes.to_string();
    if data.num == "請假" {
        absent = true;
        excused = true;
        score = 0;
    } else if data.num == "缺考" {
        absent = true;
        score = 0;
    }else {
        score = match data.num.parse::<i32>() {
            Ok(num) => num,
            Err(_) => {
                return HttpResponse::BadRequest().body("題數格式錯誤，請填入題數(整數)或請假、缺考");
            }
        }
    }
    let query = r#"
                INSERT INTO ExamAttendance (ExamSession_SN, StudentID, IsAbsent, IsExcused, CorrectAnswersCount, Notes)
                VALUES (?, ?, ?, ?, ?, ?);
            "#;

    match sqlx::query(query)
        .bind(exam_session_sn)
        .bind(&id)
        .bind(absent)
        .bind(excused)
        .bind(score)
        .bind(&note)
        .execute(db_pool.get_ref())
        .await
    {
        Ok(_) => (),
        Err(sqlx::Error::Database(err)) if err.is_unique_violation() => {
            return HttpResponse::BadRequest().body("此場次的學號已經被新增過了，欲新增此成績請使用修改功能。");
        }
        Err(sqlx::Error::Database(err)) if err.code() == Some(std::borrow::Cow::Borrowed("23000")) => {
            return HttpResponse::Conflict().body("學生資訊無此學號，請先新增這個學號再新增此成績。");
        }
        Err(err) => {
            return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
        }
    }
    HttpResponse::Ok().body("")
}