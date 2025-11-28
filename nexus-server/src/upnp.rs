//! UPnP/IGD port forwarding for NAT traversal

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
const ROUTING_TEST_ADDRESS: &str = "8.8.8.8:80";

/// UPnP gateway handle for managing port mappings
pub struct UpnpGateway {
    gateway: igd_next::Gateway,
    external_port: u16,
    local_addr: SocketAddrV4,
}

impl UpnpGateway {
    /// Search for UPnP gateway and request port forwarding
    ///
    /// # Arguments
    /// * `bind_addr` - The IP address the server is bound to
    /// * `port` - The port to forward
    ///
    /// # Returns
    /// * `Ok(UpnpGateway)` - Successfully configured port forwarding
    /// * `Err(String)` - Failed to configure (gateway not found, port forwarding failed, etc.)
    pub async fn setup(bind_addr: IpAddr, port: u16) -> Result<Self, String> {
        // UPnP only works with IPv4
        let local_addr = match bind_addr {
            IpAddr::V4(ipv4) => {
                // If bound to 0.0.0.0, we need to detect the actual local IP
                if ipv4.is_unspecified() {
                    Self::get_local_ipv4()?
                } else {
                    ipv4
                }
            }
            IpAddr::V6(_) => {
                return Err(ERR_IPV6_NOT_SUPPORTED.to_string());
            }
        };

        let local_socket = SocketAddrV4::new(local_addr, port);

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

        // Request port forwarding
        println!(
            "{}{}:{} -> {}:{}",
            MSG_REQUESTING_PORT_FORWARD, external_ip, port, local_addr, port
        );

        tokio::task::spawn_blocking({
            let gateway = gateway.clone();
            move || {
                gateway.add_port(
                    igd_next::PortMappingProtocol::TCP,
                    port,
                    std::net::SocketAddr::V4(local_socket),
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
            MSG_UPNP_CONFIGURED, external_ip, port, local_addr, port
        );

        Ok(Self {
            gateway,
            external_port: port,
            local_addr: local_socket,
        })
    }

    /// Remove port forwarding mapping
    pub async fn remove_port_mapping(&self) -> Result<(), String> {
        let gateway = self.gateway.clone();
        let external_port = self.external_port;

        tokio::task::spawn_blocking(move || {
            gateway.remove_port(igd_next::PortMappingProtocol::TCP, external_port)
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_REMOVE_PORT_MAPPING, e))?;

        Ok(())
    }

    /// Renew the port mapping lease
    pub async fn renew_lease(&self) -> Result<(), String> {
        let gateway = self.gateway.clone();
        let external_port = self.external_port;

        let local_addr = self.local_addr;

        tokio::task::spawn_blocking(move || {
            gateway.add_port(
                igd_next::PortMappingProtocol::TCP,
                external_port,
                std::net::SocketAddr::V4(local_addr),
                LEASE_DURATION,
                PROTOCOL_DESCRIPTION,
            )
        })
        .await
        .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE_TASK, e))?
        .map_err(|e| format!("{}{}", ERR_UPNP_RENEW_LEASE, e))?;

        Ok(())
    }

    /// Get the local IPv4 address by connecting to a public IP
    /// This helps determine the actual interface when bound to 0.0.0.0
    fn get_local_ipv4() -> Result<std::net::Ipv4Addr, String> {
        use std::net::{TcpStream, ToSocketAddrs};

        // Try to connect to a public DNS server to determine our local IP
        // We don't actually send any data, just use this to determine routing
        // Use a shorter timeout since this might fail on isolated networks
        if let Ok(addrs) = ROUTING_TEST_ADDRESS.to_socket_addrs() {
            if let Some(addr) = addrs.into_iter().next() {
                if let Ok(socket) = TcpStream::connect_timeout(&addr, Duration::from_millis(500)) {
                    if let Ok(local_addr) = socket.local_addr() {
                        if let IpAddr::V4(ipv4) = local_addr.ip() {
                            return Ok(ipv4);
                        }
                    }
                }
            }
        }

        // Fallback: try to find a non-loopback IPv4 address from network interfaces
        // This works even without internet connectivity
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
/// # Arguments
/// * `gateway` - The UPnP gateway handle (Arc-wrapped for shared access)
/// * `renewal_interval` - How often to renew the lease (should be less than LEASE_DURATION)
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
                eprintln!("{}", WARN_UPNP_PORT_EXPIRE);
            }
        }
    })
}
