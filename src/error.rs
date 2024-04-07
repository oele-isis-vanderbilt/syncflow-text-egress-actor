use livekit_api::access_token::AccessTokenError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TextEgressError {
    #[error("Failed to generate access token: {0}")]
    AccessTokenError(#[from] AccessTokenError),

    #[error("Failed to connect to room: {0}")]
    RoomError(#[from] livekit::RoomError),
}
