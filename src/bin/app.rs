use actix::Actor;
use livekit_text_egress_actor::text_egress_actor::{
    EgressMessages, TextEgressActor, TextEgressInfo, TextEgressRequest,
};
use std::env;

#[actix_rt::main]
async fn main() {
    env::set_var("RUST_LOG", "actix_rt=debug,livekit_text_egress_actor=debug");

    env_logger::init();
    let actor = TextEgressActor.start();

    let req = EgressMessages::Start(TextEgressRequest {
        room_name: "room_name".to_string(),
        topic: Some("topic".to_string()),
    });

    let res = actor.send(req).await.unwrap();
    println!("Response: {:?}", res);
}
