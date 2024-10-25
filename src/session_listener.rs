use amqprs::consumer::DefaultConsumer;
use amqprs::BasicProperties;

use amqprs::channel::{
    BasicConsumeArguments, BasicPublishArguments, Channel, ExchangeDeclareArguments,
    QueueBindArguments, QueueDeclareArguments,
};
use amqprs::connection::{Connection, OpenConnectionArguments};

use amqprs::tls::TlsAdaptor;
use syncflow_client::ProjectClient;
use syncflow_shared::device_models::NewSessionMessage;
use syncflow_shared::project_models::NewSessionRequest;
use tokio::sync::Notify;

use crate::config::S3Config;
use crate::error_messages::TextEgressError;

#[derive(Clone)]
pub struct SessionListener {
    connection: Connection,
}

impl SessionListener {
    pub async fn create(
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        vhost_name: &str,
        use_ssl: bool,
    ) -> Result<Self, TextEgressError> {
        let args = if use_ssl {
            OpenConnectionArguments::new(host, port, username, password)
                .virtual_host(vhost_name)
                .tls_adaptor(TlsAdaptor::without_client_auth(None, host.to_string()).unwrap())
                .finish()
        } else {
            OpenConnectionArguments::new(host, port, username, password)
                .virtual_host(vhost_name)
                .finish()
        };

        let connection = Connection::open(&args).await?;

        Ok(SessionListener { connection })
    }

    pub async fn listen(
        &self,
        exchange_name: &str,
        binding_key: &str,
        project_client: &ProjectClient,
        s3_config: &S3Config,
    ) -> Result<(), TextEgressError> {
        let channel = self.connection.open_channel(None).await?;

        let queue_declare_args = QueueDeclareArguments::default()
            .exclusive(true)
            .auto_delete(true)
            .finish();

        let (queue_name, _, _) = channel
            .queue_declare(queue_declare_args)
            .await?
            .ok_or_else(|| {
                TextEgressError::AMQPError(amqprs::error::Error::ChannelUseError(
                    "Failed to declare queue".to_string(),
                ))
            })?;

        let queue_bind_args = QueueBindArguments::new(&queue_name, exchange_name, binding_key);
        channel.queue_bind(queue_bind_args).await?;

        let consume_args = BasicConsumeArguments::new(&queue_name, "text-egress-actor-consumer")
            .manual_ack(false)
            .finish();

        let (ctag, mut rx_messages) = channel.basic_consume_rx(consume_args).await?;

        while let Some(msg) = rx_messages.recv().await {
            let msg = msg.content.unwrap();
            let new_session = serde_json::from_slice::<NewSessionMessage>(&msg).unwrap();

            let new_session_request = NewSessionRequest {
                ..Default::default()
            };
            let session_token = project_client.create_session(&new_session_request).await?;
        }

        let guard = Notify::new();

        guard.notified().await;

        Ok(())
    }
}
