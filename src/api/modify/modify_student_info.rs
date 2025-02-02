use crate::api::lib::is_authorization;
use actix_session::Session;
use actix_web::{post, web, HttpRequest, HttpResponse};
use serde::Deserialize;
use sqlx::{MySqlPool, mysql::MySqlArguments};
use sqlx::Arguments;

#[derive(Deserialize,Debug)]
struct ModifyData {
    name: Option<String>,
    enrollment_status: Option<String>,
    student_attribute: Option<String>,
    notes: Option<String>,
}
#[post("/api/modify_student_info")]
async fn modify_student_info(
    from_data: web::Json<ModifyData>,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session.clone()) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let student_id: Option<String> = session.get("modify_student_id").unwrap();
    let student_id = match student_id {
        Some(id) => id,
        None => return HttpResponse::BadRequest().body("找不到要修改的學生學號，請先點擊查詢後再修改"),
    };
    let original_data: ModifyData = ModifyData{
        name: session.get("modify_name").unwrap(),
        enrollment_status: session.get("modify_enrollment_status").unwrap(),
        student_attribute: session.get("modify_student_attribute").unwrap(),
        notes: session.get("modify_notes").unwrap_or(None),
    };
    let new_data = from_data.into_inner();

    // 輔助函式：將 enrollment_status 轉換成數字
    fn convert_enrollment_status(s: &str) -> Result<i32, HttpResponse> {
        match s {
            "在學" => Ok(1),
            "休學" => Ok(2),
            "退學" => Ok(3),
            _ => Err(HttpResponse::BadRequest().body("註冊狀態請填入在學、休學、退學")),
        }
    }

    // 輔助函式：將 student_attribute 轉換成數字
    fn convert_student_attribute(s: &str) -> Result<i32, HttpResponse> {
        match s {
            "本系" => Ok(1),
            "外系" => Ok(2),
            "外校" => Ok(3),
            _ => Err(HttpResponse::BadRequest().body("學生屬性請填入本系、外系、外校")),
        }
    }

    // 用來動態組合更新的 SQL 語句與參數
    let mut set_clauses = Vec::new();
    let mut query_args = MySqlArguments::default();

    // 處理 name
    if let Some(new_name) = new_data.name {
        if Some(new_name.clone()) != original_data.name {
            set_clauses.push("Name = ?");
            let _= query_args.add(new_name);
        }
    }

    // 處理 enrollment_status：前端是字串，要轉換成數字
    if let Some(new_enroll_str) = new_data.enrollment_status {
        if Some(new_enroll_str.clone()) != original_data.enrollment_status {
            match convert_enrollment_status(&new_enroll_str) {
                Ok(new_enroll_num) => {
                    set_clauses.push("EnrollmentStatus_SN = ?");
                    let _= query_args.add(new_enroll_num);
                }
                Err(err) => {
                    return err;
                }
            }
        }
    }

    // 處理 student_attribute：同樣需要轉換
    if let Some(new_attr_str) = new_data.student_attribute {
        if Some(new_attr_str.clone()) != original_data.student_attribute {
            match convert_student_attribute(&new_attr_str) {
                Ok(new_attr_num) => {
                    set_clauses.push("StudentAttribute_SN = ?");
                    let _= query_args.add(new_attr_num);
                }
                Err(err) => {
                    return err;
                }
            }
        }
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
            let _= query_args.add(new_notes_val);
        }
    }

    // 如果沒有任何欄位有變化，就直接回傳
    if set_clauses.is_empty() {
        return HttpResponse::Ok().body("無更新內容");
    }

    // 組合 SQL 語句
    let set_clause = set_clauses.join(", ");
    let sql = format!("UPDATE StudentInfo SET {} WHERE StudentID = ?", set_clause);
    // 最後將 student_id 當作條件參數加入
    let _= query_args.add(student_id);

    // 執行更新
    let result = sqlx::query_with(&sql, query_args)
        .execute(db_pool.get_ref())
        .await;

    match result {
        Ok(res) => {
            HttpResponse::Ok().body("更新成功")
        },
        Err(e) => {
            HttpResponse::InternalServerError().body("更新失敗")
        }
    }
}