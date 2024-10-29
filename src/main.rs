use actix::Actor;
use livekit_text_egress_actor::config::TextEgressConfig;
use livekit_text_egress_actor::session_listener_actor::{ProjectMessages, SessionListenerActor};
use rustls::crypto::aws_lc_rs::default_provider;
use std::error::Error;
use std::{env, vec};
use tokio::signal;

#[actix_rt::main]
async fn main() -> Result<(), Box<dyn Error>> {
    default_provider().install_default().unwrap();
    let config = TextEgressConfig::load()?;
    env::set_var(
        "RUST_LOG",
        "actix_web=debug,actix_rt=debug,livekit_text_egress_actor=debug",
    );
    env_logger::init();
    let mut actors = vec![];
    for key in config.syncflow_api_keys.iter() {
        let session_listener_actor = SessionListenerActor::new(
            &config.rabbitmq_host,
            config.rabbitmq_port,
            true,
            &key.project_id,
            &config.syncflow_server_url,
            &key.key,
            &key.secret,
        )
        .start();

        let register = session_listener_actor
            .send(ProjectMessages::Register)
            .await??;

        actors.push(session_listener_actor);

        log::info!("Registered to project: {:#?}", register);
    }

    let mut terminate_signal = signal::unix::signal(signal::unix::SignalKind::terminate())?;

    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            log::info!("Ctrl+C received, initiating shutdown...");
        },
        _ = terminate_signal.recv() => {
            log::info!("SIGTERM received, initiating shutdown...");
        },
    }

    for actor in actors {
        let result = actor.send(ProjectMessages::Deregister).await??;
        log::info!("Deregistered from project: {:#?}", result);
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    Ok(())
}
