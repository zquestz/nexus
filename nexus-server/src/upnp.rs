//! UPnP/IGD port forwarding for the BBS server
//!
//! Thin wrapper over [`nexus_common::upnp`] that builds the BBS-specific
//! port list (TCP main + TCP transfer + UDP voice + optional WS ports)
//! and delegates discovery, lease renewal, and shutdown cleanup to the
//! common adapter.

use std::net::IpAddr;

use nexus_common::upnp::{PortMapping, PortMappingProtocol, UpnpGateway};

pub use nexus_common::upnp::{UpnpGateway as Gateway, spawn_lease_renewal_task};

/// Description sent to the router alongside each port mapping. Routers
/// typically surface this in their admin UI.
const PROTOCOL_DESCRIPTION: &str = "Nexus BBS";

/// Discover the LAN gateway and request port forwarding for the BBS
/// server's port set.
///
/// `main_port` is forwarded as TCP **and** UDP (UDP for voice, which
/// shares the BBS port number); `transfer_port` and the optional
/// WebSocket ports are TCP only.
///
/// All UPnP errors are recoverable — the server continues without port
/// forwarding. Returns operator-facing English error strings.
pub async fn setup(
    bind_addr: IpAddr,
    main_port: u16,
    transfer_port: u16,
    websocket_port: Option<u16>,
    transfer_websocket_port: Option<u16>,
) -> Result<UpnpGateway, String> {
    use PortMappingProtocol::{TCP, UDP};

    let mut ports = vec![
        PortMapping {
            protocol: TCP,
            port: main_port,
        },
        PortMapping {
            protocol: TCP,
            port: transfer_port,
        },
        PortMapping {
            protocol: UDP,
            port: main_port,
        },
    ];

    if let Some(port) = websocket_port {
        ports.push(PortMapping {
            protocol: TCP,
            port,
        });
    }
    if let Some(port) = transfer_websocket_port {
        ports.push(PortMapping {
            protocol: TCP,
            port,
        });
    }

    UpnpGateway::setup(bind_addr, ports, PROTOCOL_DESCRIPTION).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_description_is_set() {
        assert!(!PROTOCOL_DESCRIPTION.is_empty());
        assert!(PROTOCOL_DESCRIPTION.contains("Nexus"));
    }
}
