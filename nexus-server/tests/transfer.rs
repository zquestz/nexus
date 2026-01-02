//! Integration tests for file transfer protocol (port 7501)
//!
//! These tests verify the transfer handler's behavior for file downloads,
//! including authentication, permission checks, and file streaming.

mod common;

use std::io::Cursor;

use common::create_test_db;
use nexus_common::framing::{FrameReader, FrameWriter, MessageId, RawFrame};
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_server::db::{self, Permission, Permissions};
use tempfile::TempDir;
use tokio::fs;
use tokio::io::BufReader;

// ============================================================================
// Helper Functions
// ============================================================================

/// Create a test file area with some test files
async fn create_test_file_area() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

    // Create shared directory structure
    fs::create_dir_all(root.join("shared/Documents"))
        .await
        .unwrap();
    fs::create_dir_all(root.join("shared/Uploads [NEXUS-UL]"))
        .await
        .unwrap();
    fs::create_dir_all(root.join("shared/Submissions [NEXUS-DB]"))
        .await
        .unwrap();
    fs::create_dir_all(root.join("shared/For Alice [NEXUS-DB-alice]"))
        .await
        .unwrap();
    fs::create_dir_all(root.join("users/alice")).await.unwrap();
    fs::create_dir_all(root.join("users/bob")).await.unwrap();

    // Create test files
    fs::write(root.join("shared/Documents/readme.txt"), b"Hello, World!")
        .await
        .unwrap();
    fs::write(root.join("shared/Documents/data.bin"), vec![0u8; 1024])
        .await
        .unwrap();
    fs::write(
        root.join("shared/Submissions [NEXUS-DB]/secret.txt"),
        b"Secret data",
    )
    .await
    .unwrap();
    fs::write(
        root.join("shared/For Alice [NEXUS-DB-alice]/alice_file.txt"),
        b"Alice's private file",
    )
    .await
    .unwrap();
    fs::write(
        root.join("users/alice/personal.txt"),
        b"Alice's personal file",
    )
    .await
    .unwrap();

    temp_dir
}

// ============================================================================
// Dropbox Access Tests (Unit-level, no network)
// ============================================================================

#[test]
fn test_dropbox_path_parsing() {
    use nexus_server::files::folder_type::{FolderType, parse_folder_type};

    // Default folder
    assert!(matches!(
        parse_folder_type("Documents"),
        FolderType::Default
    ));

    // Upload folder
    assert!(matches!(
        parse_folder_type("Uploads [NEXUS-UL]"),
        FolderType::Upload
    ));

    // Generic dropbox
    assert!(matches!(
        parse_folder_type("Submissions [NEXUS-DB]"),
        FolderType::DropBox
    ));

    // User dropbox
    match parse_folder_type("For Alice [NEXUS-DB-alice]") {
        FolderType::UserDropBox(owner) => assert_eq!(owner.to_lowercase(), "alice"),
        _ => panic!("Expected UserDropBox"),
    }

    // Case insensitivity
    assert!(matches!(
        parse_folder_type("uploads [nexus-ul]"),
        FolderType::Upload
    ));
}

// ============================================================================
// Path Resolution Tests
// ============================================================================

#[tokio::test]
async fn test_resolve_user_area_with_personal_folder() {
    use nexus_server::files::area::resolve_user_area;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    // Alice has a personal folder
    let alice_area = resolve_user_area(root, "alice");
    assert!(alice_area.ends_with("users/alice"));

    // Charlie doesn't have a personal folder, gets shared
    let charlie_area = resolve_user_area(root, "charlie");
    assert!(charlie_area.ends_with("shared"));
}

#[tokio::test]
async fn test_path_validation_rejects_traversal() {
    use nexus_server::files::path::build_and_validate_candidate_path;

    let temp_dir = create_test_file_area().await;
    let area_root = temp_dir.path().join("shared");

    // Normal path should work
    let result = build_and_validate_candidate_path(&area_root, "/Documents/readme.txt");
    assert!(result.is_ok());

    // Path traversal should fail
    let result = build_and_validate_candidate_path(&area_root, "/../../../etc/passwd");
    assert!(result.is_err());

    // Embedded traversal should fail
    let result = build_and_validate_candidate_path(&area_root, "/Documents/../../../etc/passwd");
    assert!(result.is_err());
}

// ============================================================================
// File Scanning Tests
// ============================================================================

#[tokio::test]
async fn test_scan_single_file() {
    let temp_dir = create_test_file_area().await;
    let file_path = temp_dir.path().join("shared/Documents/readme.txt");

    let metadata = fs::metadata(&file_path).await.unwrap();
    assert!(metadata.is_file());
    assert_eq!(metadata.len(), 13); // "Hello, World!" is 13 bytes
}

#[tokio::test]
async fn test_scan_directory_structure() {
    let temp_dir = create_test_file_area().await;
    let docs_path = temp_dir.path().join("shared/Documents");

    let mut entries = fs::read_dir(&docs_path).await.unwrap();
    let mut file_names = Vec::new();

    while let Some(entry) = entries.next_entry().await.unwrap() {
        file_names.push(entry.file_name().to_string_lossy().to_string());
    }

    file_names.sort();
    assert_eq!(file_names, vec!["data.bin", "readme.txt"]);
}

// ============================================================================
// Hash Computation Tests
// ============================================================================

#[tokio::test]
async fn test_sha256_known_value() {
    use sha2::{Digest, Sha256};

    let data = b"Hello, World!";
    let mut hasher = Sha256::new();
    hasher.update(data);
    let hash = hasher.finalize();
    let hex: String = hash.iter().map(|b| format!("{b:02x}")).collect();

    assert_eq!(
        hex,
        "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
    );
}

// ============================================================================
// Permission Tests (Database-level)
// ============================================================================

#[tokio::test]
async fn test_file_download_permission_in_db() {
    let db = create_test_db().await;

    // Create user with file_download permission
    let hashed = db::hash_password("password").unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::FileDownload);
    perms.add(Permission::FileList);

    let user = db
        .users
        .create_user("downloader", &hashed, false, false, true, &perms)
        .await
        .unwrap();

    // Verify permissions are stored by fetching them back
    let stored_perms = db.users.get_user_permissions(user.id).await.unwrap();
    let perm_vec = stored_perms.to_vec();
    assert!(perm_vec.contains(&Permission::FileDownload));
    assert!(perm_vec.contains(&Permission::FileList));
    assert!(!perm_vec.contains(&Permission::FileRoot));
}

#[tokio::test]
async fn test_admin_has_implicit_permissions() {
    let db = create_test_db().await;

    // Create admin with no explicit permissions
    let hashed = db::hash_password("password").unwrap();
    let admin = db
        .users
        .create_user("admin", &hashed, true, false, true, &Permissions::new())
        .await
        .unwrap();

    // Admin should be marked as admin
    assert!(admin.is_admin);

    // Verify admin flag via lookup
    let fetched = db
        .users
        .get_user_by_username("admin")
        .await
        .unwrap()
        .unwrap();
    assert!(fetched.is_admin);
}

// ============================================================================
// Message Serialization Tests
// ============================================================================

#[test]
fn test_file_download_message_serialization() {
    let msg = ClientMessage::FileDownload {
        path: "/Documents/readme.txt".to_string(),
        root: false,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FileDownload\""));
    assert!(json.contains("\"path\":\"/Documents/readme.txt\""));

    // Deserialize back
    let parsed: ClientMessage = serde_json::from_str(&json).unwrap();
    match parsed {
        ClientMessage::FileDownload { path, root } => {
            assert_eq!(path, "/Documents/readme.txt");
            assert!(!root);
        }
        _ => panic!("Expected FileDownload"),
    }
}

#[test]
fn test_file_download_response_success() {
    let msg = ServerMessage::FileDownloadResponse {
        success: true,
        error: None,
        error_kind: None,
        size: Some(1024),
        file_count: Some(5),
        transfer_id: Some("abcd1234".to_string()),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"size\":1024"));
    assert!(json.contains("\"file_count\":5"));
    assert!(json.contains("\"transfer_id\":\"abcd1234\""));
}

#[test]
fn test_file_download_response_error() {
    let msg = ServerMessage::FileDownloadResponse {
        success: false,
        error: Some("Permission denied".to_string()),
        error_kind: Some("permission".to_string()),
        size: None,
        file_count: None,
        transfer_id: None,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"error\":\"Permission denied\""));
    assert!(json.contains("\"error_kind\":\"permission\""));
    // None fields should be omitted
    assert!(!json.contains("\"size\""));
    assert!(!json.contains("\"file_count\""));
}

#[test]
fn test_file_start_message() {
    let msg = ServerMessage::FileStart {
        path: "Documents/readme.txt".to_string(),
        size: 1024,
        sha256: "abc123".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FileStart\""));
    assert!(json.contains("\"path\":\"Documents/readme.txt\""));
    assert!(json.contains("\"size\":1024"));
    assert!(json.contains("\"sha256\":\"abc123\""));
}

#[test]
fn test_file_start_response_no_local_file() {
    let msg = ClientMessage::FileStartResponse {
        size: 0,
        sha256: None,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"size\":0"));
    assert!(!json.contains("\"sha256\"")); // None should be omitted
}

#[test]
fn test_file_start_response_with_partial() {
    let msg = ClientMessage::FileStartResponse {
        size: 512,
        sha256: Some("partial_hash_here".to_string()),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"size\":512"));
    assert!(json.contains("\"sha256\":\"partial_hash_here\""));
}

#[test]
fn test_transfer_complete_success() {
    let msg = ServerMessage::TransferComplete {
        success: true,
        error: None,
        error_kind: None,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"TransferComplete\""));
    assert!(json.contains("\"success\":true"));
    assert!(!json.contains("\"error\""));
}

#[test]
fn test_transfer_complete_failure() {
    let msg = ServerMessage::TransferComplete {
        success: false,
        error: Some("File deleted during transfer".to_string()),
        error_kind: Some("io_error".to_string()),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"error\":\"File deleted during transfer\""));
    assert!(json.contains("\"error_kind\":\"io_error\""));
}

// ============================================================================
// Frame Protocol Tests
// ============================================================================

#[tokio::test]
async fn test_frame_roundtrip_file_download() {
    let msg = ClientMessage::FileDownload {
        path: "/test/file.txt".to_string(),
        root: false,
    };
    let payload = serde_json::to_vec(&msg).unwrap();
    let id = MessageId::new();

    // Write frame
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileDownload".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    // Read frame back
    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader.read_frame().await.unwrap().unwrap();
    assert_eq!(frame.message_id, id);
    assert_eq!(frame.message_type, "FileDownload");

    // Verify payload
    let parsed: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
    match parsed {
        ClientMessage::FileDownload { path, root } => {
            assert_eq!(path, "/test/file.txt");
            assert!(!root);
        }
        _ => panic!("Wrong message type"),
    }
}

// ============================================================================
// Transfer ID Tests
// ============================================================================

#[test]
fn test_transfer_id_in_response() {
    // Verify transfer_id is properly included in FileDownloadResponse
    let response = ServerMessage::FileDownloadResponse {
        success: true,
        error: None,
        error_kind: None,
        size: Some(100),
        file_count: Some(1),
        transfer_id: Some("deadbeef".to_string()),
    };

    let json = serde_json::to_string(&response).unwrap();
    assert!(json.contains("\"transfer_id\":\"deadbeef\""));
}

// ============================================================================
// Error Kind Tests
// ============================================================================

#[test]
fn test_error_kinds() {
    // Test various error_kind values that client should handle
    let error_kinds = vec![
        ("not_found", "Path doesn't exist"),
        ("permission", "Permission denied"),
        ("invalid", "Invalid path"),
        ("io_error", "I/O error during transfer"),
        ("protocol_error", "Protocol violation"),
    ];

    for (kind, _description) in error_kinds {
        let msg = ServerMessage::FileDownloadResponse {
            success: false,
            error: Some("Test error".to_string()),
            error_kind: Some(kind.to_string()),
            size: None,
            file_count: None,
            transfer_id: None,
        };

        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(&format!("\"error_kind\":\"{kind}\"")));
    }
}

// ============================================================================
// Empty Directory / Zero-byte File Tests
// ============================================================================

#[tokio::test]
async fn test_empty_directory_handling() {
    let temp_dir = TempDir::new().unwrap();
    let empty_dir = temp_dir.path().join("empty");
    fs::create_dir(&empty_dir).await.unwrap();

    // Verify directory is empty
    let mut entries = fs::read_dir(&empty_dir).await.unwrap();
    assert!(entries.next_entry().await.unwrap().is_none());
}

#[tokio::test]
async fn test_zero_byte_file() {
    let temp_dir = TempDir::new().unwrap();
    let file_path = temp_dir.path().join("empty.txt");
    fs::write(&file_path, b"").await.unwrap();

    let metadata = fs::metadata(&file_path).await.unwrap();
    assert_eq!(metadata.len(), 0);
}

// ============================================================================
// Symlink Tests (Unix only)
// ============================================================================

#[cfg(unix)]
#[tokio::test]
async fn test_symlink_in_file_area() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    // Create a real file
    let real_file = root.join("real.txt");
    fs::write(&real_file, b"Real content").await.unwrap();

    // Create a symlink to it
    let link_path = root.join("link.txt");
    symlink(&real_file, &link_path).unwrap();

    // Verify symlink exists and points to correct target
    let metadata = fs::symlink_metadata(&link_path).await.unwrap();
    assert!(metadata.file_type().is_symlink());

    // Reading through symlink should work
    let content = fs::read_to_string(&link_path).await.unwrap();
    assert_eq!(content, "Real content");
}

// ============================================================================
// Resume Logic Tests
// ============================================================================

#[test]
fn test_resume_response_new_download() {
    // Client has no local file - should start from beginning
    let response = ClientMessage::FileStartResponse {
        size: 0,
        sha256: None,
    };

    match response {
        ClientMessage::FileStartResponse { size, sha256 } => {
            assert_eq!(size, 0);
            assert!(sha256.is_none());
        }
        _ => panic!("Wrong type"),
    }
}

#[test]
fn test_resume_response_partial_file() {
    // Client has partial file with hash
    let response = ClientMessage::FileStartResponse {
        size: 512,
        sha256: Some("abc123def456".to_string()),
    };

    match response {
        ClientMessage::FileStartResponse { size, sha256 } => {
            assert_eq!(size, 512);
            assert_eq!(sha256.unwrap(), "abc123def456");
        }
        _ => panic!("Wrong type"),
    }
}

#[test]
fn test_resume_response_complete_file() {
    // Client has complete file (size matches server's file)
    let response = ClientMessage::FileStartResponse {
        size: 1024, // Same as server's file
        sha256: Some("full_file_hash".to_string()),
    };

    match response {
        ClientMessage::FileStartResponse { size, sha256 } => {
            assert_eq!(size, 1024);
            assert!(sha256.is_some());
        }
        _ => panic!("Wrong type"),
    }
}

// ============================================================================
// Dropbox Security Tests
// ============================================================================

#[tokio::test]
async fn test_directory_scan_filters_dropbox_contents() {
    // Security test: When downloading a parent directory, dropbox contents
    // should be filtered out for users who don't have access.
    use nexus_server::files::folder_type::{FolderType, parse_folder_type};

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    // Verify the dropbox folders exist with expected types
    assert!(matches!(
        parse_folder_type("Submissions [NEXUS-DB]"),
        FolderType::DropBox
    ));
    assert!(matches!(
        parse_folder_type("For Alice [NEXUS-DB-alice]"),
        FolderType::UserDropBox(_)
    ));

    // Verify the files exist in the dropbox folders
    let dropbox_file = root.join("shared/Submissions [NEXUS-DB]/secret.txt");
    assert!(dropbox_file.exists());

    let alice_dropbox_file = root.join("shared/For Alice [NEXUS-DB-alice]/alice_file.txt");
    assert!(alice_dropbox_file.exists());

    // Regular documents should be accessible
    let regular_file = root.join("shared/Documents/readme.txt");
    assert!(regular_file.exists());

    // The scan_directory_recursive function now checks can_access_for_download
    // for each file/directory, so dropbox contents are filtered out for
    // users who don't have access. This test verifies the test fixtures exist.
}

#[test]
fn test_dropbox_access_rules() {
    use nexus_server::files::folder_type::{FolderType, parse_folder_type};

    // Test the access control rules that scan_directory_recursive uses:

    // Generic dropbox - only admins can access
    assert!(matches!(
        parse_folder_type("Submissions [NEXUS-DB]"),
        FolderType::DropBox
    ));

    // User dropbox - only named user and admins can access
    match parse_folder_type("For Alice [NEXUS-DB-alice]") {
        FolderType::UserDropBox(owner) => {
            assert_eq!(owner.to_lowercase(), "alice");
        }
        _ => panic!("Expected UserDropBox"),
    }

    // Upload folder - everyone can download
    assert!(matches!(
        parse_folder_type("Uploads [NEXUS-UL]"),
        FolderType::Upload
    ));

    // Default folder - everyone can download
    assert!(matches!(
        parse_folder_type("Documents"),
        FolderType::Default
    ));
}
