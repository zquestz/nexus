//! UPnP/IGD port-forwarding adapter
//!
//! Generic port-forwarding setup, lease renewal, rediscovery, and
//! cleanup over a caller-supplied list of port mappings. Daemons pass
//! the ports they want forwarded; this module handles gateway
//! discovery, mapping, periodic renewal, and shutdown removal.
//!
//! ## Lifecycle
//!
//! 1. `UpnpGateway::setup(bind_addr, ports, description)` discovers the
//!    LAN's UPnP/IGD gateway, retrieves the external IP, and adds each
//!    requested port mapping with a 1-hour lease.
//! 2. `spawn_lease_renewal_task` keeps the mappings alive by re-adding
//!    them every 30 minutes (50% of the lease). On renewal failure it
//!    re-discovers the gateway (handles router reboots) and re-adds.
//! 3. On graceful shutdown, callers run `remove_port_mapping` to drop
//!    the mappings cleanly. Failures are non-fatal — leases expire on
//!    their own within an hour.
//!
//! ## IPv4 only
//!
//! UPnP/IGD is a NAT-traversal protocol for IPv4. The adapter:
//! - Works with `--bind 0.0.0.0` (default)
//! - Works with `--bind ::` (dual-stack — uses IPv4 routing internally)
//! - Works with specific IPv4 addresses
//! - Rejects specific IPv6 addresses (e.g., Yggdrasil) with a helpful
//!   error
//!
//! ## Error handling
//!
//! All UPnP failures are non-fatal at the protocol level — the daemon
//! continues without port forwarding. Errors are returned as `Result<_,
//! String>` with operator-facing English text; callers log them
//! directly (no i18n needed for plumbing-level operator output).
//!
//! Available behind the `upnp` Cargo feature so consumers without UPnP
//! plumbing don't pay for `igd-next`.

use std::net::{IpAddr, SocketAddrV4};
use std::sync::RwLock;
use std::time::Duration;

pub use igd_next::PortMappingProtocol;
use igd_next::{Gateway, SearchOptions};
use tracing::{info, warn};

/// UPnP port mapping lease duration in seconds (1 hour).
const LEASE_DURATION: u32 = 3600;

/// Gateway search timeout — long enough for firewall approval dialogs
/// without leaving the daemon hanging at startup.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Bind address for the routing-detection probe.
const UDP_BIND_ADDRESS: &str = "0.0.0.0:0";

/// Remote address used by `get_local_ipv4` to make the OS pick a local
/// interface. No packets are sent — it's a routing-table query.
const ROUTING_TEST_ADDRESS: &str = "8.8.8.8:80";

// Operator-facing English error / log strings. Inline because they're
// plumbing-level — they go to operator logs and don't need translation
// (daemons don't expose UPnP errors to end users).

const ERR_PORT_FORWARD_TASK: &str = "Port forwarding task failed: ";
const ERR_SEARCH_TASK_FAILED: &str = "UPnP search task failed: ";
const ERR_GATEWAY_NOT_FOUND: &str = "UPnP gateway not found: ";
const ERR_GET_EXTERNAL_IP_TASK: &str = "Failed to get external IP task: ";
const ERR_GET_EXTERNAL_IP: &str = "Failed to get external IP: ";
const ERR_REMOVE_PORT_TASK: &str = "Remove port mapping task failed: ";
const ERR_RENEW_LEASE: &str = "Failed to renew lease: ";
const ERR_CREATE_UDP_SOCKET: &str = "Failed to create UDP socket: ";
const ERR_DETERMINE_ROUTING: &str = "Failed to determine routing: ";
const ERR_LOOPBACK_ONLY: &str = "Only loopback address available";
const ERR_IPV6_EXPECTED_IPV4: &str = "Local address is IPv6, expected IPv4";
const ERR_GET_LOCAL_ADDRESS: &str = "Failed to get local address: ";
const ERR_IPV6_NOT_SUPPORTED: &str = "UPnP is not supported for IPv6 addresses. Use IPv4 binding (e.g., --bind 0.0.0.0) for UPnP support.";

const LOG_CONFIGURED: &str = "UPnP configured";
const LOG_RENEWAL_FAILED: &str = "UPnP lease renewal failed";
const LOG_REDISCOVERING: &str = "UPnP rediscovering gateway";
const LOG_REDISCOVERED: &str = "UPnP gateway rediscovered";
const LOG_REDISCOVERY_FAILED: &str = "UPnP rediscovery failed";
const LOG_PORT_EXPIRE: &str = "UPnP port mappings may expire";

/// A single port to forward through the UPnP gateway.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PortMapping {
    pub protocol: PortMappingProtocol,
    pub port: u16,
}

/// UPnP gateway handle managing a configured list of port mappings.
///
/// Construct via [`UpnpGateway::setup`]. The returned handle owns the
/// gateway connection and the configured port list; renewals and
/// removals operate over that list.
pub struct UpnpGateway {
    gateway: RwLock<Gateway>,
    ports: Vec<PortMapping>,
    /// Local IPv4 + the *first* TCP port (informational only — used as
    /// the inner socket address for mappings, but each mapping uses
    /// its own port from `ports`).
    local_addr: SocketAddrV4,
    /// Description sent to the gateway alongside each mapping. Routers
    /// often surface this to the operator in their admin UI.
    description: String,
}

/// Add (or refresh) a single port mapping on the supplied gateway.
async fn add_port_mapping(
    gateway: &Gateway,
    mapping: PortMapping,
    local_ip: std::net::Ipv4Addr,
    description: &str,
) -> Result<(), String> {
    let socket = SocketAddrV4::new(local_ip, mapping.port);
    let gw = gateway.clone();
    let description = description.to_string();
    tokio::task::spawn_blocking(move || {
        gw.add_port(
            mapping.protocol,
            mapping.port,
            std::net::SocketAddr::V4(socket),
            LEASE_DURATION,
            &description,
        )
    })
    .await
    .map_err(|e| format!("{}{}", ERR_PORT_FORWARD_TASK, e))?
    .map_err(|e| e.to_string())?;
    Ok(())
}

impl UpnpGateway {
    /// Discover the LAN gateway and request port forwarding for each
    /// entry in `ports`.
    ///
    /// `bind_addr` is the daemon's bind IP. `0.0.0.0` and `::`
    /// (unspecified) trigger automatic local-IP detection. A specific
    /// IPv6 bind is rejected (UPnP is IPv4-only).
    ///
    /// `description` is a short human-readable string sent with each
    /// mapping; routers typically surface it in their admin UI (e.g.,
    /// `"Nexus BBS Server"`).
    ///
    /// # Errors
    ///
    /// Returns operator-facing English error strings on gateway-not-found,
    /// permission denied, and similar UPnP failures. All UPnP errors
    /// are recoverable — the daemon should continue without port
    /// forwarding.
    pub async fn setup(
        bind_addr: IpAddr,
        ports: Vec<PortMapping>,
        description: &str,
    ) -> Result<Self, String> {
        // UPnP only works with IPv4, but `::` (unspecified) binds IPv4
        // too via dual-stack.
        let local_addr = match bind_addr {
            IpAddr::V4(ipv4) => {
                if ipv4.is_unspecified() {
                    Self::get_local_ipv4()?
                } else {
                    ipv4
                }
            }
            IpAddr::V6(ipv6) => {
                if ipv6.is_unspecified() {
                    Self::get_local_ipv4()?
                } else {
                    return Err(ERR_IPV6_NOT_SUPPORTED.to_string());
                }
            }
        };

        let gateway = tokio::task::spawn_blocking(move || {
            igd_next::search_gateway(SearchOptions {
                timeout: Some(SEARCH_TIMEOUT),
                ..Default::default()
            })
        })
        .await
        .map_err(|e| format!("{}{}", ERR_SEARCH_TASK_FAILED, e))?
        .map_err(|e| format!("{}{}", ERR_GATEWAY_NOT_FOUND, e))?;

        let external_ip = tokio::task::spawn_blocking({
            let gateway = gateway.clone();
            move || gateway.get_external_ip()
        })
        .await
        .map_err(|e| format!("{}{}", ERR_GET_EXTERNAL_IP_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_GET_EXTERNAL_IP, e))?;

        // Add every requested mapping.
        for mapping in &ports {
            add_port_mapping(&gateway, *mapping, local_addr, description).await?;
        }

        // Group ports by protocol for the structured "configured" log.
        // Only emit a per-protocol field when at least one port of that
        // protocol was configured, so daemons that don't forward UDP
        // (e.g., the tracker) don't get an empty `udp_ports=""` field.
        let tcp_ports: Vec<u16> = ports
            .iter()
            .filter(|p| p.protocol == PortMappingProtocol::TCP)
            .map(|p| p.port)
            .collect();
        let udp_ports: Vec<u16> = ports
            .iter()
            .filter(|p| p.protocol == PortMappingProtocol::UDP)
            .map(|p| p.port)
            .collect();
        let tcp_list = port_list(&tcp_ports);
        let udp_list = port_list(&udp_ports);
        match (tcp_ports.is_empty(), udp_ports.is_empty()) {
            (false, false) => info!(
                external_ip = %external_ip,
                local_ip = %local_addr,
                tcp_ports = %tcp_list,
                udp_ports = %udp_list,
                "{}",
                LOG_CONFIGURED
            ),
            (false, true) => info!(
                external_ip = %external_ip,
                local_ip = %local_addr,
                tcp_ports = %tcp_list,
                "{}",
                LOG_CONFIGURED
            ),
            (true, false) => info!(
                external_ip = %external_ip,
                local_ip = %local_addr,
                udp_ports = %udp_list,
                "{}",
                LOG_CONFIGURED
            ),
            (true, true) => info!(
                external_ip = %external_ip,
                local_ip = %local_addr,
                "{}",
                LOG_CONFIGURED
            ),
        }

        // `local_addr` here is just informational (each mapping uses
        // its own port from `ports`). We pick the first port for the
        // socket-addr-display so the field has a sensible value.
        let representative_port = ports.first().map(|p| p.port).unwrap_or(0);

        Ok(Self {
            gateway: RwLock::new(gateway),
            ports,
            local_addr: SocketAddrV4::new(local_addr, representative_port),
            description: description.to_string(),
        })
    }

    /// Remove all configured port mappings from the router. Called
    /// during graceful shutdown. Failures are recoverable — leases
    /// expire on their own within `LEASE_DURATION` seconds.
    pub async fn remove_port_mapping(&self) -> Result<(), String> {
        let gateway = self
            .gateway
            .read()
            .expect("UPnP gateway lock poisoned")
            .clone();

        for mapping in &self.ports {
            let gw = gateway.clone();
            let mapping = *mapping;
            tokio::task::spawn_blocking(move || gw.remove_port(mapping.protocol, mapping.port))
                .await
                .map_err(|e| format!("{}{}", ERR_REMOVE_PORT_TASK, e))?
                .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Renew all configured port mappings (extends each lease by
    /// another `LEASE_DURATION` seconds). The background renewal task
    /// calls this every 30 minutes.
    pub async fn renew_lease(&self) -> Result<(), String> {
        let gateway = self
            .gateway
            .read()
            .expect("UPnP gateway lock poisoned")
            .clone();
        let local_ip = *self.local_addr.ip();

        for mapping in &self.ports {
            add_port_mapping(&gateway, *mapping, local_ip, &self.description)
                .await
                .map_err(|e| format!("{}{}", ERR_RENEW_LEASE, e))?;
        }

        Ok(())
    }

    /// Re-discover the gateway and re-add all configured mappings.
    /// Called when renewal fails (typically because the router
    /// rebooted and forgot the lease, or got a new LAN address).
    pub async fn rediscover_and_remap(&self) -> Result<(), String> {
        let local_ip = *self.local_addr.ip();

        let new_gateway = tokio::task::spawn_blocking(move || {
            igd_next::search_gateway(SearchOptions {
                timeout: Some(SEARCH_TIMEOUT),
                ..Default::default()
            })
        })
        .await
        .map_err(|e| format!("{}{}", ERR_SEARCH_TASK_FAILED, e))?
        .map_err(|e| format!("{}{}", ERR_GATEWAY_NOT_FOUND, e))?;

        for mapping in &self.ports {
            add_port_mapping(&new_gateway, *mapping, local_ip, &self.description).await?;
        }

        *self.gateway.write().expect("UPnP gateway lock poisoned") = new_gateway;
        Ok(())
    }

    /// Determine the local IPv4 address by asking the OS which
    /// interface would be used to reach an external address. No
    /// packets are sent — it's a pure routing-table query.
    fn get_local_ipv4() -> Result<std::net::Ipv4Addr, String> {
        use std::net::UdpSocket;

        let socket = UdpSocket::bind(UDP_BIND_ADDRESS)
            .map_err(|e| format!("{}{}", ERR_CREATE_UDP_SOCKET, e))?;
        socket
            .connect(ROUTING_TEST_ADDRESS)
            .map_err(|e| format!("{}{}", ERR_DETERMINE_ROUTING, e))?;

        match socket.local_addr() {
            Ok(addr) => match addr.ip() {
                IpAddr::V4(ipv4) if !ipv4.is_loopback() => Ok(ipv4),
                IpAddr::V4(_) => Err(ERR_LOOPBACK_ONLY.to_string()),
                IpAddr::V6(_) => Err(ERR_IPV6_EXPECTED_IPV4.to_string()),
            },
            Err(e) => Err(format!("{}{}", ERR_GET_LOCAL_ADDRESS, e)),
        }
    }
}

/// Build a comma-separated port list for log output.
fn port_list(ports: &[u16]) -> String {
    ports
        .iter()
        .map(u16::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Spawn a background task that keeps the gateway's port mappings
/// alive by re-adding them at half the lease duration (every 30
/// minutes). On renewal failure, the task tries to rediscover the
/// gateway (handles router reboots) before giving up.
///
/// The task should be aborted via `JoinHandle::abort` during graceful
/// shutdown.
pub fn spawn_lease_renewal_task(
    gateway: std::sync::Arc<UpnpGateway>,
) -> tokio::task::JoinHandle<()> {
    // Renew at 50% of lease duration to ensure we don't lose the mapping.
    let renewal_interval = Duration::from_secs(u64::from(LEASE_DURATION / 2));

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(renewal_interval);
        // Skip the first tick (immediate).
        interval.tick().await;

        loop {
            interval.tick().await;

            if let Err(e) = gateway.renew_lease().await {
                warn!(err = %e, "{}", LOG_RENEWAL_FAILED);
                info!("{}", LOG_REDISCOVERING);

                match gateway.rediscover_and_remap().await {
                    Ok(()) => {
                        info!("{}", LOG_REDISCOVERED);
                    }
                    Err(e2) => {
                        warn!(err = %e2, "{}", LOG_REDISCOVERY_FAILED);
                        warn!("{}", LOG_PORT_EXPIRE);
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lease_duration_is_one_hour() {
        assert_eq!(LEASE_DURATION, 3600);
    }

    #[test]
    fn test_search_timeout_is_reasonable() {
        // Long enough for firewall approval dialogs, short enough not
        // to hang startup.
        assert!(SEARCH_TIMEOUT.as_secs() >= 10);
        assert!(SEARCH_TIMEOUT.as_secs() <= 30);
    }

    #[test]
    fn test_get_local_ipv4_returns_non_loopback() {
        // Requires network connectivity but doesn't send packets.
        // May fail in isolated environments (containers, CI) — that's OK.
        if let Ok(ip) = UpnpGateway::get_local_ipv4() {
            assert!(!ip.is_loopback(), "should not return loopback");
            assert!(!ip.is_unspecified(), "should not return 0.0.0.0");
        }
    }

    #[test]
    fn test_udp_bind_address_is_valid() {
        let addr: std::net::SocketAddr = UDP_BIND_ADDRESS.parse().expect("valid socket address");
        assert!(addr.ip().is_unspecified());
        assert_eq!(addr.port(), 0);
    }

    #[test]
    fn test_routing_test_address_is_valid() {
        let addr: std::net::SocketAddr =
            ROUTING_TEST_ADDRESS.parse().expect("valid socket address");
        assert!(!addr.ip().is_unspecified());
        assert!(addr.port() > 0);
    }

    #[test]
    fn test_port_list_formatting() {
        assert_eq!(port_list(&[]), "");
        assert_eq!(port_list(&[7500]), "7500");
        assert_eq!(port_list(&[7500, 7501, 7502]), "7500, 7501, 7502");
    }
}
