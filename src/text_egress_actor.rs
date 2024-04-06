use actix::dev::{MessageResponse, OneshotSender};
use actix::prelude::*;
use log;
use serde::{Deserialize, Serialize};

#[derive(Message)]
#[rtype(result = "Result<TextEgressInfo, TextEgressError>")]
pub enum EgressMessages {
    Start(TextEgressRequest),
    Stop(TextEgressInfoRequest),
    Info(TextEgressInfoRequest),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressRequest {
    pub room_name: String,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressInfoRequest {
    pub egress_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressInfo {
    pub egress_id: String,
    pub room_name: String,
    pub topic: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressError;

pub struct TextEgressActor;

impl Handler<EgressMessages> for TextEgressActor {
    type Result = Result<TextEgressInfo, TextEgressError>;

    fn handle(&mut self, msg: EgressMessages, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            EgressMessages::Start(req) => {
                log::info!("Starting egress for room: {}", req.room_name);
                Ok(TextEgressInfo {
                    egress_id: "egress_id".to_string(),
                    room_name: req.room_name,
                    topic: req.topic,
                    active: true,
                })
            }
            EgressMessages::Stop(req) => {
                log::info!("Stopping egress for room: {}", req.egress_id);
                Ok(TextEgressInfo {
                    egress_id: req.egress_id,
                    room_name: "room_name".to_string(),
                    topic: None,
                    active: false,
                })
            }
            EgressMessages::Info(req) => {
                log::info!("Getting info for egress: {}", req.egress_id);
                Ok(TextEgressInfo {
                    egress_id: req.egress_id,
                    room_name: "room_name".to_string(),
                    topic: None,
                    active: true,
                })
            }
        }
    }
}

impl Actor for TextEgressActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("TextEgressActor started");
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("TextEgressActor stopped");
    }
}
