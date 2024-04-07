use crate::error::TextEgressError;
use livekit::{Room, RoomEvent};
use livekit_api::access_token::{AccessToken, VideoGrants};
use tokio::sync::mpsc;

fn get_egress_token(
    room_name: &str,
    api_key: &str,
    api_secret: &str,
) -> Result<String, TextEgressError> {
    let video_grants = VideoGrants {
        room: room_name.to_string(),
        hidden: true,
        can_subscribe: true,
        can_publish: false,
        room_join: true,
        room_create: false,
        ..Default::default()
    };

    let access_token = AccessToken::with_api_key(api_key, api_secret)
        .with_name("TEXT-EGRESS-BOT")
        .with_identity("TEXT-EGRESS-BOT")
        .with_grants(video_grants);

    let jwt = access_token.to_jwt()?;
    Ok(jwt)
}

pub(crate) async fn join_room(
    server_url: &str,
    access_token_str: &str,
) -> Result<(Room, mpsc::UnboundedReceiver<RoomEvent>), TextEgressError> {
    let (room, mut room_events) =
        Room::connect(server_url, access_token_str, Default::default()).await?;

    Ok((room, room_events))
}

pub(crate) async fn listen_to_room_data_channels(
    room_name: &str,
    topic: Option<String>,
    api_key: &str,
    api_secret: &str,
    server_url: &str,
) -> Result<(), TextEgressError> {
    log::info!("Listening to room data channels for room: {:?}", room_name);
    let jwt = get_egress_token(room_name, api_key, api_secret).map_err(|e| {
        log::error!("Failed to generate access token: {:?}", e);
        e
    })?;
    log::info!("JWT: {:?}", jwt);
    let (room, mut room_events) = join_room(server_url, &jwt).await?;
    log::info!("Joined room: {:?}", room_name);

    while let Some(event) = room_events.recv().await {
        match event {
            RoomEvent::DataReceived {
                payload,
                participant,
                kind,
                topic,
            } => {
                //lossy utf-8 conversion
                let payload_str = String::from_utf8_lossy(&payload);
                log::info!(
                    "Data received from: {:?}-{:?}: {:?}",
                    participant,
                    topic,
                    payload_str
                );
            }
            _ => {}
        }
    }

    // Connect to the server and listen to the room events
    Ok(())
}
