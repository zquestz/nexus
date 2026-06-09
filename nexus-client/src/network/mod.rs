//! Network connection and message handling

mod connect;
mod constants;
mod stream;
pub mod tls;
mod tracker_query;
pub mod types;

pub use connect::connect_to_server;
pub use constants::{CONNECTION_TIMEOUT, DNS_LOOKUP_TIMEOUT, FEATURE_FILES, FEATURE_NEWS};
pub use stream::{NETWORK_RECEIVERS, ShutdownHandle, network_stream};
pub use tracker_query::query_tracker;
pub use types::{
    ConnectionParams, FingerprintInterception, FingerprintMismatchDetails, ProxyConfig,
    TrackerQueryError, TrackerQueryOk, TrackerQueryParams,
};
