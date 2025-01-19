use actix_web::HttpRequest;
use actix_session::Session;
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