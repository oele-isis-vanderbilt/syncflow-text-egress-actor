use actix::dev::{MessageResponse, OneshotSender};
use actix::prelude::*;
use log;
use serde::{Deserialize, Serialize};
use tokio::spawn;
use uuid::Uuid;

use crate::room_handler::listen_to_room_data_channels;
use tokio::task::{self, AbortHandle};

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

pub struct TextEgressActor {
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_server_url: String,
}

impl TextEgressActor {
    pub fn new(api_key: &str, api_secret: &str, server_url: &str) -> Self {
        TextEgressActor {
            livekit_api_key: api_key.to_string(),
            livekit_api_secret: api_secret.to_string(),
            livekit_server_url: server_url.to_string(),
        }
    }
}

impl Handler<EgressMessages> for TextEgressActor {
    type Result = Result<TextEgressInfo, TextEgressError>;

    fn handle(&mut self, msg: EgressMessages, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            EgressMessages::Start(req) => {
                log::info!("Starting egress for room: {}", &req.room_name);
                let egress_id = Uuid::new_v4().to_string();

                let room_name = req.room_name.clone();
                let topic = req.topic.clone(); // Assuming this is cloneable. Adjust if it's an Option<String> or similar.
                let livekit_api_key = self.livekit_api_key.clone();
                let livekit_api_secret = self.livekit_api_secret.clone();
                let livekit_server_url = self.livekit_server_url.clone();

                let handle = ctx.spawn(actix::fut::wrap_future(async move {
                    let _ = listen_to_room_data_channels(
                        &room_name,
                        topic,
                        &livekit_api_key,
                        &livekit_api_secret,
                        &livekit_server_url,
                    )
                    .await;
                }));

                Ok(TextEgressInfo {
                    egress_id,
                    room_name: req.room_name,
                    topic: None,
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
