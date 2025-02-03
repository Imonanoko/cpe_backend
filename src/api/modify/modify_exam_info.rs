use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::Deserialize;
use sqlx::{MySqlPool, mysql::MySqlArguments};
use sqlx::Arguments;

#[derive(Deserialize,Debug)]
struct ModifyData {
    exam_date: Option<String>,
    exam_type: Option<String>,
    notes: Option<String>,
}


#[post("/api/modify_exam_info")]
async fn modify_exam_info(
    req: HttpRequest,
    mut session: Session,
    db_pool: web::Data<MySqlPool>,
    data: web::Json<ModifyData>,
) -> HttpResponse {
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let exam_sn: Option<i32> = session.get("modify_exam_sn").unwrap();
    let exam_sn = match exam_sn {
        Some(sn) => sn,
        None => return HttpResponse::BadRequest().body("找不到要修改的考試 SN，請先點擊查詢後再修改"),
    };
    let original_data: ModifyData = ModifyData {
        exam_date: session.get("modify_exam_date").unwrap(),
        exam_type: session.get("modify_exam_type").unwrap(),
        notes: session.get("modify_notes").unwrap_or(None),
    };
    let new_data = data.into_inner();
    let mut set_clauses = Vec::new();
    let mut query_args = MySqlArguments::default();

    // 處理 exam_date
    if new_data.exam_date != original_data.exam_date {
        set_clauses.push("ExamDate = ?");
        let _ = query_args.add(new_data.exam_date);
    }

    // 處理 exam_type：前端是字串，要轉換成數字
    if new_data.exam_type != original_data.exam_type {
        set_clauses.push("ExamType = ?");
        let _ = query_args.add(new_data.exam_type);
    }

    // 處理 notes
    if let Some(new_notes) = new_data.notes {
        let new_notes_val = if new_notes.trim().is_empty() {
            None
        } else {
            Some(new_notes)
        };

        if new_notes_val != original_data.notes {
            set_clauses.push("Notes = ?");
            let _ = query_args.add(new_notes_val);
        }
    }

    // 如果沒有任何欄位有變化，就直接回傳
    if set_clauses.is_empty() {
        clean_session(&mut session);
        return HttpResponse::Ok().body("無更新內容");
    }

    // 組合 SQL 語句
    let set_clause = set_clauses.join(", ");
    let sql = format!("UPDATE ExamSessions SET {} WHERE SN = ?", set_clause);
    // 最後將 exam_sn 當作條件參數加入
    let _ = query_args.add(exam_sn);

    // 執行更新
    let result = sqlx::query_with(&sql, query_args)
        .execute(db_pool.get_ref())
        .await;

    match result {
        Ok(_res) => {
            clean_session(&mut session);
            HttpResponse::Ok().body("更新成功")
        }
        Err(_e) => {
            HttpResponse::InternalServerError().body("更新失敗")
        }
    }
}

fn clean_session(session: &mut Session) {
    session.remove("modify_exam_sn");
    session.remove("modify_exam_date");
    session.remove("modify_exam_type");
    session.remove("modify_notes");
}