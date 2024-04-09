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
}
