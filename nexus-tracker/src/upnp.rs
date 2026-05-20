//! UPnP/IGD port forwarding: a thin wrapper over [`nexus_common::upnp`] that builds the tracker's
//! port list and delegates discovery / renewal / cleanup. The tracker has no UDP (no voice), so
//! every mapping is TCP.

use std::net::IpAddr;

use nexus_common::upnp::{PortMapping, PortMappingProtocol, UpnpGateway};

pub use nexus_common::upnp::{UpnpGateway as Gateway, spawn_lease_renewal_task};

/// Description sent to the router alongside each port mapping. Routers
/// typically surface this in their admin UI.
const PROTOCOL_DESCRIPTION: &str = "Nexus Tracker";

/// Discover the LAN gateway and forward `main_port` (and `websocket_port` if `Some`) as TCP.
///
/// # Errors
///
/// Operator-facing strings on failure (no gateway, IPv6 bind, etc.); all recoverable — the
/// tracker continues without forwarding.
pub async fn setup(
    bind_addr: IpAddr,
    main_port: u16,
    websocket_port: Option<u16>,
) -> Result<UpnpGateway, String> {
    let mut ports = vec![PortMapping {
        protocol: PortMappingProtocol::TCP,
        port: main_port,
    }];

    if let Some(port) = websocket_port {
        ports.push(PortMapping {
            protocol: PortMappingProtocol::TCP,
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
