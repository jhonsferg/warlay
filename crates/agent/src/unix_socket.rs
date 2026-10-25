//! The daemon's IPC endpoint on Linux/Unix: a Unix domain socket speaking
//! exactly the same newline-delimited JSON protocol as the Windows named
//! pipe, so a client drives the daemon the same way on both platforms.
//!
//! Access control, the Unix counterpart of the pipe's ACL plus
//! `security::verify_client_process`: the socket file is owner-only
//! (0600), and every connection's peer credentials (`SO_PEERCRED`) must be
//! this daemon's own user or root -- anything else is dropped before a
//! single request is read.

use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::PathBuf;
use std::sync::Arc;

use tokio::net::{UnixListener, UnixStream};
use tokio::signal::unix::{signal, SignalKind};
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::state::{teardown_tunnel_and_lan, DaemonState};

/// `$WARLAY_DAEMON_SOCKET`, else `$XDG_RUNTIME_DIR/warlay-daemon.sock`,
/// else `/tmp/warlay-daemon-<uid>.sock`.
pub fn socket_path() -> PathBuf {
    if let Some(path) = std::env::var_os("WARLAY_DAEMON_SOCKET").filter(|v| !v.is_empty()) {
        return PathBuf::from(path);
    }
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        return PathBuf::from(dir).join("warlay-daemon.sock");
    }
    PathBuf::from(format!("/tmp/warlay-daemon-{}.sock", own_uid()))
}

/// The user this process runs as (owner of its `/proc/self`).
fn own_uid() -> u32 {
    std::fs::metadata("/proc/self").map(|m| m.uid()).unwrap_or(0)
}

/// Binds the socket, replacing a stale file left by a daemon that died, but
/// refusing to start if another daemon is actually listening there.
async fn bind(path: &PathBuf) -> io::Result<UnixListener> {
    if path.exists() {
        if UnixStream::connect(path).await.is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("another daemon is already listening on {}", path.display()),
            ));
        }
        std::fs::remove_file(path)?;
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}

/// Whether a connection comes from this daemon's own user (or root).
fn peer_allowed(stream: &UnixStream, own_uid: u32) -> bool {
    match stream.peer_cred() {
        Ok(cred) if cred.uid() == own_uid || cred.uid() == 0 => true,
        Ok(cred) => {
            warn!(peer_uid = cred.uid(), peer_pid = ?cred.pid(), own_uid, "dropping a connection from another user");
            false
        }
        Err(error) => {
            warn!(%error, "could not read the peer's credentials, dropping the connection");
            false
        }
    }
}

/// Accepts clients until SIGINT/SIGTERM (what `docker stop` and systemd
/// send), then tears the active session down and removes the socket.
pub async fn serve(state: Arc<Mutex<DaemonState>>) -> io::Result<()> {
    let path = socket_path();
    let listener = bind(&path).await?;
    let own_uid = own_uid();
    info!(socket = %path.display(), own_uid, "listening on Unix socket");

    let mut terminate = signal(SignalKind::terminate())?;
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                let (stream, _) = match accepted {
                    Ok(accepted) => accepted,
                    Err(error) => {
                        warn!(%error, "failed to accept a connection");
                        continue;
                    }
                };
                if !peer_allowed(&stream, own_uid) {
                    continue;
                }
                info!("client connected to daemon");
                tokio::spawn(crate::ipc_server::handle_connection(stream, state.clone()));
            }
            _ = tokio::signal::ctrl_c() => break,
            _ = terminate.recv() => break,
            // The desktop app asked us to exit.
            () = crate::shutdown::requested() => break,
        }
    }

    info!("shutdown signal received, tearing down active session");
    {
        let mut s = state.lock().await;
        crate::session::clear_room_session(&mut s);
        teardown_tunnel_and_lan(&mut s);
    }
    let _ = std::fs::remove_file(&path);
    Ok(())
}
