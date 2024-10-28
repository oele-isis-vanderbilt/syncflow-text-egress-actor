use crate::error_messages::TextEgressError;
use actix::prelude::*;
use actix::{Actor, Addr, Handler};
use amqprs::channel::QueueDeclareArguments;
use amqprs::connection::{Connection, OpenConnectionArguments};
use amqprs::tls::TlsAdaptor;
use syncflow_shared::deployment_config::S3Config;
use std::sync::Mutex;
use std::{collections::HashMap, sync::Arc};
use syncflow_client::ProjectClient;
use syncflow_shared::device_models::{DeviceRegisterRequest, DeviceResponse};

pub struct SessionListenerActor {
    pub rabbitmq_host: String,
    pub port: u16,
    pub use_ssl: bool,
    project_clients: Arc<Mutex<HashMap<String, ProjectClient>>>,
    pub registered_egress_actors: Arc<Mutex<HashMap<String, String>>>,
}

impl SessionListenerActor {
    pub fn new(rabbitmq_host: &str, port: u16, use_ssl: bool) -> Self {
        SessionListenerActor {
            rabbitmq_host: rabbitmq_host.to_string(),
            port,
            use_ssl,
            project_clients: Default::default(),
            registered_egress_actors: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Message)]
#[rtype(result = "Result<DeviceResponse, TextEgressError>")]
pub enum ProjectMessages {
    RegisterToProject {
        project_id: String,
        api_key: String,
        api_secret: String,
        base_url: String,
    },
    DeregisterFromProject {
        project_id: String,
    },
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
            ProjectMessages::RegisterToProject {
                project_id,
                api_key,
                api_secret,
                base_url,
            } => {
                let project_clients = self.project_clients.clone();
                let registered_actors = self.registered_egress_actors.clone();

                let fut = async move {
                    let client = ProjectClient::new(&base_url, &project_id, &api_key, &api_secret);
                    let registration_request = DeviceRegisterRequest {
                        name: "text-egress".to_string(),
                        group: "text-egress".to_string(),
                        comments: Some("Text Egress Actor".to_string()),
                    };

                    let egress_actor_response =
                        client.register_device(&registration_request).await?;

                    log::info!(
                        "Registered session listener actor to {:#?} : {:#?}",
                        &project_id,
                        egress_actor_response
                    );

                    let mut project_clients = project_clients.lock().unwrap();
                    let mut registered_actors = registered_actors.lock().unwrap();
                   
                    project_clients.insert(project_id.clone(), client);
                    registered_actors.insert(project_id, egress_actor_response.id.clone());
                   
                    Ok(egress_actor_response)
                };

                Box::pin(fut.into_actor(self))
            }
            ProjectMessages::DeregisterFromProject { project_id } => {
                let project_clients = self.project_clients.clone();
                let registered_devices = self.registered_egress_actors.clone();
                let fut = async move {
                    let mut project_clients = project_clients.lock().unwrap();
                    let mut registered_devices = registered_devices.lock().unwrap();
                    let project_client = project_clients.remove(&project_id).unwrap();

                    let device_id = registered_devices.remove(&project_id).unwrap();
                    let device_response = project_client.delete_device(&device_id).await?;
                    log::info!(
                        "Deregistered session listener actor from {:#?} : {:#?}",
                        &project_id,
                        device_response
                    );

                    Ok(device_response)
                };

                Box::pin(fut.into_actor(self))
            }
        }
    }
}


#[derive(Debug, Clone, Message)]
#[rtype(result = "()")]
pub enum RabbitMQListenerActorMessages {
    StartListening {
        project_id: String,
        device_id: String,
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
        device_id: String,
    }
}

pub struct RabbitMQListenerActor {
    pub project_id: String,
    pub device_id: String,
}

impl Actor for RabbitMQListenerActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, _ctx: &mut Self::Context) {
        log::info!("RabbitMQLivekitRoomJoinActor started for project: {:#?}", self.project_id);
    }
}


impl Handler<RabbitMQListenerActorMessages> for RabbitMQListenerActor {
    type Result = ();

    fn handle(&mut self, msg: RabbitMQListenerActorMessages, _ctx: &mut Self::Context) -> Self::Result {
        match msg {
            RabbitMQListenerActorMessages::StartListening {
                project_id,
                device_id,
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
                let fut = async move {
                    let args = if use_ssl {
                        OpenConnectionArguments::new(&rabbitmq_host, rabbitmq_port, &api_token, &group_name)
                            .virtual_host(&rabbitmq_vhost_name)
                            .tls_adaptor(TlsAdaptor::without_client_auth(None, rabbitmq_host.to_string()).unwrap())
                            .finish()
                    } else {
                        OpenConnectionArguments::new(&rabbitmq_host, rabbitmq_port, &api_token, &group_name)
                            .virtual_host(&rabbitmq_vhost_name)
                            .finish()
                    };
            
                    let connection = Connection::open(&args).await?;
            
                    let channel = connection.open_channel(None).await?;

                    let queue_declare_args = QueueDeclareArguments::default()
                        .exclusive(true)
                        .auto_delete(true)
                        .finish();

                    let (queue_name, _, _) = channel.queue_declare(queue_declare_args).await?;
                    
                    channel.queue_bind(queue_name.as_str(), exchange_name.as_str(), binding_key.as_str(), QueueBindArguments::default()).await?;
                }
            }
            RabbitMQListenerActorMessages::StopListening { project_id, device_id } => {
                log::info!("Stopping RabbitMQ listener for project: {:#?}", project_id);
            }
        }
    }
    
}
