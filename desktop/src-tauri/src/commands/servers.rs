use crate::core::dto::ServerDto;
use crate::core::AppError;
use crate::state::AppState;
use tauri::State;
use uuid::Uuid;

fn invalid(field: &str, value: &str) -> AppError {
    AppError::InvalidInput(format!("'{value}' is not a valid {field}"))
}

#[tauri::command]
pub fn list_servers(state: State<'_, AppState>) -> Result<Vec<ServerDto>, AppError> {
    Ok(state.store.list_servers()?.into_iter().map(ServerDto::from).collect())
}

/// Registers a server by address alone — its display name isn't something
/// the user types in, it's whatever the server calls itself once the app
/// actually talks to it (see `connect_to_server` / `HandshakeWelcome`).
#[tauri::command]
pub fn add_server(state: State<'_, AppState>, address: String) -> Result<ServerDto, AppError> {
    let address = address.trim();
    if address.is_empty() {
        return Err(AppError::InvalidInput(
            "la dirección del servidor no puede estar vacía".to_string(),
        ));
    }
    Ok(ServerDto::from(state.store.add_server(address)?))
}

#[tauri::command]
pub fn remove_server(state: State<'_, AppState>, server_id: String) -> Result<(), AppError> {
    let id = Uuid::parse_str(&server_id).map_err(|_| invalid("server id", &server_id))?;
    if *state.active_room_server.lock().expect("active room lock poisoned") == Some(id) {
        return Err(AppError::InvalidInput(
            "termina o abandona la partida activa antes de eliminar este servidor".to_string(),
        ));
    }
    state.store.remove_server(id)?;
    Ok(())
}

/// Exposes this installation's own persistent identity — the id it
/// presents to every server it connects to.
#[tauri::command]
pub fn get_client_id(state: State<'_, AppState>) -> String {
    state.store.client_id().to_string()
}
