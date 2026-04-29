//! UPnP/IGD port forwarding for NAT traversal
//!
//! This module provides automatic port forwarding using the UPnP/IGD (Universal Plug and Play /
//! Internet Gateway Device) protocol. When enabled with the `--upnp` flag, the server will:
//!
//! - Discover the local router via multicast
//! - Request TCP port forwarding with a 1-hour lease
//! - Display the external IP address
//! - Automatically renew the lease every 30 minutes
//! - Clean up port mapping on graceful shutdown
//!
//! ## Usage
//!
//! ```bash
//! nexusd --upnp                    # Enable UPnP with default port
//! nexusd --upnp --port 8080        # Enable UPnP with custom port
//! ```
//!
//! ## How It Works
//!
//! 1. **Gateway Discovery**: Sends multicast packets to discover UPnP-capable routers
//! 2. **Port Mapping**: Requests TCP port forwarding from router's WAN to server's LAN
//! 3. **Lease Renewal**: Background task renews mapping every 30 minutes (50% of lease duration)
//! 4. **Cleanup**: Removes port mapping when server shuts down cleanly (Ctrl+C)
//!
//! ## IPv4 Only
//!
//! UPnP/IGD is designed for IPv4 NAT traversal. The module:
//! - Works with `--bind 0.0.0.0` (default)
//! - Works with `--bind ::` (dual-stack mode, uses IPv4 routing)
//! - Works with specific IPv4 addresses
//! - Rejects specific IPv6 addresses (like Yggdrasil) with a helpful error
//!
//! ## Local IP Detection
//!
//! When bound to `0.0.0.0` or `::`, the module detects the actual local IP by creating a UDP
//! socket and checking which interface the OS would use to reach a remote address. This is a
//! pure routing table lookup - no packets are actually sent.
//!
//! ## Error Handling
//!
//! All UPnP failures are non-fatal. If UPnP setup fails, the server continues without port
//! forwarding and prints a warning suggesting manual configuration.

use std::net::{IpAddr, SocketAddrV4};
use std::sync::RwLock;
use std::time::Duration;

use igd_next::SearchOptions;
use tracing::{info, warn};

use crate::constants::*;

/// UPnP port mapping lease duration (in seconds)
/// 3600 seconds = 1 hour
const LEASE_DURATION: u32 = 3600;

/// UPnP gateway search timeout (allows time for firewall approval dialogs)
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);

/// Protocol description for UPnP mapping
const PROTOCOL_DESCRIPTION: &str = "Nexus BBS Server";

/// Network addresses for routing detection
const UDP_BIND_ADDRESS: &str = "0.0.0.0:0";

/// Remote address for local routing table lookup (no actual connection is made)
const ROUTING_TEST_ADDRESS: &str = "8.8.8.8:80";

/// UPnP gateway handle for managing port mappings
pub struct UpnpGateway {
    gateway: RwLock<igd_next::Gateway>,
    /// Main BBS port
    main_port: u16,
    /// Transfer port for file downloads
    transfer_port: u16,
    /// WebSocket BBS port (None if WebSocket disabled)
    websocket_port: Option<u16>,
    /// WebSocket transfer port (None if WebSocket disabled)
    transfer_websocket_port: Option<u16>,
    local_addr: SocketAddrV4,
}

/// Add a port mapping to the gateway
///
/// This is a helper to reduce duplication across setup, renewal, and rediscovery.
async fn add_port_mapping(
    gateway: &igd_next::Gateway,
    protocol: igd_next::PortMappingProtocol,
    port: u16,
    local_ip: std::net::Ipv4Addr,
) -> Result<(), String> {
    let socket = SocketAddrV4::new(local_ip, port);
    let gw = gateway.clone();
    tokio::task::spawn_blocking(move || {
        gw.add_port(
            protocol,
            port,
            std::net::SocketAddr::V4(socket),
            LEASE_DURATION,
            PROTOCOL_DESCRIPTION,
        )
    })
    .await
    .map_err(|e| format!("{}{}", ERR_UPNP_PORT_FORWARD_TASK, e))?
    .map_err(|e| e.to_string())?;
    Ok(())
}

impl UpnpGateway {
    /// Search for UPnP gateway and request port forwarding
    ///
    /// This performs the complete UPnP setup sequence:
    /// 1. Determines the local IPv4 address (detects if bound to 0.0.0.0 or ::)
    /// 2. Discovers UPnP gateway on the network (15-second timeout)
    /// 3. Retrieves the external IP address from the gateway
    /// 4. Requests TCP port forwarding with a 1-hour lease
    ///
    /// # Arguments
    /// * `bind_addr` - The IP address the server is bound to
    /// * `main_port` - The main BBS port to forward
    /// * `transfer_port` - The transfer port to forward
    /// * `websocket_port` - The WebSocket BBS port to forward (None if disabled)
    /// * `transfer_websocket_port` - The WebSocket transfer port to forward (None if disabled)
    ///
    /// # Returns
    /// * `Ok(UpnpGateway)` - Successfully configured port forwarding
    /// * `Err(String)` - Failed to configure (gateway not found, port forwarding failed, etc.)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::net::IpAddr;
    /// # use nexus_server::upnp::UpnpGateway;
    /// # async fn example() -> Result<(), String> {
    /// let bind_addr: IpAddr = "0.0.0.0".parse().expect("valid IP address");
    /// let gateway = UpnpGateway::setup(bind_addr, 7500, 7501, Some(7502), Some(7503)).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn setup(
        bind_addr: IpAddr,
        main_port: u16,
        transfer_port: u16,
        websocket_port: Option<u16>,
        transfer_websocket_port: Option<u16>,
    ) -> Result<Self, String> {
        // UPnP only works with IPv4, but :: (dual-stack) binds IPv4 too
        let local_addr = match bind_addr {
            IpAddr::V4(ipv4) => {
                // If bound to 0.0.0.0, we need to detect the actual local IP
                if ipv4.is_unspecified() {
                    Self::get_local_ipv4()?
                } else {
                    ipv4
                }
            }
            IpAddr::V6(ipv6) => {
                // :: (unspecified) enables dual-stack, so UPnP can work for IPv4
                if ipv6.is_unspecified() {
                    Self::get_local_ipv4()?
                } else {
                    // Specific IPv6 address (like Yggdrasil) - UPnP won't work
                    return Err(ERR_IPV6_NOT_SUPPORTED.to_string());
                }
            }
        };

        // Search for gateway with timeout
        let gateway = tokio::task::spawn_blocking(move || {
            igd_next::search_gateway(SearchOptions {
                timeout: Some(SEARCH_TIMEOUT),
                ..Default::default()
            })
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_SEARCH_TASK_FAILED, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_GATEWAY_NOT_FOUND, e))?;

        // Get external IP address
        let external_ip = tokio::task::spawn_blocking({
            let gateway = gateway.clone();
            move || gateway.get_external_ip()
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_GET_EXTERNAL_IP_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_GET_EXTERNAL_IP, e))?;

        use igd_next::PortMappingProtocol::{TCP, UDP};

        // Request port forwarding for all TCP ports
        add_port_mapping(&gateway, TCP, main_port, local_addr).await?;
        add_port_mapping(&gateway, TCP, transfer_port, local_addr).await?;

        // Request UDP port forwarding for voice (same port as main)
        add_port_mapping(&gateway, UDP, main_port, local_addr).await?;

        // Build port list for summary message
        let mut tcp_ports = vec![main_port, transfer_port];

        if let Some(ws_port) = websocket_port {
            add_port_mapping(&gateway, TCP, ws_port, local_addr).await?;
            tcp_ports.push(ws_port);
        }

        if let Some(ws_transfer_port) = transfer_websocket_port {
            add_port_mapping(&gateway, TCP, ws_transfer_port, local_addr).await?;
            tcp_ports.push(ws_transfer_port);
        }

        // Print single summary line
        let tcp_port_list = tcp_ports
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        info!(
            external_ip = %external_ip,
            local_ip = %local_addr,
            tcp_ports = %tcp_port_list,
            udp_port = main_port,
            "{}", LOG_UPNP_CONFIGURED
        );

        Ok(Self {
            gateway: RwLock::new(gateway),
            main_port,
            transfer_port,
            websocket_port,
            transfer_websocket_port,
            local_addr: SocketAddrV4::new(local_addr, main_port),
        })
    }

    /// Remove port forwarding mappings from the router
    ///
    /// This is called during graceful shutdown to clean up the UPnP port mappings.
    /// If removal fails, the mappings will expire after the lease duration (1 hour).
    pub async fn remove_port_mapping(&self) -> Result<(), String> {
        let gateway = self
            .gateway
            .read()
            .expect("UPnP gateway lock poisoned")
            .clone();

        // Remove main port mapping
        let main_port = self.main_port;
        let gw = gateway.clone();
        tokio::task::spawn_blocking(move || {
            gw.remove_port(igd_next::PortMappingProtocol::TCP, main_port)
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
        .map_err(|e| e.to_string())?;

        // Remove transfer port mapping
        let transfer_port = self.transfer_port;
        let gw2 = gateway.clone();
        tokio::task::spawn_blocking(move || {
            gw2.remove_port(igd_next::PortMappingProtocol::TCP, transfer_port)
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
        .map_err(|e| e.to_string())?;

        // Remove UDP voice port mapping (same port as main)
        let voice_port = self.main_port;
        let gw_voice = gateway.clone();
        tokio::task::spawn_blocking(move || {
            gw_voice.remove_port(igd_next::PortMappingProtocol::UDP, voice_port)
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
        .map_err(|e| e.to_string())?;

        // Remove WebSocket BBS port mapping if enabled
        if let Some(websocket_port) = self.websocket_port {
            let gw3 = gateway.clone();
            tokio::task::spawn_blocking(move || {
                gw3.remove_port(igd_next::PortMappingProtocol::TCP, websocket_port)
            })
            .await
            .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
            .map_err(|e| e.to_string())?;
        }

        // Remove WebSocket transfer port mapping if enabled
        if let Some(transfer_websocket_port) = self.transfer_websocket_port {
            tokio::task::spawn_blocking(move || {
                gateway.remove_port(igd_next::PortMappingProtocol::TCP, transfer_websocket_port)
            })
            .await
            .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Renew the port mapping leases
    ///
    /// Extends the port forwarding leases for another hour. This is called automatically
    /// by the background renewal task every 30 minutes.
    pub async fn renew_lease(&self) -> Result<(), String> {
        let gateway = self
            .gateway
            .read()
            .expect("UPnP gateway lock poisoned")
            .clone();
        let local_ip = *self.local_addr.ip();

        use igd_next::PortMappingProtocol::{TCP, UDP};

        // Renew main port lease (TCP)
        add_port_mapping(&gateway, TCP, self.main_port, local_ip)
            .await
            .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;

        // Renew transfer port lease (TCP)
        add_port_mapping(&gateway, TCP, self.transfer_port, local_ip)
            .await
            .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;

        // Renew voice port lease (UDP, same port as main)
        add_port_mapping(&gateway, UDP, self.main_port, local_ip)
            .await
            .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;

        // Renew WebSocket BBS port lease if enabled
        if let Some(websocket_port) = self.websocket_port {
            add_port_mapping(&gateway, TCP, websocket_port, local_ip)
                .await
                .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;
        }

        // Renew WebSocket transfer port lease if enabled
        if let Some(transfer_websocket_port) = self.transfer_websocket_port {
            add_port_mapping(&gateway, TCP, transfer_websocket_port, local_ip)
                .await
                .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;
        }

        Ok(())
    }

    /// Re-discover gateway and re-establish port mappings
    ///
    /// Called when lease renewal fails, attempts to find the gateway again
    /// (in case router rebooted) and re-add the port mappings.
    pub async fn rediscover_and_remap(&self) -> Result<(), String> {
        let local_ip = *self.local_addr.ip();

        // Search for gateway again
        let new_gateway = tokio::task::spawn_blocking(move || {
            igd_next::search_gateway(SearchOptions {
                timeout: Some(SEARCH_TIMEOUT),
                ..Default::default()
            })
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_SEARCH_TASK_FAILED, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_GATEWAY_NOT_FOUND, e))?;

        use igd_next::PortMappingProtocol::{TCP, UDP};

        // Add main port mapping (TCP)
        add_port_mapping(&new_gateway, TCP, self.main_port, local_ip).await?;

        // Add transfer port mapping (TCP)
        add_port_mapping(&new_gateway, TCP, self.transfer_port, local_ip).await?;

        // Add voice port mapping (UDP, same port as main)
        add_port_mapping(&new_gateway, UDP, self.main_port, local_ip).await?;

        // Add WebSocket BBS port mapping if enabled
        if let Some(websocket_port) = self.websocket_port {
            add_port_mapping(&new_gateway, TCP, websocket_port, local_ip).await?;
        }

        // Add WebSocket transfer port mapping if enabled
        if let Some(transfer_websocket_port) = self.transfer_websocket_port {
            add_port_mapping(&new_gateway, TCP, transfer_websocket_port, local_ip).await?;
        }

        // Update stored gateway
        *self.gateway.write().expect("UPnP gateway lock poisoned") = new_gateway;

        Ok(())
    }

    /// Get the local IPv4 address using UDP socket routing
    ///
    /// This helps determine the actual interface when bound to 0.0.0.0 or ::.
    /// Creates a UDP socket and "connects" to a remote address, which causes the OS
    /// to determine which local interface would be used. No actual packets are sent.
    fn get_local_ipv4() -> Result<std::net::Ipv4Addr, String> {
        use std::net::UdpSocket;

        // Bind UDP socket to 0.0.0.0:0 and connect to a remote address
        // This doesn't actually send packets but OS routing determines the interface
        let socket = UdpSocket::bind(UDP_BIND_ADDRESS)
            .map_err(|e| format!("{}{}", ERR_UPNP_CREATE_UDP_SOCKET, e))?;

        // Try to "connect" to a remote address to determine routing
        // This is purely local routing table lookup, no packets sent
        socket
            .connect(ROUTING_TEST_ADDRESS)
            .map_err(|e| format!("{}{}", ERR_UPNP_DETERMINE_ROUTING, e))?;

        match socket.local_addr() {
            Ok(addr) => match addr.ip() {
                IpAddr::V4(ipv4) if !ipv4.is_loopback() => Ok(ipv4),
                IpAddr::V4(_) => Err(ERR_UPNP_LOOPBACK_ONLY.to_string()),
                IpAddr::V6(_) => Err(ERR_UPNP_IPV6_EXPECTED_IPV4.to_string()),
            },
            Err(e) => Err(format!("{}{}", ERR_UPNP_GET_LOCAL_ADDRESS, e)),
        }
    }
}

/// Background task to periodically renew UPnP lease
///
/// Spawns a tokio task that renews the port mapping every 30 minutes (50% of the
/// 1-hour lease duration). This ensures the port mapping never expires while the
/// server is running.
///
/// If renewal fails, attempts to re-discover the gateway and re-establish mappings.
/// The task should be aborted during server shutdown.
///
/// # Arguments
/// * `gateway` - The UPnP gateway handle (Arc-wrapped for shared access)
///
/// # Returns
/// A tokio task handle that can be aborted to stop renewal
pub fn spawn_lease_renewal_task(
    gateway: std::sync::Arc<UpnpGateway>,
) -> tokio::task::JoinHandle<()> {
    // Renew at 50% of lease duration to ensure we don't lose the mapping
    let renewal_interval = Duration::from_secs((LEASE_DURATION / 2) as u64);

    tokio::spawn(async move {
        let mut interval = tokio::time::interval(renewal_interval);
        // Skip the first tick (immediate)
        interval.tick().await;

        loop {
            interval.tick().await;

            if let Err(e) = gateway.renew_lease().await {
                warn!(err = %e, "{}", LOG_UPNP_RENEWAL_FAILED);
                info!("{}", LOG_UPNP_REDISCOVERING);

                // Try to re-discover gateway and re-establish mappings
                match gateway.rediscover_and_remap().await {
                    Ok(()) => {
                        info!("{}", LOG_UPNP_REDISCOVERED);
                    }
                    Err(e2) => {
                        warn!(err = %e2, "{}", LOG_UPNP_REDISCOVERY_FAILED);
                        warn!("{}", LOG_UPNP_PORT_EXPIRE);
                    }
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==========================================================================
    // Constants tests
    // ==========================================================================

    #[test]
    fn test_lease_duration_is_one_hour() {
        assert_eq!(LEASE_DURATION, 3600);
    }

    #[test]
    fn test_search_timeout_is_reasonable() {
        // Should be long enough for firewall dialogs but not too long
        assert!(SEARCH_TIMEOUT.as_secs() >= 10);
        assert!(SEARCH_TIMEOUT.as_secs() <= 30);
    }

    // ==========================================================================
    // get_local_ipv4 tests
    // ==========================================================================

    #[test]
    fn test_get_local_ipv4_returns_non_loopback() {
        // This test requires network connectivity but doesn't send packets
        match UpnpGateway::get_local_ipv4() {
            Ok(ip) => {
                assert!(!ip.is_loopback(), "Should not return loopback address");
                assert!(!ip.is_unspecified(), "Should not return 0.0.0.0");
            }
            Err(_) => {
                // May fail in isolated environments (containers, CI) - that's OK
            }
        }
    }

    #[test]
    fn test_udp_bind_address_is_valid() {
        // Verify the constant is a valid socket address
        let addr: std::net::SocketAddr = UDP_BIND_ADDRESS.parse().expect("valid socket address");
        assert!(addr.ip().is_unspecified());
        assert_eq!(addr.port(), 0);
    }

    #[test]
    fn test_routing_test_address_is_valid() {
        // Verify the constant is a valid socket address
        let addr: std::net::SocketAddr =
            ROUTING_TEST_ADDRESS.parse().expect("valid socket address");
        assert!(!addr.ip().is_unspecified());
        assert!(addr.port() > 0);
    }
}
