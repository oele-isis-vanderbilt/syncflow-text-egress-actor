use actix::Actor;
use livekit_text_egress_actor::config::TextEgressConfig;
use livekit_text_egress_actor::session_listener_actor::{ProjectMessages, SessionListenerActor};
use rustls::crypto::aws_lc_rs::default_provider;
use std::env;
use std::error::Error;

use syncflow_client::ProjectClient;
use syncflow_shared::device_models::DeviceRegisterRequest;

#[actix_rt::main]
async fn main() -> Result<(), Box<dyn Error>> {
    default_provider().install_default().unwrap();
    let config = TextEgressConfig::load()?;
    env::set_var(
        "RUST_LOG",
        "actix_web=debug,actix_rt=debug,livekit_text_egress_actor=debug",
    );
    env_logger::init();

    let session_listerner_actor =
        SessionListenerActor::new(&config.rabbitmq_host, config.rabbitmq_port, false);
    let addr = session_listerner_actor.start();

    for key in config.syncflow_api_keys.iter() {
        let result = addr
            .send(ProjectMessages::RegisterToProject {
                project_id: key.project_id.clone(),
                api_key: key.key.clone(),
                api_secret: key.secret.clone(),
                base_url: config.syncflow_server_url.clone(),
            })
            .await??;

        log::info!("Registered to project: {:#?}", result);

        let device_deregistered = addr
            .send(ProjectMessages::DeregisterFromProject {
                project_id: key.project_id.clone(),
            })
            .await??;

        log::info!("Deregistered from project: {:#?}", device_deregistered);
    }

    Ok(())
}
