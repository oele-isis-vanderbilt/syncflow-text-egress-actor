use actix::Actor;
use rustls::crypto::aws_lc_rs::default_provider;
use std::error::Error;
use std::{env, vec};
use syncflow_text_egress_actor::config::TextEgressConfig;
use syncflow_text_egress_actor::session_listener_actor::{ProjectMessages, SessionListenerActor};
use tokio::signal;

#[actix_rt::main]
async fn main() -> Result<(), Box<dyn Error>> {
    default_provider().install_default().unwrap();
    let config = TextEgressConfig::load()?;
    env::set_var(
        "RUST_LOG",
        "actix_web=debug,actix_rt=debug,syncflow_text_egress_actor=debug",
    );
    env_logger::init();
    log::info!("Initializing TextEgressActor");
    let mut actors = vec![];
    for project in config.projects.iter() {
        let session_listener_actor = SessionListenerActor::new(
            &config.rabbitmq_host,
            config.rabbitmq_port,
            true,
            &project.project_id,
            &config.syncflow_server_url,
            &project.key,
            &project.secret,
            &project.s3_config,
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
        let _ = actor.send(ProjectMessages::Deregister).await??;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
    }

    Ok(())
}
