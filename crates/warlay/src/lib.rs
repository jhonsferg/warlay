//! Facade over Warlay's shared crates. Enable the features matching what
//! you need (`protocol`, `core`, `crypto`, `engine`, `matchmaking`,
//! `storage`, `transport`, `config`) and access them as `warlay::x::...`.

#[cfg(feature = "protocol")]
pub use protocol;

#[cfg(feature = "core")]
pub use warlay_core as core;

#[cfg(feature = "crypto")]
pub use crypto;

#[cfg(feature = "engine")]
pub use engine;

#[cfg(feature = "matchmaking")]
pub use matchmaking;

#[cfg(feature = "storage")]
pub use storage;

#[cfg(feature = "transport")]
pub use transport;

#[cfg(feature = "config")]
pub use config;
