use actix_web::{get, HttpRequest, HttpResponse, Responder};
use actix_session::Session;

#[get("/api/check_session")]
async fn check_session(req: HttpRequest, session: Session) -> impl Responder {
    // 從請求頭中提取 CSRF Token
    let csrf_token_header = req
        .headers()
        .get("X-CSRF-Token")
        .and_then(|header| header.to_str().ok());
    // 從 Session 中獲取 CSRF Token
    let csrf_token_session: Option<String> = session.get("csrf_token").unwrap_or(None);
    // 驗證 Token 是否匹配
    if let (Some(header_token), Some(session_token)) = (csrf_token_header, csrf_token_session) {
        // println!("header_token: {}, session_token: {}", header_token, session_token);
        if header_token == session_token {
            if let Some(is_logged_in) = session.get::<bool>("is_logged_in").unwrap_or(None) {
                if is_logged_in {
                    return HttpResponse::Ok().body("User is logged in.");
                }
            }
            return HttpResponse::Unauthorized().body("User is not logged in.");
        }
    }
    HttpResponse::Forbidden().body("Invalid CSRF token.")
}
