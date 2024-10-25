use livekit_text_egress_actor::config::TextEgressConfig;
use livekit_text_egress_actor::session_listener::SessionListener;
use rustls::crypto::aws_lc_rs::default_provider;
use std::env;
use std::error::Error;
use syncflow_client::ProjectClient;
use syncflow_shared::device_models::DeviceRegisterRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    default_provider().install_default().unwrap();
    let config = TextEgressConfig::load()?;
    env::set_var(
        "RUST_LOG",
        "actix_web=debug,actix_rt=debug,livekit_text_egress_actor=debug",
    );
    env_logger::init();

    for key in config.syncflow_api_keys.iter() {
        let client = ProjectClient::new(
            &config.syncflow_server_url,
            &key.project_id,
            &key.key,
            &key.secret,
        );

        println!(
            "Registering text egress actor device for project {:#?}",
            key.project_id
        );
        let registration_request = DeviceRegisterRequest {
            name: config.device_group_name.clone(),
            group: "text-egress".to_string(),
            comments: Some("Text Egress Actor".to_string()),
        };

        println!(
            "Registering text egress actor device: {:#?}",
            registration_request
        );

        let egress_actor_response = client.register_device(&registration_request).await?;

        log::info!(
            "Registered text egress actor device: {:#?} for project {:#?}",
            egress_actor_response,
            key.project_id
        );

        let token = client.get_api_token().await?;

        let session_listener = SessionListener::create(
            &config.rabbitmq_host,
            config.rabbitmq_port,
            &token,
            &egress_actor_response.group,
            "syncflow",
            true,
        )
        .await?;
        let client_cloned = client.clone();
        let cloned_s3_config = config.s3_config.clone();

        let handle = tokio::spawn(async move {
            session_listener
                .listen(
                    &egress_actor_response
                        .session_notification_exchange_name
                        .unwrap(),
                    &egress_actor_response
                        .session_notification_binding_key
                        .unwrap(),
                    &client_cloned,
                    &cloned_s3_config,
                )
                .await
                .unwrap();
        });

        handle.await?;

        let egress_actor_response = client.delete_device(&egress_actor_response.id).await?;

        log::info!(
            "Deleted text egress actor device: {:#?}",
            egress_actor_response
        );
    }

    Ok(())
}
