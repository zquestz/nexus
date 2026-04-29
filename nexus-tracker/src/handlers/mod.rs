//! Tracker request handlers
//!
//! Each submodule handles one message type. Handlers are pure functions
//! over their inputs (decoded message + connection context) and write a
//! response via the supplied [`FrameWriter`].
//!
//! [`FrameWriter`]: nexus_common::framing::FrameWriter

pub mod handshake;
