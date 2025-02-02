use actix_web::{post, web, HttpResponse, HttpRequest};
use actix_session::Session;
use actix_multipart::Multipart;
use sqlx::MySqlPool;
use crate::api::lib::is_authorization;
use std::fs::File;
use std::io::Write;
use calamine::DataType;
use calamine::Reader;
use futures_util::StreamExt as _;

#[post("/api/mutiple_add_student_info")]
pub async fn mutiple_add_student_info(
    mut payload: Multipart,
    req: HttpRequest,
    session: Session,
    db_pool: web::Data<MySqlPool>,
) -> HttpResponse {
    if !is_authorization(req, session) {
        return HttpResponse::Unauthorized().body("Session 無效或過期，或是無效的 CSRF Token");
    }
    let temp_filepath = "./uploads/students_info.xlsx";
    //儲存上傳的檔案
    while let Some(Ok(field)) = payload.next().await {
        let content_disposition = field.content_disposition();
        if let Some(filename) = content_disposition.and_then(|cd| cd.get_filename()) {
            let file_ext = std::path::Path::new(filename)
                .extension()
                .and_then(|s| s.to_str());
            if file_ext != Some("xlsx") {
                return HttpResponse::BadRequest().body("請上傳xlsx檔案");
            }
            let mut f = File::create(temp_filepath).expect("Failed to create file");
            let mut field_stream = field;
            while let Some(chunk) = field_stream.next().await {
                let data = chunk.expect("Error reading chunk");
                f.write_all(&data).expect("Error writing chunk");
            }
        }
    }
    if !std::path::Path::new(temp_filepath).exists() {
        println!("File does not exist: {}", temp_filepath);
        return HttpResponse::InternalServerError().body("Failed to process file");
    }
    //解析上傳的檔案，取得學號的list
    let mut workbook = match calamine::open_workbook_auto(temp_filepath) {
        Ok(wb) => wb,
        Err(err) => {
            println!("Failed to open Excel file: {}", err);
            return HttpResponse::InternalServerError().body("無效的 Excel file");
        }
    };
    let range = match workbook.worksheet_range("工作表1") {
        Ok(range) => range,
        Err(err) => {
            println!("Error reading sheet: {}", err);
            return HttpResponse::BadRequest().body("請將需要查詢的資料放入工作表1");
        }
    };
    let header_row = range.rows().next().unwrap();
    let expected_headers = vec!["學號", "姓名", "註冊狀況(只能填在學、休學、退學)", "學生屬性(只能填本系、外系、外校)", "備註"];
    let actual_headers: Vec<String> = header_row.iter()
    .map(|cell| cell.get_string().unwrap_or("").to_string())
    .collect();

    if actual_headers != expected_headers {
        println!("標題不正確: {:?}", actual_headers);
        return HttpResponse::BadRequest().body("標題與預期不符，請檢查工作表格式");
    }
    let query = r#"
        INSERT INTO StudentInfo (
            StudentID,
            Name,
            EnrollmentStatus_SN,
            StudentAttribute_SN,
            Notes
        ) VALUES (?, ?, ?, ?, ?)
    "#;
    for row in range.rows().skip(1) {
        let student_id = match row.get(0) {
            Some(id) => {
                if let Some(id) = id.get_string() {
                    id.to_ascii_uppercase()
                }else {
                    return HttpResponse::BadRequest().body("學號欄位不能為空");
                }
            }
            None => {
                return HttpResponse::InternalServerError().body("server讀取excel錯誤");
            }
        };
        let name = match row.get(1) {
            Some(name) => {
                if let Some(name) = name.get_string() {
                    name
                }else {
                    return HttpResponse::BadRequest().body("姓名欄位不能為空");
                }
            }
            None => {
                return HttpResponse::InternalServerError().body("server讀取excel錯誤");
            }
        };
        //之後記得改成動態調整，因為資料庫可能會出現第4個狀態(先讀資料庫在做匹配)
        let es_sn = match row.get(2) {
            Some(status) => {
                if let Some(status) = status.get_string() {
                    match  status {
                        "在學" => 1,
                        "休學" => 2,
                        "退學" => 3,
                        _=> return HttpResponse::BadRequest().body("註冊狀況欄位只能填入在學、休學、退學"),
                    }
                }else{
                    return HttpResponse::BadRequest().body("註冊狀況欄位不能為空");
                }
            },
            None => {
                return HttpResponse::InternalServerError().body("server讀取excel錯誤");
            }
        };
        //之後記得改成動態調整，因為資料庫可能會出現第4個屬性(先讀資料庫在做匹配)
        let sa_sn = match row.get(3) {
            Some(attribute) => {
                if let Some(attribute) = attribute.get_string() {
                    match attribute {
                        "本系" => 1,
                        "外系" => 2,
                        "外校" => 3,
                        _=> return HttpResponse::BadRequest().body("學生屬性欄位只能填入在學、休學、退學"),
                    }
                }else {
                    return HttpResponse::BadRequest().body("學生屬性欄位不能為空");
                }
            },
            None => {
                return HttpResponse::InternalServerError().body("server讀取excel錯誤");
            }
        };
        let note = match row.get(4) {
            Some(note) => note.get_string().unwrap_or(""),
            None => {
                return HttpResponse::InternalServerError().body("server讀取excel錯誤");
            }
        };
        match sqlx::query(query)
            .bind(&student_id)
            .bind(name)
            .bind(es_sn)
            .bind(sa_sn)
            .bind(note)
            .execute(db_pool.get_ref())
            .await 
        {
            Ok(_) => (),
            Err(sqlx::Error::Database(err)) if err.code() == Some(std::borrow::Cow::Borrowed("23000")) => {
                return HttpResponse::Conflict().body(format!("學號:{}，已經被新增過",&student_id));
            }
            Err(err) => {
                return HttpResponse::InternalServerError().body(format!("Internal server error.: {}", err));
            }
        }
    }
    HttpResponse::Ok().body("成功新增學生資料")
}