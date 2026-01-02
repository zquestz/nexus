//! Network connection and message handling

mod connect;
mod constants;
mod stream;
pub mod tls;
mod types;

pub use connect::connect_to_server;
pub use stream::{NETWORK_RECEIVERS, ShutdownHandle, network_stream};
pub use types::{ConnectionParams, ProxyConfig};
