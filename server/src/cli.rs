use clap::{Args as ClapArgs, Parser, Subcommand};
use std::net::SocketAddr;
use std::path::PathBuf;
use warlay::config::Settings;

#[derive(Debug, Parser)]
#[command(
    name = "warlay-server",
    about = "Warlay relay server: centralized matchmaking hub (VPS) or LAN-mesh broadcast bridge (Termux)"
)]
pub struct Cli {
    #[command(subcommand)]
    pub mode: Mode,
}

#[derive(Debug, Subcommand)]
pub enum Mode {
    /// Centralized matchmaking (WebSocket) + room-scoped UDP relay. This is
    /// the definitive architecture for a VPS deployment.
    Hub(HubArgs),
    /// Broadcast-reflection peer mesh between isolated LAN subnets
    /// (e.g. a Termux bridge on a phone hotspot).
    LanMesh(LanMeshArgs),
}

#[derive(Debug, ClapArgs)]
pub struct HubArgs {
    /// Path to a TOML configuration file. When omitted, built-in defaults are
    /// used and can still be overridden by the flags below.
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Address the WebSocket matchmaking control plane binds to.
    #[arg(long)]
    pub ws_addr: Option<SocketAddr>,

    /// Address the room-scoped UDP relay socket binds to.
    #[arg(long)]
    pub relay_addr: Option<SocketAddr>,

    /// Address the TCP game-session tunnel binds to.
    #[arg(long)]
    pub tcp_tunnel_addr: Option<SocketAddr>,

    /// Address the UDP rendezvous socket binds to.
    #[arg(long)]
    pub rendezvous_udp_addr: Option<SocketAddr>,

    /// Human-readable name announced to clients.
    #[arg(long)]
    pub name: Option<String>,

    /// Overrides the configured tracing log level (e.g. "debug", "warn").
    #[arg(long)]
    pub log_level: Option<String>,
}

#[derive(Debug, ClapArgs)]
pub struct LanMeshArgs {
    /// Path to a TOML configuration file. When omitted, built-in defaults are
    /// used and can still be overridden by the flags below.
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// Address the relay data socket binds to (game traffic).
    #[arg(long)]
    pub bind: Option<SocketAddr>,

    /// Address the control socket binds to (discovery / game listing).
    #[arg(long)]
    pub control: Option<SocketAddr>,

    /// Peer bridge address to relay with. May be repeated for a mesh.
    #[arg(long = "peer")]
    pub peers: Vec<SocketAddr>,

    /// Human-readable name announced to clients over the control plane.
    #[arg(long)]
    pub name: Option<String>,

    /// Overrides the configured tracing log level (e.g. "debug", "warn").
    #[arg(long)]
    pub log_level: Option<String>,
}

pub fn apply_hub_overrides(settings: &mut Settings, args: &HubArgs) {
    if let Some(ws_addr) = args.ws_addr {
        settings.hub.ws_addr = ws_addr;
    }
    if let Some(relay_addr) = args.relay_addr {
        settings.hub.relay_addr = relay_addr;
    }
    if let Some(tcp_tunnel_addr) = args.tcp_tunnel_addr {
        settings.hub.tcp_tunnel_addr = tcp_tunnel_addr;
    }
    if let Some(rendezvous_udp_addr) = args.rendezvous_udp_addr {
        settings.hub.rendezvous_udp_addr = rendezvous_udp_addr;
    }
    if let Some(name) = &args.name {
        settings.server_name = name.clone();
    }
    if let Some(log_level) = &args.log_level {
        settings.observability.log_level = log_level.clone();
    }
}

pub fn apply_lan_mesh_overrides(settings: &mut Settings, args: &LanMeshArgs) {
    if let Some(bind) = args.bind {
        settings.relay.bind_addr = bind;
    }
    if let Some(control) = args.control {
        settings.relay.control_addr = control;
    }
    if !args.peers.is_empty() {
        settings.relay.peers.extend(args.peers.iter().copied());
    }
    if let Some(name) = &args.name {
        settings.server_name = name.clone();
    }
    if let Some(log_level) = &args.log_level {
        settings.observability.log_level = log_level.clone();
    }
}
