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

use crate::constants::*;
use igd_next::SearchOptions;
use std::net::{IpAddr, SocketAddrV4};
use std::time::Duration;

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
    gateway: std::sync::RwLock<igd_next::Gateway>,
    /// Main BBS port
    main_port: u16,
    /// Transfer port for file downloads
    transfer_port: u16,
    local_addr: SocketAddrV4,
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
    ///
    /// # Returns
    /// * `Ok(UpnpGateway)` - Successfully configured port forwarding
    /// * `Err(String)` - Failed to configure (gateway not found, port forwarding failed, etc.)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use std::net::IpAddr;
    /// # async fn example() -> Result<(), String> {
    /// let gateway = UpnpGateway::setup("0.0.0.0".parse().unwrap(), 7500, 7501).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn setup(
        bind_addr: IpAddr,
        main_port: u16,
        transfer_port: u16,
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

        let local_addr_only = local_addr;

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

        // Request port forwarding for main BBS port
        println!(
            "{}{}:{} -> {}:{}",
            MSG_REQUESTING_PORT_FORWARD, external_ip, main_port, local_addr_only, main_port
        );

        let main_socket = SocketAddrV4::new(local_addr_only, main_port);
        tokio::task::spawn_blocking({
            let gateway = gateway.clone();
            move || {
                gateway.add_port(
                    igd_next::PortMappingProtocol::TCP,
                    main_port,
                    std::net::SocketAddr::V4(main_socket),
                    LEASE_DURATION,
                    PROTOCOL_DESCRIPTION,
                )
            }
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_PORT_FORWARD_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_ADD_PORT_MAPPING, e))?;

        println!(
            "{}{}:{} -> {}:{}",
            MSG_UPNP_CONFIGURED, external_ip, main_port, local_addr_only, main_port
        );

        // Request port forwarding for transfer port
        println!(
            "{}{}:{} -> {}:{}",
            MSG_REQUESTING_PORT_FORWARD, external_ip, transfer_port, local_addr_only, transfer_port
        );

        let transfer_socket = SocketAddrV4::new(local_addr_only, transfer_port);
        tokio::task::spawn_blocking({
            let gateway = gateway.clone();
            move || {
                gateway.add_port(
                    igd_next::PortMappingProtocol::TCP,
                    transfer_port,
                    std::net::SocketAddr::V4(transfer_socket),
                    LEASE_DURATION,
                    PROTOCOL_DESCRIPTION,
                )
            }
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_PORT_FORWARD_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_ADD_PORT_MAPPING, e))?;

        println!(
            "{}{}:{} -> {}:{}",
            MSG_UPNP_CONFIGURED, external_ip, transfer_port, local_addr_only, transfer_port
        );

        Ok(Self {
            gateway: std::sync::RwLock::new(gateway),
            main_port,
            transfer_port,
            local_addr: main_socket,
        })
    }

    /// Remove port forwarding mappings from the router
    ///
    /// This is called during graceful shutdown to clean up the UPnP port mappings.
    /// If removal fails, the mappings will expire after the lease duration (1 hour).
    pub async fn remove_port_mapping(&self) -> Result<(), String> {
        let gateway = self.gateway.read().unwrap().clone();

        // Remove main port mapping
        let main_port = self.main_port;
        let gw = gateway.clone();
        tokio::task::spawn_blocking(move || {
            gw.remove_port(igd_next::PortMappingProtocol::TCP, main_port)
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_MAPPING, e))?;

        // Remove transfer port mapping
        let transfer_port = self.transfer_port;
        tokio::task::spawn_blocking(move || {
            gateway.remove_port(igd_next::PortMappingProtocol::TCP, transfer_port)
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_MAPPING, e))?;

        Ok(())
    }

    /// Renew the port mapping leases
    ///
    /// Extends the port forwarding leases for another hour. This is called automatically
    /// by the background renewal task every 30 minutes.
    pub async fn renew_lease(&self) -> Result<(), String> {
        let gateway = self.gateway.read().unwrap().clone();
        let local_addr = self.local_addr;

        // Renew main port lease
        let main_port = self.main_port;
        let main_socket = SocketAddrV4::new(*local_addr.ip(), main_port);
        let gw = gateway.clone();

        tokio::task::spawn_blocking(move || {
            gw.add_port(
                igd_next::PortMappingProtocol::TCP,
                main_port,
                std::net::SocketAddr::V4(main_socket),
                LEASE_DURATION,
                PROTOCOL_DESCRIPTION,
            )
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;

        // Renew transfer port lease
        let transfer_port = self.transfer_port;
        let transfer_socket = SocketAddrV4::new(*local_addr.ip(), transfer_port);

        tokio::task::spawn_blocking(move || {
            gateway.add_port(
                igd_next::PortMappingProtocol::TCP,
                transfer_port,
                std::net::SocketAddr::V4(transfer_socket),
                LEASE_DURATION,
                PROTOCOL_DESCRIPTION,
            )
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;

        Ok(())
    }

    /// Re-discover gateway and re-establish port mappings
    ///
    /// Called when lease renewal fails, attempts to find the gateway again
    /// (in case router rebooted) and re-add the port mappings.
    pub async fn rediscover_and_remap(&self) -> Result<(), String> {
        let local_addr = self.local_addr;
        let main_port = self.main_port;
        let transfer_port = self.transfer_port;

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

        // Add main port mapping
        let main_socket = SocketAddrV4::new(*local_addr.ip(), main_port);
        let gw = new_gateway.clone();
        tokio::task::spawn_blocking(move || {
            gw.add_port(
                igd_next::PortMappingProtocol::TCP,
                main_port,
                std::net::SocketAddr::V4(main_socket),
                LEASE_DURATION,
                PROTOCOL_DESCRIPTION,
            )
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_PORT_FORWARD_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_ADD_PORT_MAPPING, e))?;

        // Add transfer port mapping
        let transfer_socket = SocketAddrV4::new(*local_addr.ip(), transfer_port);
        let gw = new_gateway.clone();
        tokio::task::spawn_blocking(move || {
            gw.add_port(
                igd_next::PortMappingProtocol::TCP,
                transfer_port,
                std::net::SocketAddr::V4(transfer_socket),
                LEASE_DURATION,
                PROTOCOL_DESCRIPTION,
            )
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_PORT_FORWARD_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_ADD_PORT_MAPPING, e))?;

        // Update stored gateway
        *self.gateway.write().unwrap() = new_gateway;

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
                eprintln!("{}{}", WARN_UPNP_RENEW_FAILED, e);
                eprintln!("{}", MSG_UPNP_REDISCOVERING);

                // Try to re-discover gateway and re-establish mappings
                match gateway.rediscover_and_remap().await {
                    Ok(()) => {
                        eprintln!("{}", MSG_UPNP_REDISCOVERED);
                    }
                    Err(e2) => {
                        eprintln!("{}{}", WARN_UPNP_REDISCOVER_FAILED, e2);
                        eprintln!("{}", WARN_UPNP_PORT_EXPIRE);
                    }
                }
            }
        }
    })
}
