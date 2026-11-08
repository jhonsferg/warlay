//! Voice transport for the in-room voice chat: the webview hands each encoded
//! Opus frame here as a raw binary body and it is sent to every connected
//! member over the mesh (unreliable datagrams, like real-time audio wants).

use tauri::ipc::{InvokeBody, Request};
use tauri::State;

use crate::core::error::AppError;
use crate::state::AppState;

/// Largest frame accepted: a QUIC datagram must stay under the path MTU, and
/// a 20 ms Opus frame is ~100 bytes, so anything near this is a bug.
const MAX_FRAME_BYTES: usize = 1000;

/// Validates the invoke body: it must be raw bytes, non-empty and small.
fn frame_from_body(body: &InvokeBody) -> Result<Vec<u8>, AppError> {
    let InvokeBody::Raw(bytes) = body else {
        return Err(AppError::InvalidInput(
            "el frame de voz debe enviarse como bytes crudos".to_string(),
        ));
    };
    if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
        return Err(AppError::InvalidInput(format!(
            "frame de voz de {} bytes fuera de rango (1..={MAX_FRAME_BYTES})",
            bytes.len()
        )));
    }
    Ok(bytes.clone())
}

/// Sends one encoded voice frame to the room. Returns how many members it
/// reached (0 when alone or not linked yet -- not an error).
#[tauri::command]
pub async fn send_voice_frame(request: Request<'_>, state: State<'_, AppState>) -> Result<usize, AppError> {
    let frame = frame_from_body(request.body())?;
    let agent_state = state.agent_state.lock().unwrap().clone();
    let Some(agent_state) = agent_state else {
        return Ok(0);
    };
    let mesh = agent_state.lock().await.peer_mesh.clone();
    Ok(mesh.map(|mesh| mesh.broadcast_voice_frame(&frame)).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_small_non_empty_raw_bodies_are_accepted() {
        assert_eq!(frame_from_body(&InvokeBody::Raw(vec![1, 2, 3])).unwrap(), vec![1, 2, 3]);
        assert!(frame_from_body(&InvokeBody::Raw(vec![0; MAX_FRAME_BYTES])).is_ok());

        for bad in [vec![], vec![0; MAX_FRAME_BYTES + 1]] {
            assert!(matches!(
                frame_from_body(&InvokeBody::Raw(bad)),
                Err(AppError::InvalidInput(_))
            ));
        }
        assert!(matches!(
            frame_from_body(&InvokeBody::Json(serde_json::json!([1, 2, 3]))),
            Err(AppError::InvalidInput(_))
        ));
    }
}
