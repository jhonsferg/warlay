use std::sync::Arc;

use tokio::sync::Mutex;

use warlay::protocol::ipc::{RuntimeRequest, RuntimeResponse};

use super::state::RuntimeState;

/// Runs one request against the shared runtime state and answers it.
pub async fn handle_request(req: RuntimeRequest, state: Arc<Mutex<RuntimeState>>) -> RuntimeResponse {
    match req {
        RuntimeRequest::StartRoomSession {
            room_id,
            user_id,
            is_host,
            rendezvous_addr,
            relay_addr,
            ticket,
        } => {
            super::session::start_room_session(state, room_id, user_id, is_host, rendezvous_addr, relay_addr, ticket)
                .await
        }

        RuntimeRequest::StopRoomSession => super::session::stop_room_session(state).await,

        RuntimeRequest::StartGameTunnel { game_port } => super::session::start_game_tunnel(state, game_port).await,

        RuntimeRequest::UpdateRoomHelper {
            rendezvous_addr,
            relay_addr,
        } => super::session::update_room_helper(state, rendezvous_addr, relay_addr).await,
    }
}
