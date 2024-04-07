use actix::Actor;
use livekit_text_egress_actor::text_egress_actor::{
    EgressMessages, TextEgressActor, TextEgressRequest,
};
use std::env;

#[actix_rt::main]
async fn main() {
    env::set_var("RUST_LOG", "actix_rt=debug,livekit_text_egress_actor=debug");
    env_logger::init();

    let api_key = "APIfJENwGJaE6DD".to_string();
    let api_secret = "cmPbeYxmovNQ4zb7HnABS1eKF4GSm0PX9MTtci0fFi7A".to_string();
    let server_url = "ws://localhost:7880".to_string();

    let actor = TextEgressActor::new(&api_key, &api_secret, &server_url).start();

    let req = EgressMessages::Start(TextEgressRequest {
        room_name: "LiveKitELP_tzxcci".to_string(),
        topic: Some("topic".to_string()),
    });

    let res = actor.send(req).await.unwrap();
    println!("Response: {:?}", res);

    // Sleep for a while to allow the actor to process the request
    actix_rt::time::sleep(std::time::Duration::from_secs(500)).await;
}
