use actix_web::{ post, HttpResponse, Responder};

#[post("/api/login")]
async fn login() -> impl Responder {
    HttpResponse::Ok().body("test web success.")
}