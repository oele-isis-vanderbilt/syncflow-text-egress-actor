use actix::prelude::*;
use log;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::error_messages::TextEgressError;
use crate::room_listener_actor::{DataEgressResultFiles, RoomListenerActor, RoomListenerMessages};
use crate::s3_uploader_actor::{S3UploaderActor, S3UploaderMessages};

#[derive(Message, Debug, Clone)]
#[rtype(result = "Result<TextEgressInfo, TextEgressError>")]
pub enum EgressMessages {
    Start(TextEgressRequest),
    Stop(TextEgressInfoRequest),
    Info(TextEgressInfoRequest),
}

#[derive(Message, Debug)]
#[rtype(result = "Result<HashMap<String, TextEgressInfo>, TextEgressError>")]
pub enum HTTPOnlyMessages {
    ListEgresses,
    DeleteRecords,
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
pub(crate) enum RoomListenerUpdates {
    Started {
        egress_id: String,
        room_name: String,
        topic: Option<String>,
        files: Vec<DataEgressResultFiles>,
    },
    Updated {
        egress_id: String,
        room_name: String,
        topic: Option<String>,
        files: Vec<DataEgressResultFiles>,
    },
    Failed {
        egress_id: String,
        error: TextEgressError,
    },
    Stopped {
        egress_id: String,
        room_name: String,
        topic: Option<String>,
        files: Vec<DataEgressResultFiles>,
    },
}

#[derive(Message, Debug)]
#[rtype(result = "()")]
#[allow(dead_code)]
pub(crate) enum S3UploaderUpdates {
    Started {
        egress_id: String,
        files: Vec<String>,
        bucket: String,
    },
    Completed {
        egress_id: String,
        files: Vec<String>,
        bucket: String,
    },
    Failed {
        egress_id: String,
        error: TextEgressError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressRequest {
    pub room_name: String,
    pub topic: Option<String>,
    pub s3_bucket_name: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub s3_region: String,
    pub s3_endpoint: String,
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
    pub started_at: Option<usize>,
    pub stopped_at: Option<usize>,
    #[serde(skip_serializing, default, skip_deserializing)]
    pub files: Vec<DataEgressResultFiles>,
    pub error: Option<String>,
    pub status: TextEgressStatus,
    pub paths: Vec<String>,
    pub s3_bucket_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextEgressStatus {
    Started,
    Stopped,
    Failed,
    Starting,
    Stopping,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Credentials {
    pub access_key: String,
    pub secret_key: String,
    pub region: String,
    pub endpoint: String,
    pub bucket: String,
}

pub struct TextEgressActor {
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_server_url: String,
    room_listener_actors: HashMap<String, Addr<RoomListenerActor>>,
    s3_uploader_actors: Arc<Mutex<HashMap<String, Addr<S3UploaderActor>>>>,
    active_egresses: Arc<Mutex<HashMap<String, TextEgressInfo>>>,
    credentials: HashMap<String, S3Credentials>,
}

impl TextEgressActor {
    pub fn new(api_key: &str, api_secret: &str, server_url: &str) -> Self {
        TextEgressActor {
            livekit_api_key: api_key.to_string(),
            livekit_api_secret: api_secret.to_string(),
            livekit_server_url: server_url.to_string(),
            room_listener_actors: HashMap::new(),
            s3_uploader_actors: Arc::new(Mutex::new(HashMap::new())),
            active_egresses: Arc::new(Mutex::new(HashMap::new())),
            credentials: HashMap::new(),
        }
    }
}

impl Handler<EgressMessages> for TextEgressActor {
    type Result = ResponseActFuture<Self, Result<TextEgressInfo, TextEgressError>>;

    fn handle(&mut self, msg: EgressMessages, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            EgressMessages::Start(req) => {
                log::info!("Starting egress for room: {}", &req.room_name);
                let egress_id = Uuid::new_v4().to_string();

                let room_name = req.room_name.clone();
                let topic = req.topic.clone();
                let livekit_api_key = self.livekit_api_key.clone();
                let livekit_api_secret = self.livekit_api_secret.clone();
                let livekit_server_url = self.livekit_server_url.clone();

                let room_listener_actor = RoomListenerActor::new(
                    &egress_id,
                    &livekit_api_key,
                    &livekit_api_secret,
                    &livekit_server_url,
                    ctx.address().clone(),
                );

                let actor_addr = room_listener_actor.start();
                actor_addr.do_send(RoomListenerMessages::StartListening {
                    room_name: room_name.clone(),
                    topic: topic.clone(),
                });

                self.room_listener_actors
                    .insert(egress_id.clone(), actor_addr);

                let credentials = S3Credentials {
                    access_key: req.s3_access_key.clone(),
                    secret_key: req.s3_secret_key.clone(),
                    region: req.s3_region.clone(),
                    endpoint: req.s3_endpoint.clone(),
                    bucket: req.s3_bucket_name.clone(),
                };
                self.credentials.insert(egress_id.clone(), credentials);

                let fut = async move {
                    Ok(TextEgressInfo {
                        egress_id,
                        room_name,
                        topic,
                        started_at: None,
                        stopped_at: None,
                        files: vec![],
                        error: None,
                        status: TextEgressStatus::Starting,
                        paths: vec![],
                        s3_bucket_name: None,
                    })
                };

                Box::pin(fut.into_actor(self))
            }
            EgressMessages::Stop(req) => {
                let actor = self.room_listener_actors.get(&req.egress_id).map_or_else(
                    || {
                        return Err(TextEgressError::EgressNotFound(req.egress_id.clone()));
                    },
                    |actor| Ok(actor),
                );

                if let Err(e) = actor {
                    return Box::pin(async { Err(e) }.into_actor(self));
                }
                let actor = actor.unwrap();

                actor.do_send(RoomListenerMessages::StopListening);
                let active_egresses_arc = self.active_egresses.clone();

                let fut = async move {
                    let active_egresses = active_egresses_arc.lock().await;
                    active_egresses.get(&req.egress_id).map_or_else(
                        || Err(TextEgressError::EgressNotFound(req.egress_id.clone())),
                        |active_egress| {
                            let active_egress = active_egress.clone();
                            Ok(active_egress)
                        },
                    )
                };

                Box::pin(fut.into_actor(self))
            }
            EgressMessages::Info(req) => {
                log::info!("Getting info for egress: {}", req.egress_id);
                let active_egresses_arc = self.active_egresses.clone();

                let fut = async move {
                    let active_egresses = active_egresses_arc.lock().await;
                    log::info!("Active egresses: {:?}", &active_egresses);
                    active_egresses.get(&req.egress_id).map_or_else(
                        || Err(TextEgressError::EgressNotFound(req.egress_id.clone())),
                        |active_egress| {
                            let active_egress = active_egress.clone();
                            Ok(active_egress)
                        },
                    )
                };
                Box::pin(fut.into_actor(self))
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

impl Handler<RoomListenerUpdates> for TextEgressActor {
    type Result = ();
    fn handle(&mut self, msg: RoomListenerUpdates, ctx: &mut Self::Context) -> Self::Result {
        match msg {
            RoomListenerUpdates::Started {
                room_name,
                topic,
                egress_id,
                files,
            } => {
                let active_egresses_arc = self.active_egresses.clone();

                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    active_egresses.insert(
                        egress_id.clone(),
                        TextEgressInfo {
                            files,
                            egress_id,
                            status: TextEgressStatus::Started,
                            started_at: Some(chrono::Utc::now().timestamp() as usize),
                            stopped_at: None,
                            room_name,
                            topic,
                            error: None,
                            paths: vec![],
                            s3_bucket_name: None,
                        },
                    );
                });
            }
            RoomListenerUpdates::Updated {
                room_name,
                topic,
                egress_id,
                files,
            } => {
                let active_egresses_arc = self.active_egresses.clone();
                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let existing = active_egresses.get_mut(&egress_id);
                    if let Some(active_egress) = existing {
                        active_egress.files = files;
                        active_egress.topic = topic;
                        active_egress.room_name = room_name;
                    }
                });
            }
            RoomListenerUpdates::Failed { error, egress_id } => {
                let active_egresses_arc = self.active_egresses.clone();

                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let existing = active_egresses.get_mut(&egress_id);
                    if let Some(active_egress) = existing {
                        active_egress.files = vec![];
                        active_egress.error = Some(error.to_string());
                        active_egress.status = TextEgressStatus::Failed;
                    }
                });
            }
            RoomListenerUpdates::Stopped {
                files,
                egress_id,
                room_name,
                topic,
            } => {
                let active_egresses_arc = self.active_egresses.clone();
                let s3_credentials = self.credentials.get(&egress_id).unwrap().clone();
                let s3_uploaders_arc = self.s3_uploader_actors.clone();
                let addr = ctx.address().clone();

                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let mut s3_uploaders = s3_uploaders_arc.lock().await;
                    let existing = active_egresses.get_mut(&egress_id);
                    if let Some(active_egress) = existing {
                        active_egress.files = files;
                        active_egress.room_name = room_name;
                        active_egress.topic = topic;
                        active_egress.stopped_at = Some(chrono::Utc::now().timestamp() as usize);
                        active_egress.status = TextEgressStatus::Stopped;

                        let s3_uploader = S3UploaderActor::new(
                            &egress_id,
                            &s3_credentials.bucket,
                            &s3_credentials.access_key,
                            &s3_credentials.secret_key,
                            &s3_credentials.region,
                            &s3_credentials.endpoint,
                            addr,
                        );
                        let s3_uploader_addr = s3_uploader.start();

                        let files = active_egress
                            .files
                            .iter()
                            .map(|f| f.file_path.clone())
                            .collect();

                        let prefix = format!(
                            "{}/{}/{}/egressId({})-{}",
                            &active_egress.room_name,
                            "text-egress",
                            &active_egress
                                .topic
                                .clone()
                                .unwrap_or_else(|| "all-topics".to_string()),
                            &egress_id,
                            chrono::DateTime::from_timestamp(
                                active_egress.started_at.unwrap_or_default() as i64,
                                0
                            )
                            .unwrap_or_default()
                            .format("%Y-%m-%dT%H:%M:%S%Z"),
                        );

                        s3_uploader_addr.do_send(S3UploaderMessages::Start { prefix, files });
                        s3_uploaders.insert(egress_id.clone(), s3_uploader_addr);
                    }
                });
            }
        }
    }
}

impl Handler<S3UploaderUpdates> for TextEgressActor {
    type Result = ();

    fn handle(&mut self, msg: S3UploaderUpdates, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            S3UploaderUpdates::Started {
                egress_id,
                files,
                bucket,
            } => {
                let active_egresses_arc = self.active_egresses.clone();
                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let existing = active_egresses.get_mut(&egress_id);
                    if let Some(active_egress) = existing {
                        active_egress.paths = files;
                        active_egress.s3_bucket_name = Some(bucket);
                    }
                });
            }
            S3UploaderUpdates::Completed {
                egress_id,
                files,
                bucket,
            } => {
                let active_egresses_arc = self.active_egresses.clone();
                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let existing = active_egresses.get_mut(&egress_id);
                    if let Some(active_egress) = existing {
                        active_egress.paths = files;
                        active_egress.s3_bucket_name = Some(bucket);
                        active_egress.status = TextEgressStatus::Complete;
                    }
                });
            }
            S3UploaderUpdates::Failed { egress_id, error } => {
                let active_egresses_arc = self.active_egresses.clone();
                actix::spawn(async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let existing = active_egresses.get_mut(&egress_id);
                    if let Some(active_egress) = existing {
                        active_egress.error = Some(error.to_string());
                        active_egress.status = TextEgressStatus::Failed;
                    }
                });
            }
        }
    }
}

impl Handler<HTTPOnlyMessages> for TextEgressActor {
    type Result = ResponseActFuture<Self, Result<HashMap<String, TextEgressInfo>, TextEgressError>>;

    fn handle(&mut self, msg: HTTPOnlyMessages, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            HTTPOnlyMessages::ListEgresses => {
                let active_egresses_arc = self.active_egresses.clone();
                let fut = async move {
                    let active_egresses = active_egresses_arc.lock().await;
                    Ok(active_egresses.clone())
                };
                Box::pin(fut.into_actor(self))
            }
            HTTPOnlyMessages::DeleteRecords => {
                let active_egresses_arc = self.active_egresses.clone();
                let fut = async move {
                    let mut active_egresses = active_egresses_arc.lock().await;
                    let cloned_map = active_egresses.clone();
                    active_egresses.clear();
                    Ok(cloned_map)
                };
                Box::pin(fut.into_actor(self))
            }
        }
    }
}
