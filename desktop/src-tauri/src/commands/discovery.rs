use crate::core::dto::GameDto;
use crate::core::AppError;
use crate::network::{self, DetectedSubnet, Ipv4Cidr, RelayInfo, MAX_SCAN_HOSTS};
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;
use tauri::{AppHandle, Emitter};

/// Lists every IPv4 interface on this machine (primary network as well as
/// secondary adapters like a phone hotspot uplink or a VPN), so the UI can
/// offer them as scan targets alongside any manually entered CIDR range.
#[tauri::command]
pub fn list_local_subnets() -> Result<Vec<DetectedSubnet>, AppError> {
    Ok(network::list_local_subnets()?)
}

/// Probes every host across the given CIDR ranges for a warlay-server
/// instance, streaming each discovered relay to the frontend as a
/// `discovery://relay-found` event and returning the full list once the
/// scan window closes.
#[tauri::command]
pub async fn discover_relays(
    app: AppHandle,
    cidr_ranges: Vec<String>,
    control_port: u16,
    timeout_ms: u64,
) -> Result<Vec<RelayInfo>, AppError> {
    let cidrs: Vec<Ipv4Cidr> = cidr_ranges
        .iter()
        .map(|raw| Ipv4Cidr::parse(raw))
        .collect::<Result<_, _>>()?;

    let total_hosts: u64 = cidrs.iter().map(Ipv4Cidr::host_count).sum();
    if total_hosts > MAX_SCAN_HOSTS {
        return Err(network::NetworkError::RangeTooLarge(total_hosts).into());
    }

    let mut receiver = network::spawn_discovery(cidrs, control_port, Duration::from_millis(timeout_ms));
    let mut found = Vec::new();
    while let Some(relay) = receiver.recv().await {
        let _ = app.emit("discovery://relay-found", &relay);
        found.push(relay);
    }
    let _ = app.emit("discovery://completed", found.len());
    Ok(found)
}

/// Queries a single, already-discovered relay for the games it has recently
/// seen broadcast on its local subnet.
#[tauri::command]
pub async fn list_games(relay_ip: String, control_port: u16) -> Result<Vec<GameDto>, AppError> {
    let ip: IpAddr = relay_ip
        .parse()
        .map_err(|_| AppError::InvalidInput(format!("'{relay_ip}' is not a valid IP address")))?;
    let target = SocketAddr::new(ip, control_port);
    let games = network::query_games(target, Duration::from_secs(2)).await?;
    Ok(games
        .into_iter()
        .map(|g| crate::core::dto::GameDto {
            source_addr: g.source_addr.to_string(),
            game_name: g.game_name,
            last_seen_secs_ago: g.last_seen_secs_ago,
            payload_len: g.payload_len,
        })
        .collect())
}
