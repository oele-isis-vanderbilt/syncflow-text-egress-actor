use livekit_api::access_token::AccessTokenError;
use rusoto_core::RusotoError;
use rusoto_s3::PutObjectError;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum TextEgressError {
    #[error("Failed to generate access token: {0}")]
    AccessTokenError(#[from] AccessTokenError),

    #[error("Failed to connect to room: {0}")]
    RoomError(#[from] livekit::RoomError),

    #[error("File handler error: {0}")]
    FileError(#[from] std::io::Error),

    #[error("Egress not found: {0}")]
    EgressNotFound(String),

    #[error("Failed to upload file: {0}")]
    S3UploadError(#[from] RusotoError<PutObjectError>),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Error loading environment file: {0}")]
    DotEnvError(#[from] dotenvy::Error),

    #[error("Error deserializing environment: {0}")]
    EnviousError(#[from] envious::EnvDeserializationError),

    #[error("Error in reqwest: {0}")]
    ReqwestError(#[from] reqwest::Error),

    #[error("Error in JWT: {0}")]
    JWTError(#[from] jsonwebtoken::errors::Error),

    #[error("AMQPError: {0}")]
    AMQPError(#[from] amqprs::error::Error),

    #[error("Project client error: {0}")]
    ProjectClientError(#[from] syncflow_client::ProjectClientError),

    #[error("Egress group not registered for project id: {0}")]
    DeviceNotRegistered(String),
}
