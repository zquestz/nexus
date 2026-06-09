//! Storage operations for chat history files
//!
//! Handles reading, writing, and managing obfuscated chat history files on disk.
//! Files are organized as:
//! `~/.local/share/nexus/history/{sha256(fingerprint)}/{sha256(fingerprint:user_id)}/{sha256(fold_name(other_nickname))}.enc`
//!
//! The per-account segment is keyed by the immutable user id (not the username),
//! so a rename never moves the directory. See the parent module for security
//! model details (obfuscation, not encryption).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nexus_common::names::fold_name;
use nexus_common::protocol::ServerMessage;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::crypto::{CryptoError, HistoryCrypto};
use crate::config::settings::ChatHistoryRetention;

/// File extension for encrypted history files
const HISTORY_FILE_EXT: &str = "enc";

/// Unix file permissions for history files (owner read/write only)
#[cfg(unix)]
const FILE_PERMISSIONS: u32 = 0o600;

/// Unix directory permissions for history directories
#[cfg(unix)]
const DIR_PERMISSIONS: u32 = 0o700;

/// File format for storing conversations
/// Wraps messages with metadata to avoid deriving other_nickname from message content
#[derive(Serialize, Deserialize)]
struct ConversationFile {
    /// The other party's nickname (stable across our nickname changes)
    other_nickname: String,
    /// The messages in this conversation
    messages: Vec<ServerMessage>,
}

/// Manages chat history storage for a single server connection
pub struct HistoryManager {
    /// Base directory for this server+account combination, keyed by the immutable
    /// user id so a rename never moves it:
    /// `~/.local/share/nexus/history/{sha256(fingerprint)}/{sha256(fingerprint:user_id)}/`
    base_dir: PathBuf,
    /// Crypto instance for this server
    crypto: HistoryCrypto,
    /// Cached conversations: other_nickname -> messages
    conversations: HashMap<String, Vec<ServerMessage>>,
    /// Whether history saving is enabled
    enabled: bool,
    /// Retention policy
    retention: ChatHistoryRetention,
}

impl HistoryManager {
    /// Create a new history manager for a server connection
    ///
    /// # Arguments
    /// * `fingerprint` - Server certificate fingerprint (hex-encoded SHA-256)
    /// * `user_id` - Your immutable account id on this server. Keys the directory,
    ///   so a username rename never moves it.
    /// * `retention` - Retention policy from settings
    pub fn new(fingerprint: &str, user_id: i64, retention: ChatHistoryRetention) -> Self {
        let base_dir = Self::build_base_dir(fingerprint, user_id);
        let crypto = HistoryCrypto::new(fingerprint);
        let enabled = retention.is_enabled();

        Self {
            base_dir,
            crypto,
            conversations: HashMap::new(),
            enabled,
            retention,
        }
    }

    /// Update the retention policy (called when reconnecting to pick up setting changes)
    pub fn update_retention(&mut self, retention: ChatHistoryRetention) {
        self.retention = retention;
        self.enabled = retention.is_enabled();
        // Clear in-memory conversations when disabling so the "map is empty when
        // disabled" invariant holds unconditionally, not just at creation/load time.
        // This prevents stale pre-disable entries from reappearing if re-enabled.
        if !self.enabled {
            self.conversations.clear();
        }
    }

    /// Build the base directory path for a server+user combination
    ///
    /// This can be used as a key to share managers across connections
    /// to the same server+account.
    pub fn build_base_dir(fingerprint: &str, user_id: i64) -> PathBuf {
        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nexus")
            .join("history");

        let fingerprint_hash = sha256_hex(fingerprint);
        // Key the per-account segment by the immutable user id so a username rename
        // never moves the directory. The fingerprint is folded into the hash so the
        // same id on different servers hashes differently — no single precomputed
        // table works across servers.
        let user_hash = sha256_hex(&format!("{fingerprint}:{user_id}"));

        data_dir.join(fingerprint_hash).join(user_hash)
    }

    /// One-time, best-effort migration of the pre-id history layout. Directories
    /// used to be keyed by `sha256(fold(username))`; they're now keyed by user id
    /// (see `build_base_dir`). If the legacy dir exists and the id-keyed dir does
    /// not, move it. Misses an account renamed while offline across this upgrade
    /// (the prior username is unknown) — that history is orphaned once.
    pub fn migrate_legacy_user_dir(fingerprint: &str, username: &str, user_id: i64) {
        let current = Self::build_base_dir(fingerprint, user_id);
        let Some(parent) = current.parent() else {
            return;
        };
        let legacy = parent.join(sha256_hex(&fold_name(username)));
        if legacy != current && legacy.exists() && !current.exists() {
            // Whole-directory move within one server's tree (same filesystem); the
            // fingerprint-derived crypto key is unchanged, so no re-encryption.
            let _ = fs::rename(&legacy, &current);
        }
    }

    /// Load all conversations from disk
    ///
    /// Returns a map of other_nickname -> messages for restoring user message tabs.
    /// Should be called on connect after authentication.
    pub fn load_all(&mut self) -> Result<HashMap<String, Vec<ServerMessage>>, HistoryError> {
        if !self.enabled {
            return Ok(HashMap::new());
        }

        // Create directory if it doesn't exist
        if !self.base_dir.exists() {
            return Ok(HashMap::new());
        }

        let mut loaded = HashMap::new();
        let now = chrono::Utc::now().timestamp();
        let cutoff = self
            .retention
            .days()
            .map(|d| now.saturating_sub((d as i64).saturating_mul(86400)));

        // List all .enc files in the directory
        let entries = fs::read_dir(&self.base_dir).map_err(HistoryError::Io)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == HISTORY_FILE_EXT) {
                // Try to load and decrypt the file
                match self.load_file(&path) {
                    Ok((other_nickname, mut messages)) => {
                        // Apply per-message retention pruning if needed
                        if let Some(cutoff_ts) = cutoff {
                            let original_len = messages.len();
                            messages.retain(|msg| get_message_timestamp(msg) >= cutoff_ts);

                            // If we pruned messages, rewrite the file
                            if messages.len() != original_len {
                                if messages.is_empty() {
                                    let _ = fs::remove_file(&path);
                                    continue;
                                } else {
                                    // Rewrite with pruned messages
                                    let _ =
                                        self.save_conversation_internal(&other_nickname, &messages);
                                }
                            }
                        }

                        if !messages.is_empty() {
                            loaded.insert(other_nickname, messages);
                        }
                    }
                    Err(_) => {
                        // Silently skip corrupt/unreadable files and continue loading others
                    }
                }
            }
        }

        // `loaded` is keyed by display nickname (for tab restoration); the
        // internal cache keys by the folded nickname so case variants merge.
        self.conversations = loaded
            .iter()
            .map(|(nick, msgs)| (fold_name(nick), msgs.clone()))
            .collect();
        Ok(loaded)
    }

    /// Load a single history file
    fn load_file(&self, path: &Path) -> Result<(String, Vec<ServerMessage>), HistoryError> {
        let encrypted = fs::read(path).map_err(HistoryError::Io)?;
        let decrypted = self
            .crypto
            .decrypt(&encrypted)
            .map_err(HistoryError::Crypto)?;
        let json = String::from_utf8(decrypted).map_err(|_| HistoryError::InvalidFormat)?;

        let file: ConversationFile =
            serde_json::from_str(&json).map_err(|_| HistoryError::InvalidFormat)?;

        Ok((file.other_nickname, file.messages))
    }

    /// Add a message to a conversation and save to disk
    pub fn add_message(
        &mut self,
        other_nickname: &str,
        message: ServerMessage,
    ) -> Result<(), HistoryError> {
        if !self.enabled {
            return Ok(());
        }

        // Add to in-memory cache
        let messages = self
            .conversations
            .entry(fold_name(other_nickname))
            .or_default();

        // Check for duplicates (can happen if same user logged in twice, both receive same message)
        // Scan backwards until we hit a message with a lower timestamp - messages arrive in order,
        // so duplicates will always be recent. This is O(1) in practice.
        let msg_timestamp = get_message_timestamp(&message);
        for existing in messages.iter().rev() {
            let existing_ts = get_message_timestamp(existing);
            if existing_ts < msg_timestamp {
                // Older message - no need to check further
                break;
            }
            // Check if this is a duplicate (same timestamp, from, to, content)
            if let (
                ServerMessage::UserMessage {
                    timestamp: t1,
                    from_nickname: f1,
                    to_nickname: to1,
                    message: m1,
                    ..
                },
                ServerMessage::UserMessage {
                    timestamp: t2,
                    from_nickname: f2,
                    to_nickname: to2,
                    message: m2,
                    ..
                },
            ) = (existing, &message)
                && t1 == t2
                && f1 == f2
                && to1 == to2
                && m1 == m2
            {
                return Ok(());
            }
        }

        messages.push(message);

        // Save to disk
        self.save_conversation(other_nickname)
    }

    /// Save a conversation to disk
    fn save_conversation(&self, other_nickname: &str) -> Result<(), HistoryError> {
        let messages = self
            .conversations
            .get(&fold_name(other_nickname))
            .ok_or(HistoryError::NotFound)?;

        self.save_conversation_internal(other_nickname, messages)
    }

    /// Internal save implementation
    fn save_conversation_internal(
        &self,
        other_nickname: &str,
        messages: &[ServerMessage],
    ) -> Result<(), HistoryError> {
        // Ensure directory exists
        fs::create_dir_all(&self.base_dir).map_err(HistoryError::Io)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ =
                fs::set_permissions(&self.base_dir, fs::Permissions::from_mode(DIR_PERMISSIONS));
        }

        // Filename from the folded nickname hash (case variants share one file);
        // the display nickname is preserved in the file content below.
        let filename = format!(
            "{}.{}",
            sha256_hex(&fold_name(other_nickname)),
            HISTORY_FILE_EXT
        );
        let new_path = self.base_dir.join(&filename);

        // Wrap messages with metadata
        let file = ConversationFile {
            other_nickname: other_nickname.to_string(),
            messages: messages.to_vec(),
        };

        // Serialize and encrypt
        let json = serde_json::to_string(&file).map_err(|_| HistoryError::SerializationFailed)?;
        let encrypted = self
            .crypto
            .encrypt(json.as_bytes())
            .map_err(HistoryError::Crypto)?;

        // On Unix, create empty file and set permissions before writing content
        // This avoids a race condition where the file is briefly world-readable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            // Create empty file
            fs::File::create(&new_path).map_err(HistoryError::Io)?;
            // Set restrictive permissions while file is empty
            let _ = fs::set_permissions(&new_path, fs::Permissions::from_mode(FILE_PERMISSIONS));
        }

        // Write content (file already has correct permissions on Unix)
        fs::write(&new_path, &encrypted).map_err(HistoryError::Io)?;

        Ok(())
    }

    /// Clear history for a specific conversation
    pub fn clear_conversation(&mut self, other_nickname: &str) -> Result<(), HistoryError> {
        // Remove from memory
        self.conversations.remove(&fold_name(other_nickname));

        // Delete file from disk
        let hash = sha256_hex(&fold_name(other_nickname));
        let filename = format!("{}.{}", hash, HISTORY_FILE_EXT);
        let path = self.base_dir.join(filename);

        if path.exists() {
            fs::remove_file(&path).map_err(HistoryError::Io)?;
        }

        Ok(())
    }

    /// Re-key a conversation when the other party renames `old` -> `new`: move the
    /// in-memory entry and the on-disk file, merging into any existing `new`
    /// conversation (you may have prior history with a different account that has
    /// since taken the name `new`). The stored `other_nickname` is the load-time tab
    /// key, so we rewrite under the new filename and delete the old. No-op when
    /// nothing is keyed under `old` — including a self-rename, since you have no
    /// conversation with yourself.
    pub fn rename_conversation(&mut self, old: &str, new: &str) -> Result<(), HistoryError> {
        // Disabled retention leaves history files untouched (preserved until
        // re-enabled), so a rename is a no-op — and the in-memory map is empty when
        // disabled anyway.
        if !self.enabled {
            return Ok(());
        }

        let old_key = fold_name(old);
        let new_key = fold_name(new);
        if old_key == new_key {
            // Same folded identity. A display-case-only change (alice -> Alice) must
            // still rewrite the stored display name in place (same filename), or a
            // reload restores the old case. An exact match is a true no-op.
            if old != new
                && let Some(messages) = self.conversations.get(&new_key).cloned()
            {
                self.save_conversation_internal(new, &messages)?;
            }
            return Ok(());
        }

        let old_path = self
            .base_dir
            .join(format!("{}.{}", sha256_hex(&old_key), HISTORY_FILE_EXT));

        // Messages currently under `old` (from memory, with a disk fallback).
        let old_messages = match self.conversations.remove(&old_key) {
            Some(messages) => messages,
            None if old_path.exists() => match self.load_file(&old_path) {
                Ok((_, messages)) => messages,
                Err(_) => return Ok(()),
            },
            None => return Ok(()),
        };

        // Merge into any existing `new` conversation (memory or disk) so a name
        // collision with prior history doesn't drop messages.
        let new_path = self
            .base_dir
            .join(format!("{}.{}", sha256_hex(&new_key), HISTORY_FILE_EXT));
        let existing_new = match self.conversations.remove(&new_key) {
            Some(messages) => Some(messages),
            None if new_path.exists() => self.load_file(&new_path).ok().map(|(_, m)| m),
            None => None,
        };
        let merged = match existing_new {
            Some(existing) => merge_messages(existing, old_messages),
            None => old_messages,
        };

        // Persist under the new name, drop the old file, update memory.
        self.save_conversation_internal(new, &merged)?;
        if old_path.exists() {
            let _ = fs::remove_file(&old_path);
        }
        self.conversations.insert(new_key, merged);
        Ok(())
    }
}

/// Rotate history files from old fingerprint to new fingerprint
///
/// This re-encrypts all history files for a given server when the certificate
/// changes and the user accepts the new fingerprint.
///
/// # Arguments
/// * `old_fingerprint` - The previous certificate fingerprint
/// * `new_fingerprint` - The new certificate fingerprint
///
/// # Returns
/// Number of files successfully rotated
pub fn rotate_fingerprint(old_fingerprint: &str, new_fingerprint: &str) -> usize {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nexus")
        .join("history");

    let old_fp_hash = sha256_hex(old_fingerprint);
    let new_fp_hash = sha256_hex(new_fingerprint);

    let old_dir = data_dir.join(&old_fp_hash);
    let new_dir = data_dir.join(&new_fp_hash);

    // If old directory doesn't exist, nothing to rotate
    if !old_dir.exists() {
        return 0;
    }

    // If fingerprints are the same, nothing to do
    if old_fp_hash == new_fp_hash {
        return 0;
    }

    let old_crypto = HistoryCrypto::new(old_fingerprint);
    let new_crypto = HistoryCrypto::new(new_fingerprint);

    let mut rotated_count = 0;

    // Walk through all user directories in the old fingerprint directory
    let Ok(user_dirs) = fs::read_dir(&old_dir) else {
        return 0;
    };

    for user_entry in user_dirs.flatten() {
        let user_path = user_entry.path();
        if !user_path.is_dir() {
            continue;
        }

        // Preserve the per-account directory hash.
        let Some(user_dir_hash) = user_path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };

        // Create corresponding directory in new fingerprint location
        let new_user_dir = new_dir.join(user_dir_hash);
        if fs::create_dir_all(&new_user_dir).is_err() {
            continue;
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&new_dir, fs::Permissions::from_mode(DIR_PERMISSIONS));
            let _ = fs::set_permissions(&new_user_dir, fs::Permissions::from_mode(DIR_PERMISSIONS));
        }

        // Process all .enc files in this user directory
        let Ok(files) = fs::read_dir(&user_path) else {
            continue;
        };

        for file_entry in files.flatten() {
            let file_path = file_entry.path();
            if file_path
                .extension()
                .is_some_and(|ext| ext == HISTORY_FILE_EXT)
            {
                // Read and decrypt with old key
                let Ok(encrypted) = fs::read(&file_path) else {
                    continue;
                };
                let Ok(decrypted) = old_crypto.decrypt(&encrypted) else {
                    continue;
                };

                // Re-encrypt with new key
                let Ok(new_encrypted) = new_crypto.encrypt(&decrypted) else {
                    continue;
                };

                // Write to new location with same filename
                let Some(filename) = file_path.file_name() else {
                    continue;
                };
                let new_file_path = new_user_dir.join(filename);

                // On Unix, create empty file and set permissions before writing content
                // This avoids a race condition where the file is briefly world-readable
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    // Create empty file
                    if fs::File::create(&new_file_path).is_err() {
                        continue;
                    }
                    // Set restrictive permissions while file is empty
                    let _ = fs::set_permissions(
                        &new_file_path,
                        fs::Permissions::from_mode(FILE_PERMISSIONS),
                    );
                }

                // Write content (file already has correct permissions on Unix)
                if fs::write(&new_file_path, &new_encrypted).is_ok() {
                    // Delete old file
                    let _ = fs::remove_file(&file_path);
                    rotated_count += 1;
                }
            }
        }

        // Try to remove old user directory if empty
        let _ = fs::remove_dir(&user_path);
    }

    // Try to remove old fingerprint directory if empty
    let _ = fs::remove_dir(&old_dir);

    rotated_count
}

/// Errors that can occur during history operations
#[derive(Debug)]
pub enum HistoryError {
    /// I/O error reading/writing files
    Io(io::Error),
    /// Cryptographic operation failed
    Crypto(CryptoError),
    /// File format is invalid (not valid JSON, wrong structure)
    InvalidFormat,
    /// Serialization failed
    SerializationFailed,
    /// Conversation not found
    NotFound,
}

impl std::fmt::Display for HistoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HistoryError::Io(e) => write!(f, "I/O error: {}", e),
            HistoryError::Crypto(e) => write!(f, "crypto error: {}", e),
            HistoryError::InvalidFormat => write!(f, "invalid file format"),
            HistoryError::SerializationFailed => write!(f, "serialization failed"),
            HistoryError::NotFound => write!(f, "conversation not found"),
        }
    }
}

impl std::error::Error for HistoryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            HistoryError::Io(e) => Some(e),
            HistoryError::Crypto(e) => Some(e),
            _ => None,
        }
    }
}

/// Compute SHA-256 hash and return as hex string
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// Extract timestamp from a UserMessage
fn get_message_timestamp(msg: &ServerMessage) -> i64 {
    match msg {
        ServerMessage::UserMessage { timestamp, .. } => *timestamp,
        _ => 0,
    }
}

/// Combine two message lists into one ordered by timestamp, dropping exact-looking
/// repeats (same timestamp, sender, recipient, content). Used when a rename
/// collides with an existing conversation; this intentionally collapses repeated
/// identical same-second lines rather than preserving spam verbatim.
fn merge_messages(mut a: Vec<ServerMessage>, b: Vec<ServerMessage>) -> Vec<ServerMessage> {
    a.extend(b);
    a.sort_by_key(get_message_timestamp);

    // dedup_by only removes adjacent duplicates; with same-timestamp messages the
    // sort does not guarantee equal entries are adjacent. Use a set-based retain so
    // every duplicate is removed regardless of position.
    let mut seen = HashSet::new();
    a.retain(|msg| {
        if let ServerMessage::UserMessage {
            timestamp,
            from_nickname,
            to_nickname,
            message,
            ..
        } = msg
        {
            seen.insert((
                *timestamp,
                from_nickname.clone(),
                to_nickname.clone(),
                message.clone(),
            ))
        } else {
            true
        }
    });
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_common::protocol::ChatAction;

    fn create_test_message(from: &str, to: &str, text: &str, timestamp: i64) -> ServerMessage {
        ServerMessage::UserMessage {
            from_nickname: from.to_string(),
            from_admin: false,
            from_shared: false,
            to_nickname: to.to_string(),
            message: text.to_string(),
            action: ChatAction::Normal,
            timestamp,
        }
    }

    #[test]
    fn test_merge_messages_orders_and_dedups() {
        let a = vec![
            create_test_message("alice", "me", "first", 10),
            create_test_message("me", "alice", "third", 30),
        ];
        let b = vec![
            create_test_message("me", "alice", "third", 30), // duplicate of a's last
            create_test_message("alice", "me", "second", 20),
        ];
        let merged = merge_messages(a, b);
        let timestamps: Vec<i64> = merged.iter().map(get_message_timestamp).collect();
        assert_eq!(
            timestamps,
            vec![10, 20, 30],
            "merged conversation is time-ordered with the duplicate dropped"
        );
    }

    #[test]
    fn test_rename_conversation_moves_to_new_key() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut manager = HistoryManager::new("test_fp", 1, ChatHistoryRetention::Forever);
        manager.base_dir = tmp.path().to_path_buf();

        manager
            .add_message("alice", create_test_message("alice", "me", "hi", 10))
            .unwrap();

        manager.rename_conversation("alice", "bob").unwrap();

        assert!(!manager.conversations.contains_key(&fold_name("alice")));
        assert_eq!(
            manager.conversations.get(&fold_name("bob")).unwrap().len(),
            1
        );
        let old_file = tmp.path().join(format!(
            "{}.{}",
            sha256_hex(&fold_name("alice")),
            HISTORY_FILE_EXT
        ));
        let new_file = tmp.path().join(format!(
            "{}.{}",
            sha256_hex(&fold_name("bob")),
            HISTORY_FILE_EXT
        ));
        assert!(!old_file.exists(), "old conversation file removed");
        assert!(new_file.exists(), "new conversation file written");
    }

    #[test]
    fn test_rename_conversation_merges_on_collision() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut manager = HistoryManager::new("test_fp", 1, ChatHistoryRetention::Forever);
        manager.base_dir = tmp.path().to_path_buf();

        // Prior history with an earlier account that was named "bob".
        manager
            .add_message("bob", create_test_message("bob", "me", "old", 5))
            .unwrap();
        // Current history with "alice", who is about to be renamed to "bob".
        manager
            .add_message("alice", create_test_message("alice", "me", "new", 20))
            .unwrap();

        manager.rename_conversation("alice", "bob").unwrap();

        assert!(!manager.conversations.contains_key(&fold_name("alice")));
        let bob = manager.conversations.get(&fold_name("bob")).unwrap();
        let timestamps: Vec<i64> = bob.iter().map(get_message_timestamp).collect();
        assert_eq!(
            timestamps,
            vec![5, 20],
            "collision merges both conversations rather than overwriting"
        );
    }

    #[test]
    fn test_rename_conversation_case_only_rewrites_stored_display_name() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut manager = HistoryManager::new("test_fp", 1, ChatHistoryRetention::Forever);
        manager.base_dir = tmp.path().to_path_buf();

        manager
            .add_message("alice", create_test_message("alice", "me", "hi", 10))
            .unwrap();

        manager.rename_conversation("alice", "Alice").unwrap();

        // Folded key (and filename) unchanged, but the stored display name follows
        // the new case, so a reload restores "Alice" rather than "alice".
        let file = tmp.path().join(format!(
            "{}.{}",
            sha256_hex(&fold_name("alice")),
            HISTORY_FILE_EXT
        ));
        let (stored_nick, messages) = manager.load_file(&file).expect("read conversation file");
        assert_eq!(
            stored_nick, "Alice",
            "case-only rename must rewrite the stored display name in place"
        );
        assert_eq!(messages.len(), 1, "messages preserved");
    }

    #[test]
    fn test_sha256_hex() {
        let hash = sha256_hex("test");
        assert_eq!(hash.len(), 64); // SHA-256 produces 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_history_manager_new() {
        // Just verify construction doesn't panic
        let _manager = HistoryManager::new("test_fingerprint", 1, ChatHistoryRetention::Forever);
    }

    #[test]
    fn test_get_message_timestamp() {
        let msg = create_test_message("alice", "bob", "hello", 1234567890);
        assert_eq!(get_message_timestamp(&msg), 1234567890);
    }

    #[test]
    fn test_rotate_fingerprint_no_old_dir() {
        // Rotating when old directory doesn't exist should return 0
        let rotated = rotate_fingerprint("nonexistent_old_fp", "new_fp");
        assert_eq!(rotated, 0);
    }

    #[test]
    fn test_rotate_fingerprint_same_fingerprint() {
        // Rotating to same fingerprint should return 0
        let rotated = rotate_fingerprint("same_fp", "same_fp");
        assert_eq!(rotated, 0);
    }

    #[test]
    fn test_add_message_deduplication() {
        let mut manager = HistoryManager::new(
            "test_fingerprint",
            1,
            ChatHistoryRetention::Disabled, // Disabled so we don't write to disk
        );

        // Manually enable the in-memory cache for testing
        manager.enabled = true;

        let msg1 = create_test_message("alice", "bob", "hello", 1234567890);
        let msg2 = create_test_message("alice", "bob", "hello", 1234567890); // Same message
        let msg3 = create_test_message("alice", "bob", "different", 1234567890); // Different content
        let msg4 = create_test_message("alice", "bob", "hello", 1234567891); // Different timestamp

        // Add first message
        let _ = manager.add_message("bob", msg1);
        assert_eq!(manager.conversations.get("bob").map(|v| v.len()), Some(1));

        // Add duplicate - should be ignored
        let _ = manager.add_message("bob", msg2);
        assert_eq!(manager.conversations.get("bob").map(|v| v.len()), Some(1));

        // Add message with different content - should be added
        let _ = manager.add_message("bob", msg3);
        assert_eq!(manager.conversations.get("bob").map(|v| v.len()), Some(2));

        // Add message with different timestamp - should be added
        let _ = manager.add_message("bob", msg4);
        assert_eq!(manager.conversations.get("bob").map(|v| v.len()), Some(3));
    }

    #[test]
    fn test_build_base_dir_keys_by_user_id_and_fingerprint() {
        // Deterministic for a given (fingerprint, id).
        assert_eq!(
            HistoryManager::build_base_dir("fp", 7),
            HistoryManager::build_base_dir("fp", 7),
        );
        // Different accounts on one server get different directories.
        assert_ne!(
            HistoryManager::build_base_dir("fp", 7),
            HistoryManager::build_base_dir("fp", 8),
        );
        // The same id on different servers hashes differently (fingerprint folded
        // into the user segment defeats a single cross-server precomputed table).
        assert_ne!(
            HistoryManager::build_base_dir("fp_a", 7),
            HistoryManager::build_base_dir("fp_b", 7),
        );
    }

    #[test]
    fn test_add_message_folds_partner_nickname() {
        let mut manager =
            HistoryManager::new("test_fingerprint", 1, ChatHistoryRetention::Disabled);
        manager.enabled = true;

        // Two case variants of the same partner merge into one (folded)
        // conversation rather than splitting — so they can't clobber the same
        // on-disk file. The internal cache keys by the folded nickname.
        let _ = manager.add_message("Bob", create_test_message("Bob", "me", "hi", 1));
        let _ = manager.add_message("bob", create_test_message("bob", "me", "yo", 2));

        assert_eq!(manager.conversations.len(), 1, "case variants merge");
        assert_eq!(
            manager.conversations.get("bob").map(|v| v.len()),
            Some(2),
            "both messages land in the one folded conversation"
        );
    }
}
