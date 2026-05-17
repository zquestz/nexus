//! Bandwidth scheduler chunk size validation
//!
//! Validates the chunk size (in bytes) at which the WF2Q+ outbound
//! scheduler dispatches packets. Smaller chunks bound latency for short
//! messages competing with bulk transfers; larger chunks reduce
//! scheduling overhead.
//!
//! Called at the protocol boundary by the server-info-update handler,
//! and again at the DB layer as defense-in-depth.

/// 1 KB — small enough that any practical cap drains a chunk in under a
/// few milliseconds, keeping the per-message latency bound tight.
pub const MIN_BANDWIDTH_CHUNK_SIZE: u32 = 1024;

/// 64 KB — above this, the per-chunk latency bound exceeds typical
/// interactive thresholds even on fast links.
pub const MAX_BANDWIDTH_CHUNK_SIZE: u32 = 65536;

/// 8 KB — default chunk size. Matches the value seeded by the
/// `scheduler_chunk_size` config migration. Balances latency for short
/// messages against scheduling overhead for bulk transfers.
pub const DEFAULT_BANDWIDTH_CHUNK_SIZE: u32 = 8192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BandwidthChunkSizeError {
    TooSmall,
    TooLarge,
}

pub fn validate_bandwidth_chunk_size(size: u32) -> Result<(), BandwidthChunkSizeError> {
    if size < MIN_BANDWIDTH_CHUNK_SIZE {
        return Err(BandwidthChunkSizeError::TooSmall);
    }
    if size > MAX_BANDWIDTH_CHUNK_SIZE {
        return Err(BandwidthChunkSizeError::TooLarge);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_sizes() {
        assert!(validate_bandwidth_chunk_size(MIN_BANDWIDTH_CHUNK_SIZE).is_ok());
        assert!(validate_bandwidth_chunk_size(8192).is_ok());
        assert!(validate_bandwidth_chunk_size(MAX_BANDWIDTH_CHUNK_SIZE).is_ok());
    }

    #[test]
    fn test_too_small() {
        assert_eq!(
            validate_bandwidth_chunk_size(0),
            Err(BandwidthChunkSizeError::TooSmall)
        );
        assert_eq!(
            validate_bandwidth_chunk_size(MIN_BANDWIDTH_CHUNK_SIZE - 1),
            Err(BandwidthChunkSizeError::TooSmall)
        );
    }

    #[test]
    fn test_too_large() {
        assert_eq!(
            validate_bandwidth_chunk_size(MAX_BANDWIDTH_CHUNK_SIZE + 1),
            Err(BandwidthChunkSizeError::TooLarge)
        );
        assert_eq!(
            validate_bandwidth_chunk_size(u32::MAX),
            Err(BandwidthChunkSizeError::TooLarge)
        );
    }
}
