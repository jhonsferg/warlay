//! The network runtime of the Warlay desktop client.
//!
//! This is the half of Warlay that needs elevated privileges or long-lived
//! sockets: hub connections, LAN capture and replay, the P2P game tunnel and
//! the voice relay. It runs inside `warlay.exe` (there is no other process),
//! is driven by the Tauri commands, which call [`handlers::handle_request`]
//! directly, and does its networking through [`warlay::engine`].
//! Everything it tells the UI goes out as Tauri events (see [`events`] and
//! [`status`]).

pub mod events;
pub mod handlers;
pub mod lan;
pub mod session;
pub mod state;
pub mod status;
pub mod voice;
