use self::room_listener_actor::DataEgressResultFiles;
use crate::error_messages::TextEgressError;
use crate::room_listener_actor::{self, RoomListenerActor, RoomListenerMessages};
use actix::prelude::*;
use actix::{Actor, Addr, Handler};
use amqprs::channel::{BasicConsumeArguments, QueueBindArguments, QueueDeclareArguments};
use amqprs::connection::{Connection, OpenConnectionArguments};
use amqprs::tls::TlsAdaptor;
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use syncflow_client::ProjectClient;
use syncflow_shared::device_models::{DeviceRegisterRequest, DeviceResponse, NewSessionMessage};
use syncflow_shared::livekit_models::{TokenRequest, VideoGrantsWrapper};
use uuid::Uuid;

pub struct SessionListenerActor {
    pub rabbitmq_host: String,
    pub port: u16,
    pub use_ssl: bool,
    project_client: Arc<Mutex<ProjectClient>>,
    registered_egress_group: Arc<Mutex<Option<DeviceResponse>>>,
    rabbitmq_listener: Arc<Mutex<Option<Addr<RabbitMQListenerActor>>>>,
}

impl SessionListenerActor {
    pub fn new(
        rabbitmq_host: &str,
        port: u16,
        use_ssl: bool,
        project_id: &str,
        base_url: &str,
        api_key: &str,
        api_secret: &str,
    ) -> Self {
        SessionListenerActor {
            rabbitmq_host: rabbitmq_host.to_string(),
            port,
            use_ssl,
            project_client: Arc::new(Mutex::new(ProjectClient::new(
                base_url, project_id, api_key, api_secret,
            ))),
            registered_egress_group: Arc::new(Mutex::new(None)),
            rabbitmq_listener: Arc::new(Mutex::new(None)),
        }
    }
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
#[rtype(result = "Result<(), TextEgressError>")]
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

#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<DeviceResponse, TextEgressError>")]
pub enum ProjectMessages {
    Register,
    Deregister,
}

impl Actor for SessionListenerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("SessionListenerActor started");
    }
}

impl Handler<ProjectMessages> for SessionListenerActor {
    type Result = ResponseActFuture<Self, Result<DeviceResponse, TextEgressError>>;

    fn handle(&mut self, msg: ProjectMessages, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            ProjectMessages::Register => {
                let client = self.project_client.clone();
                let addr = _ctx.address();
                let rabbitmq_host = self.rabbitmq_host.clone();
                let port = self.port;
                let use_ssl = self.use_ssl;
                let device_details_arc = self.registered_egress_group.clone();
                let actor_addr_arc = self.rabbitmq_listener.clone();

                let fut = async move {
                    let client = client.lock().unwrap();
                    let project = client.get_project_details().await?;
                    let registration_request = DeviceRegisterRequest {
                        name: "text-egress".to_string(),
                        group: "text-egress".to_string(),
                        comments: Some("Text Egress Actor".to_string()),
                    };

                    let api_token = client.get_api_token().await.unwrap();

                    let egress_actor_response =
                        client.register_device(&registration_request).await?;

                    log::info!(
                        "Registered session listener actor to {:#?} : {:#?}",
                        &project.name,
                        egress_actor_response
                    );

                    let rmq_listener_actor = RabbitMQListenerActor::new(
                        project.id.clone(),
                        egress_actor_response.id.clone(),
                        addr,
                    );

                    let rmq_listener_addr = rmq_listener_actor.start();

                    rmq_listener_addr.do_send(RabbitMQListenerActorMessages::StartListening {
                        project_id: project.id.clone(),
                        group_name: egress_actor_response.group.clone(),
                        api_token: api_token,
                        rabbitmq_host: rabbitmq_host,
                        rabbitmq_port: port,
                        rabbitmq_vhost_name: "syncflow".to_string(),
                        use_ssl: use_ssl,
                        exchange_name: egress_actor_response
                            .session_notification_exchange_name
                            .clone()
                            .unwrap(),
                        binding_key: egress_actor_response
                            .session_notification_binding_key
                            .clone()
                            .unwrap(),
                    });
                    *actor_addr_arc.lock().unwrap() = Some(rmq_listener_addr);

                    let mut device_details = device_details_arc.lock().unwrap();
                    let response = DeviceResponse {
                        id: egress_actor_response.id.clone(),
                        name: egress_actor_response.name.clone(),
                        group: egress_actor_response.group.clone(),
                        comments: egress_actor_response.comments.clone(),
                        registered_at: egress_actor_response.registered_at,
                        registered_by: egress_actor_response.registered_by,
                        project_id: egress_actor_response.project_id.clone(),
                        session_notification_exchange_name: egress_actor_response
                            .session_notification_exchange_name
                            .clone(),
                        session_notification_binding_key: egress_actor_response
                            .session_notification_binding_key
                            .clone(),
                    };

                    *device_details = Some(response);

                    Ok(egress_actor_response)
                };

                Box::pin(fut.into_actor(self))
            }
            ProjectMessages::Deregister => {
                let project_client = self.project_client.clone();
                let device_details_arc = self.registered_egress_group.clone();
                let actor_addr_arc = self.rabbitmq_listener.clone();

                let fut = async move {
                    let device_details = device_details_arc.lock().unwrap();
                    let client = project_client.lock().unwrap();
                    let project = client.get_project_details().await?;
                    let device = device_details.as_ref().ok_or_else(|| {
                        TextEgressError::DeviceNotRegistered("Device not registered".to_string())
                    })?;
                    let deregistered_device = client.delete_device(&device.id).await?;
                    log::info!(
                        "Deregistered session listener actor from {:#?} : {:#?}",
                        &project.name,
                        deregistered_device
                    );

                    let addr = actor_addr_arc.lock().unwrap();
                    if addr.is_some() {
                        addr.as_ref().unwrap().do_send(
                            RabbitMQListenerActorMessages::StopListening {
                                project_id: project.id.clone(),
                            },
                        );
                    }

                    Ok(deregistered_device)
                };

                Box::pin(fut.into_actor(self))
            }
        }
    }
}

impl Handler<RoomListenerUpdates> for SessionListenerActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, msg: RoomListenerUpdates, ctx: &mut Self::Context) -> Self::Result {
        let fut = async move {
            match msg {
                RoomListenerUpdates::Started {
                    egress_id,
                    room_name,
                    topic,
                    files,
                } => {}
                RoomListenerUpdates::Updated {
                    egress_id,
                    room_name,
                    topic,
                    files,
                } => {}
                RoomListenerUpdates::Failed { egress_id, error } => {}
                RoomListenerUpdates::Stopped {
                    egress_id,
                    room_name,
                    topic,
                    files,
                } => {}
            }
        };

        Box::pin(fut.into_actor(self))
    }
}

impl Handler<S3UploaderUpdates> for SessionListenerActor {
    type Result = ResponseActFuture<Self, Result<(), TextEgressError>>;

    fn handle(&mut self, msg: S3UploaderUpdates, ctx: &mut Self::Context) -> Self::Result {
        let fut = async move {
            match msg {
                S3UploaderUpdates::Started {
                    egress_id,
                    files,
                    bucket,
                } => {}
                S3UploaderUpdates::Completed {
                    egress_id,
                    files,
                    bucket,
                } => {}
                S3UploaderUpdates::Failed { egress_id, error } => {}
            }
            Ok(())
        };
        Box::pin(fut.into_actor(self))
    }
}

impl Handler<SessionCreatedMessage> for SessionListenerActor {
    type Result = ResponseActFuture<Self, Result<(), TextEgressError>>;

    fn handle(&mut self, msg: SessionCreatedMessage, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("Received new session message: {:#?}", msg);
        let client = self.project_client.clone();
        let parent_addr = _ctx.address();

        let fut = async move {
            let project_client = client.lock().unwrap();

            let session_token = project_client
                .generate_session_token(
                    &msg.session_id,
                    &TokenRequest {
                        identity: "text-egress-actor".to_string(),
                        name: Some("Text Egress Actor".to_string()),
                        video_grants: VideoGrantsWrapper {
                            room: msg.session_name.clone(),
                            room_join: true,
                            room_create: false,
                            can_subscribe: true,
                            ..Default::default()
                        },
                    },
                )
                .await?;
            let room_listener_actor =
                RoomListenerActor::new(&Uuid::new_v4().to_string(), parent_addr);
            let room_listener_addr = room_listener_actor.start();
            room_listener_addr.do_send(RoomListenerMessages::StartListening {
                join_token: session_token.token.clone(),
                server_url: session_token.livekit_server_url.clone().unwrap(),
                room_name: msg.session_name.clone(),
                topic: None,
            });
            Ok(())
        };

        Box::pin(fut.into_actor(self))
    }
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<(), TextEgressError>")]
pub struct SessionCreatedMessage {
    pub session_id: String,
    pub session_name: String,
    pub project_id: String,
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "(Result<(), TextEgressError>)")]
pub enum RabbitMQListenerActorMessages {
    StartListening {
        project_id: String,
        group_name: String,
        api_token: String,
        rabbitmq_host: String,
        rabbitmq_port: u16,
        rabbitmq_vhost_name: String,
        use_ssl: bool,
        exchange_name: String,
        binding_key: String,
    },

    StopListening {
        project_id: String,
    },
}

pub struct RabbitMQListenerActor {
    pub project_id: String,
    pub device_id: String,
    parent_addr: Addr<SessionListenerActor>,
    connection: Arc<Mutex<Option<Connection>>>,
}

impl RabbitMQListenerActor {
    pub fn new(
        project_id: String,
        device_id: String,
        parent_addr: Addr<SessionListenerActor>,
    ) -> Self {
        RabbitMQListenerActor {
            project_id,
            device_id,
            parent_addr,
            connection: Arc::new(Mutex::new(None)),
        }
    }
}

impl Actor for RabbitMQListenerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!(
            "RabbitMQLivekitRoomJoinActor started for project: {:#?}",
            self.project_id
        );
    }
}

impl Handler<RabbitMQListenerActorMessages> for RabbitMQListenerActor {
    type Result = ResponseActFuture<Self, Result<(), TextEgressError>>;

    fn handle(
        &mut self,
        msg: RabbitMQListenerActorMessages,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        match msg {
            RabbitMQListenerActorMessages::StartListening {
                project_id,
                group_name,
                api_token,
                rabbitmq_host,
                rabbitmq_port,
                rabbitmq_vhost_name,
                use_ssl,
                exchange_name,
                binding_key,
            } => {
                log::info!("Starting RabbitMQ listener for project: {:#?}", project_id);
                let conn = self.connection.clone();
                let parent_addr = self.parent_addr.clone();
                let fut = async move {
                    let args = if use_ssl {
                        OpenConnectionArguments::new(
                            &rabbitmq_host,
                            rabbitmq_port,
                            &api_token,
                            &group_name,
                        )
                        .virtual_host(&rabbitmq_vhost_name)
                        .tls_adaptor(
                            TlsAdaptor::without_client_auth(None, rabbitmq_host.to_string())
                                .unwrap(),
                        )
                        .finish()
                    } else {
                        OpenConnectionArguments::new(
                            &rabbitmq_host,
                            rabbitmq_port,
                            &api_token,
                            &group_name,
                        )
                        .virtual_host(&rabbitmq_vhost_name)
                        .finish()
                    };

                    let connection = Connection::open(&args).await?;

                    let channel = Arc::new(Mutex::new(connection.open_channel(None).await?));
                    *conn.lock().unwrap() = Some(connection);

                    let queue_declare_args = QueueDeclareArguments::default()
                        .exclusive(true)
                        .auto_delete(true)
                        .finish();

                    let (queue_name, _, _) = channel
                        .lock()
                        .unwrap()
                        .queue_declare(queue_declare_args)
                        .await?
                        .ok_or_else(|| {
                            TextEgressError::AMQPError(amqprs::error::Error::ChannelUseError(
                                "Failed to declare queue".to_string(),
                            ))
                        })?;

                    let queue_bind_args =
                        QueueBindArguments::new(&queue_name, &exchange_name, &binding_key);
                    channel.lock().unwrap().queue_bind(queue_bind_args).await?;

                    let cloned_channel = Arc::clone(&channel);
                    let cloned_queue = queue_name.clone();

                    let consume_args = BasicConsumeArguments::new(&cloned_queue, "text-egress");
                    let result = cloned_channel
                        .lock()
                        .unwrap()
                        .basic_consume_rx(consume_args)
                        .await;
                    let (_, mut rx) = result?;

                    while let Some(msg) = rx.recv().await {
                        let session_message =
                            serde_json::from_slice::<NewSessionMessage>(&msg.content.unwrap())?;

                        parent_addr.do_send(SessionCreatedMessage {
                            session_id: session_message.session_id,
                            session_name: session_message.session_name,
                            project_id: project_id.clone(),
                        });
                    }

                    Ok(())
                };

                Box::pin(fut.into_actor(self))
            }
            RabbitMQListenerActorMessages::StopListening { project_id } => {
                log::debug!("Stopping RabbitMQ listener for project: {:#?}", project_id);
                let conn = self.connection.clone();
                let fut = async move {
                    let connection = conn.lock().unwrap();
                    if connection.is_none() {
                    } else {
                        let conn = connection.as_ref().unwrap();
                        conn.clone().close().await?;
                        log::info!("RabbitMQ connection closed--");
                    }
                    Ok(())
                };

                Box::pin(fut.into_actor(self))
            }
        }
    }
}
