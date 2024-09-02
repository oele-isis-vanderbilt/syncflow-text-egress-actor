use crate::error_messages::TextEgressError;
use crate::text_egress_actor::{RoomListenerUpdates, TextEgressActor};
use actix::{Actor, Addr, AsyncContext, Context, Handler, Message};
use livekit::{Room, RoomEvent};
use livekit_api::access_token::{AccessToken, VideoGrants};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::fs::OpenOptions;
use std::path::PathBuf;
use tempdir::TempDir;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tokio::sync::{
    mpsc,
    oneshot::{channel, Receiver as OneshotReceiver, Sender},
};

// FixMe: This needs a proper refactoring for various reasons:
// 1. The listen to room events function is too long/complex
// 2. The use of oneshot channels with tokio select is not idiomatic to actix actors
// 3. There are various points of improvement in the code

#[derive(Message, Debug, Clone)]
#[rtype(result = "()")]
pub enum RoomListenerMessages {
    StartListening {
        room_name: String,
        topic: Option<String>,
    },
    StopListening,
}

fn get_egress_token(
    room_name: &str,
    api_key: &str,
    api_secret: &str,
) -> Result<String, TextEgressError> {
    let video_grants = VideoGrants {
        room: room_name.to_string(),
        hidden: true,
        can_subscribe: true,
        can_publish: false,
        room_join: true,
        room_create: false,
        ..Default::default()
    };

    let access_token = AccessToken::with_api_key(api_key, api_secret)
        .with_name("TEXT-EGRESS-BOT")
        .with_identity("TEXT-EGRESS-BOT")
        .with_grants(video_grants);

    access_token.to_jwt().map_err(|e| e.into())
}

pub(crate) async fn join_room(
    server_url: &str,
    access_token_str: &str,
) -> Result<(Room, mpsc::UnboundedReceiver<RoomEvent>), TextEgressError> {
    #[allow(unused_mut)]
    let (room, mut room_events) =
        Room::connect(server_url, access_token_str, Default::default()).await?;

    Ok((room, room_events))
}

pub async fn create_file(
    identity: &str,
    root: &PathBuf,
) -> Result<(File, String), TextEgressError> {
    let file_name = format!("{}.txt", identity);
    let handler = OpenOptions::new().create(true).append(true).open(root.join(&file_name)).await?;
    let filename_with_path = root.join(&file_name).to_str().unwrap().to_string();
    Ok((handler, filename_with_path))
}

pub struct RoomListenerActor {
    pub egress_id: String,
    pub livekit_api_key: String,
    pub livekit_api_secret: String,
    pub livekit_server_url: String,
    pub parent_addr: Addr<TextEgressActor>,
    cancel_sender: Option<Sender<()>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataEgressResultFiles {
    pub participant: String,
    pub file_path: String,
}

#[derive(Debug)]
pub struct FileHandler {
    pub file: File,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextEgressMetadata {
    pub room_name: String,
    pub topic: Option<String>,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

impl Actor for RoomListenerActor {
    type Context = Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!(
            "RoomListenerActor started for egress_id: {:?}",
            self.egress_id
        );
    }
}

impl RoomListenerActor {
    pub fn new(
        egress_id: &str,
        api_key: &str,
        api_secret: &str,
        server_url: &str,
        parent_addr: Addr<TextEgressActor>,
    ) -> Self {
        RoomListenerActor {
            egress_id: egress_id.to_string(),
            livekit_api_key: api_key.to_string(),
            livekit_api_secret: api_secret.to_string(),
            livekit_server_url: server_url.to_string(),
            parent_addr,
            cancel_sender: None,
        }
    }
}

impl Handler<RoomListenerMessages> for RoomListenerActor {
    type Result = ();

    fn handle(&mut self, msg: RoomListenerMessages, ctx: &mut Self::Context) -> Self::Result {
        log::info!("Received message: {:?}", &msg);
        match msg {
            RoomListenerMessages::StartListening { room_name, topic } => {
                log::info!(
                    "Starting to listen to room data channels for room: {:?}",
                    room_name
                );
                let rname = room_name.clone();
                let topic = topic.clone();
                let egress_id = self.egress_id.clone();
                let api_key = self.livekit_api_key.clone();
                let api_secret = self.livekit_api_secret.clone();
                let server_url = self.livekit_server_url.clone();
                let parent_addr = self.parent_addr.clone();
                let (tx, mut rx) = channel::<()>();
                self.cancel_sender = Some(tx);

                let fut = async move {
                    listen_to_room_data_channels(
                        &api_key,
                        &api_secret,
                        &server_url,
                        &rname,
                        &egress_id,
                        topic,
                        &mut rx,
                        parent_addr,
                    )
                    .await;
                };
                ctx.spawn(actix::fut::wrap_future(fut));
            }
            RoomListenerMessages::StopListening => {
                log::info!(
                    "Stopping listening to room data channels {:?}",
                    &self.cancel_sender
                );
                if let Some(sender) = self.cancel_sender.take() {
                    log::info!("Stopping listening to room data channels");
                    let _ = sender.send(());
                }
            }
        }
    }
}

pub(crate) async fn listen_to_room_data_channels(
    api_key: &str,
    api_secret: &str,
    server_url: &str,
    room_name: &str,
    egress_id: &str,
    topic: Option<String>,
    cancel_receiver: &mut OneshotReceiver<()>,
    parent_addr: Addr<TextEgressActor>,
) -> () {
    log::info!("Listening to room data channels for room: {:?}", room_name);

    parent_addr.do_send(RoomListenerUpdates::Started {
        egress_id: egress_id.to_string(),
        room_name: room_name.to_string(),
        files: vec![],
        topic: topic.clone(),
    });

    let jwt = get_egress_token(room_name, &api_key, &api_secret);
    let jwt = match jwt {
        Ok(jwt) => jwt,
        Err(e) => {
            log::error!("Failed to generate access token: {:?}", e);
            parent_addr.do_send(RoomListenerUpdates::Failed {
                egress_id: egress_id.to_string(),
                error: e,
            });
            return ();
        }
    };

    let room_join_result = join_room(&server_url, &jwt).await;

    let (room, mut room_events) = match room_join_result {
        Ok((room, room_events)) => (room, room_events),
        Err(e) => {
            log::error!("Failed to join room: {:?}", e);
            parent_addr.do_send(RoomListenerUpdates::Failed {
                egress_id: egress_id.to_string(),
                error: e,
            });
            return ();
        }
    };

    let mut per_participant_files: HashMap<String, FileHandler> = HashMap::new();
    let temp_dir = match TempDir::new(room_name) {
        Ok(temp_dir) => temp_dir.into_path(),
        Err(e) => {
            parent_addr.do_send(RoomListenerUpdates::Failed {
                egress_id: egress_id.to_string(),
                error: e.into(),
            });
            return ();
        }
    };

    let mut to_listen = topic;
    let mut metadata = TextEgressMetadata {
        room_name: room_name.to_string(),
        topic: to_listen.clone(),
        started_at: chrono::Utc::now().timestamp(),
        ended_at: None,
    };

    println!("Listening to room data channels for room: {:?}", room_name);

    loop {
        tokio::select! {
            _ = &mut *cancel_receiver => {
                log::info!("Cancelling listening to room data channels");
                // leave the room
                let _ = room.close().await;
                break;
            }
            Some(event) = room_events.recv() => {
                match event {
                    RoomEvent::DataReceived {
                        payload,
                        participant,
                        kind: _,
                        topic,
                    } => {
                        println!("Data received from participant: {:?}, payload: {:?}", participant, payload);
                        let timestamp = chrono::Utc::now();
                        let timestamp_str_iso = timestamp.format("%Y-%m-%dT%H:%M:%S%Z").to_string();
                        // let timestamp_iso = 
                        let timestamp_ns = timestamp.timestamp_nanos_opt().unwrap_or_default();
                        let payload_str = format!("{}|{}|{}\n", timestamp_str_iso, timestamp_ns, String::from_utf8_lossy(&payload).to_string());
                        if let Some(participant) = participant {
                            if let (Some(topic), Some(to_listen)) = (&topic, &to_listen) {
                                log::info!("Comparing topics: {:?} and {:?}", topic, to_listen);
                                if topic != to_listen {
                                    continue;
                                }
                            }

                            let topic_prefix = to_listen.take().unwrap_or_else(|| "all-topics".to_string());
                            let filepath = format!(
                                "{}-{}-{}",
                                participant.identity().as_str(),
                                topic_prefix,
                                chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%Z").to_string()
                            );

                            if !per_participant_files.contains_key(participant.identity().as_str()) {
                                match create_file(&filepath, &temp_dir).await {
                                    Ok((fh, fname)) => {
                                        per_participant_files.insert(
                                            participant.identity().into(),
                                            FileHandler {
                                                file: fh,
                                                path: fname,
                                            },
                                        );
                                        parent_addr.do_send(RoomListenerUpdates::Updated {
                                            egress_id: egress_id.to_string(),
                                            room_name: room_name.to_string(),
                                            files: per_participant_files
                                            .iter()
                                            .map(|(participant, file_handler)| DataEgressResultFiles {
                                                participant: participant.to_string(),
                                                file_path: file_handler.path.clone(),
                                            })
                                            .collect(),
                                            topic: to_listen.clone(),
                                        });
                                        ()
                                    },
                                    Err(e) => {
                                        log::error!("Failed to create file: {:?}", e);
                                        parent_addr.do_send(RoomListenerUpdates::Failed {
                                            egress_id: egress_id.to_string(),
                                            error: e.into()
                                        });
                                        return ();
                                    }
                                };
                            }

                            let handle = per_participant_files
                                .get_mut(&participant.identity().to_string())
                                .unwrap();
                            match handle.file.write_all(payload_str.as_ref()).await {
                                Ok(_) => {
                                    log::debug!(
                                        "Data received from participant: {:?}, payload: {:?}",
                                        participant.identity(),
                                        payload_str
                                    );
                                },
                                Err(e) => {
                                    log::error!("Failed to write to file: {:?}", e);
                                    parent_addr.do_send(RoomListenerUpdates::Failed {
                                        egress_id: egress_id.to_string(),
                                        error: e.into()
                                    });
                                    return ();
                                }
                            }
                        }
                    }
                    RoomEvent::Disconnected { reason } => {
                        log::info!("Disconnected from room {:?}", reason);
                        break;
                    }
                    _ => {}
                }
            },
        }
    }

    let mut results: Vec<DataEgressResultFiles> = per_participant_files
        .iter()
        .map(|(participant, file_handler)| DataEgressResultFiles {
            participant: participant.to_string(),
            file_path: file_handler.path.clone(),
        })
        .collect();

    metadata.ended_at = Some(chrono::Utc::now().timestamp());

    let metadata_file = temp_dir.join("metadata.json");
    match File::create(&metadata_file).await {
        Ok(mut fh) => {
            // Serialize metadata to file
            let _ = match serde_json::to_string(&metadata) {
                Ok(s) => fh
                    .write_all(s.as_bytes())
                    .await
                    .map_err(|e| {
                        log::error!("Failed to write metadata to file: {:?}", e);
                        parent_addr.do_send(RoomListenerUpdates::Failed {
                            egress_id: egress_id.to_string(),
                            error: e.into(),
                        });
                        return ();
                    })
                    .map(|_| {
                        log::info!("Metadata written to file: {:?}", metadata_file);
                        results.push(DataEgressResultFiles {
                            participant: "metadata".to_string(),
                            file_path: metadata_file.to_str().unwrap().to_string(),
                        });
                    }),
                Err(e) => {
                    log::error!("Failed to serialize metadata: {:?}", e);
                    parent_addr.do_send(RoomListenerUpdates::Failed {
                        egress_id: egress_id.to_string(),
                        error: e.into(),
                    });
                    return ();
                }
            };
        }
        Err(e) => {
            log::error!("Failed to create metadata file: {:?}", e);
            parent_addr.do_send(RoomListenerUpdates::Failed {
                egress_id: egress_id.to_string(),
                error: e.into(),
            });
            return ();
        }
    };

    log::info!("Stopped listening to room data channels");
    parent_addr.do_send(RoomListenerUpdates::Stopped {
        egress_id: egress_id.to_string(),
        files: results,
        room_name: room_name.to_string(),
        topic: to_listen,
    });
}
