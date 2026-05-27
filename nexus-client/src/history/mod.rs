//! Chat history persistence for user message conversations
//!
//! Stores user message history on disk with the following structure:
//! `~/.local/share/nexus/history/{sha256(fingerprint)}/{sha256(fingerprint:user_id)}/{sha256(fold_name(other_nickname))}.enc`
//!
//! # Security Model
//!
//! **This is obfuscation, not security.** The encryption key is derived from the server's
//! certificate fingerprint, which is public information (visible to anyone who connects to
//! the server). This provides:
//!
//! - Protection against casual snooping (files aren't plaintext)
//! - No protection against determined attackers who know the server fingerprint
//!
//! The goal is to prevent accidental exposure of chat history, not to provide
//! cryptographic security guarantees.

mod crypto;
mod storage;

pub use storage::{HistoryManager, rotate_fingerprint};
