use actix::Actor;
use actix_web::HttpServer;
use actix_web::{get, post, web};
use livekit_text_egress_actor::text_egress_actor::{
    EgressMessages, HTTPOnlyMessages, TextEgressActor, TextEgressInfoRequest, TextEgressRequest,
};
use livekit_text_egress_actor::utils::load_env;
use std::env;
use actix_files as fs;

#[get("/egresses")]
async fn get_egresses(actor: web::Data<actix::Addr<TextEgressActor>>) -> actix_web::HttpResponse {
    let res = actor.send(HTTPOnlyMessages::ListEgresses).await;
    match res {
        Ok(res) => match res {
            Ok(res) => actix_web::HttpResponse::Ok().json(res),
            Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
        },
        Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[post("/egress")]
async fn create_egress(
    actor: web::Data<actix::Addr<TextEgressActor>>,
    req: web::Json<TextEgressRequest>,
) -> actix_web::HttpResponse {
    let res = actor.send(EgressMessages::Start(req.into_inner())).await;
    match res {
        Ok(res) => match res {
            Ok(res) => actix_web::HttpResponse::Ok().json(res),
            Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
        },
        Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[get("/egress/{egress_id}")]
async fn get_egress(
    actor: web::Data<actix::Addr<TextEgressActor>>,
    egress_id: web::Path<String>,
) -> actix_web::HttpResponse {
    let egress_id = egress_id.into_inner();
    let res = actor
        .send(EgressMessages::Info(TextEgressInfoRequest { egress_id }))
        .await;

    match res {
        Ok(res) => match res {
            Ok(res) => actix_web::HttpResponse::Ok().json(res),
            Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
        },
        Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[post("/stop-egress/{egress_id}")]
async fn stop_egress(
    actor: web::Data<actix::Addr<TextEgressActor>>,
    egress_id: web::Path<String>,
) -> actix_web::HttpResponse {
    let egress_id = egress_id.into_inner();
    let res = actor
        .send(EgressMessages::Stop(TextEgressInfoRequest { egress_id }))
        .await;

    match res {
        Ok(res) => match res {
            Ok(res) => actix_web::HttpResponse::Ok().json(res),
            Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
        },
        Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[post("/delete-egress-records")]
async fn delete_egress_records(
    actor: web::Data<actix::Addr<TextEgressActor>>,
) -> actix_web::HttpResponse {
    let res = actor.send(HTTPOnlyMessages::DeleteRecords).await;
    match res {
        Ok(res) => match res {
            Ok(res) => actix_web::HttpResponse::Ok().json(res),
            Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
        },
        Err(e) => actix_web::HttpResponse::InternalServerError().json(e.to_string()),
    }
}

#[actix_rt::main]
async fn main() -> std::io::Result<()> {
    load_env();
    env::set_var(
        "RUST_LOG",
        "actix_web=debug,actix_rt=debug,livekit_text_egress_actor=debug",
    );
    env_logger::init();

    let api_key = env::var("LIVEKIT_API_KEY").expect("LIVEKIT_API_KEY must be set");
    let api_secret = env::var("LIVEKIT_API_SECRET").expect("LIVEKIT_API_SECRET must be set");
    let server_url = env::var("LIVEKIT_SERVER_URL").expect("LIVEKIT_SERVER_URL must be set");
    let app_host = env::var("APP_HOST").expect("APP_HOST must be set");
    let app_port = env::var("APP_PORT").expect("APP_PORT must be set");

    let server_addr = format!("{}:{}", app_host, app_port);

    let actor = TextEgressActor::new(&api_key, &api_secret, &server_url).start();

    HttpServer::new(move || {
        actix_web::App::new()
            .app_data(web::Data::new(actor.clone()))
            .wrap(actix_web::middleware::Logger::default())
            .service(get_egresses)
            .service(create_egress)
            .service(get_egress)
            .service(stop_egress)
            .service(delete_egress_records)
            .service(fs::Files::new("/css", "./static/css").show_files_listing())
            .service(fs::Files::new("/js", "./static/js").show_files_listing())
            .service(fs::Files::new("/webapp", "./static").index_file("index.html"))
    })
    .workers(4)
    .bind(server_addr)?
    .run()
    .await
}
