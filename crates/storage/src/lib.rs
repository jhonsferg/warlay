//! Warlay's small local stores, in pure Rust: a text key-value file for the
//! desktop app and the legacy hub's client registry. Earlier versions kept
//! these in SQLite; their files are imported once (see `legacy`) so nothing is
//! lost on update.

mod error;
mod file;
pub mod legacy;
mod server_store;

pub use error::StorageError;
pub use file::KvFile;
pub use server_store::{ClientRecord, ServerStore};
