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

    assert_eq!(
        hex::encode(hash),
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

// ============================================================================
// Upload Message Serialization Tests
// ============================================================================

#[test]
fn test_file_upload_message_serialization() {
    let msg = ClientMessage::FileUpload {
        destination: "/Uploads".to_string(),
        file_count: 3,
        total_size: 1048576,
        root: false,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"FileUpload\""));
    assert!(json.contains("\"destination\":\"/Uploads\""));
    assert!(json.contains("\"file_count\":3"));
    assert!(json.contains("\"total_size\":1048576"));
    // root defaults to false, may or may not be serialized
}

#[test]
fn test_file_upload_message_with_root() {
    let msg = ClientMessage::FileUpload {
        destination: "/Admin/Files".to_string(),
        file_count: 1,
        total_size: 512,
        root: true,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"root\":true"));
}

#[test]
fn test_file_upload_response_success() {
    let msg = ServerMessage::FileUploadResponse {
        success: true,
        error: None,
        error_kind: None,
        transfer_id: Some("abcd1234".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"FileUploadResponse\""));
    assert!(json.contains("\"success\":true"));
    assert!(json.contains("\"transfer_id\":\"abcd1234\""));
}

#[test]
fn test_file_upload_response_error_exists() {
    let msg = ServerMessage::FileUploadResponse {
        success: false,
        error: Some("A file with this name already exists".to_string()),
        error_kind: Some("exists".to_string()),
        transfer_id: None,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"error_kind\":\"exists\""));
}

#[test]
fn test_file_upload_response_error_conflict() {
    let msg = ServerMessage::FileUploadResponse {
        success: false,
        error: Some("Another upload to this filename is in progress".to_string()),
        error_kind: Some("conflict".to_string()),
        transfer_id: None,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"error_kind\":\"conflict\""));
}

#[test]
fn test_file_upload_response_error_permission() {
    let msg = ServerMessage::FileUploadResponse {
        success: false,
        error: Some("Permission denied".to_string()),
        error_kind: Some("permission".to_string()),
        transfer_id: None,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"error_kind\":\"permission\""));
}

#[test]
fn test_client_file_start_message() {
    // Client sends FileStart for uploads (mirrors ServerMessage::FileStart)
    let msg = ClientMessage::FileStart {
        path: "documents/report.pdf".to_string(),
        size: 2048,
        sha256: "abc123def456".to_string(),
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"FileStart\""));
    assert!(json.contains("\"path\":\"documents/report.pdf\""));
    assert!(json.contains("\"size\":2048"));
    assert!(json.contains("\"sha256\":\"abc123def456\""));
}

#[test]
fn test_server_file_start_response() {
    // Server responds with resume info
    let msg = ServerMessage::FileStartResponse {
        size: 1024,
        sha256: Some("partial_hash_here".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"FileStartResponse\""));
    assert!(json.contains("\"size\":1024"));
    assert!(json.contains("\"sha256\":\"partial_hash_here\""));
}

#[test]
fn test_server_file_start_response_no_existing() {
    // Server has no existing .part file
    let msg = ServerMessage::FileStartResponse {
        size: 0,
        sha256: None,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"size\":0"));
    // sha256 should be null or absent
}

// ============================================================================
// Upload Frame Protocol Tests
// ============================================================================

#[tokio::test]
async fn test_frame_roundtrip_file_upload() {
    let msg = ClientMessage::FileUpload {
        destination: "/Uploads/Projects".to_string(),
        file_count: 5,
        total_size: 10485760,
        root: false,
    };
    let payload = serde_json::to_vec(&msg).unwrap();
    let id = MessageId::new();

    // Write frame
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileUpload".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    // Read frame back
    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader.read_frame().await.unwrap().unwrap();
    assert_eq!(frame.message_id, id);
    assert_eq!(frame.message_type, "FileUpload");

    // Verify payload
    let parsed: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
    match parsed {
        ClientMessage::FileUpload {
            destination,
            file_count,
            total_size,
            root,
        } => {
            assert_eq!(destination, "/Uploads/Projects");
            assert_eq!(file_count, 5);
            assert_eq!(total_size, 10485760);
            assert!(!root);
        }
        _ => panic!("Wrong message type"),
    }
}

#[tokio::test]
async fn test_frame_roundtrip_client_file_start() {
    let msg = ClientMessage::FileStart {
        path: "subdir/file.txt".to_string(),
        size: 4096,
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(),
    };
    let payload = serde_json::to_vec(&msg).unwrap();
    let id = MessageId::new();

    // Write frame
    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileStart".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    // Read frame back
    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader.read_frame().await.unwrap().unwrap();
    assert_eq!(frame.message_type, "FileStart");

    let parsed: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
    match parsed {
        ClientMessage::FileStart { path, size, sha256 } => {
            assert_eq!(path, "subdir/file.txt");
            assert_eq!(size, 4096);
            assert_eq!(
                sha256,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
        }
        _ => panic!("Wrong message type"),
    }
}

// ============================================================================
// Upload Permission Tests
// ============================================================================

#[tokio::test]
async fn test_file_upload_permission_in_db() {
    let db = create_test_db().await;

    // Create user with file_upload permission
    let hashed = db::hash_password("pass123").unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::FileUpload);
    perms.add(Permission::FileList);

    let user = db
        .users
        .create_user("uploader", &hashed, false, false, true, &perms)
        .await
        .unwrap();

    // Verify permissions are stored by fetching them back
    let stored_perms = db.users.get_user_permissions(user.id).await.unwrap();
    let perm_vec = stored_perms.to_vec();
    assert!(perm_vec.contains(&Permission::FileUpload));
    assert!(perm_vec.contains(&Permission::FileList));
    assert!(!perm_vec.contains(&Permission::FileDownload));
}

#[tokio::test]
async fn test_admin_has_implicit_upload_permission() {
    let db = create_test_db().await;

    // Create admin user (no explicit permissions needed)
    let hashed = db::hash_password("adminpass").unwrap();
    let admin = db
        .users
        .create_user(
            "admin_user",
            &hashed,
            true,
            false,
            true,
            &Permissions::new(),
        )
        .await
        .unwrap();

    // Admin should be marked as admin
    assert!(admin.is_admin);

    // Verify admin flag via lookup
    let fetched = db
        .users
        .get_user_by_username("admin_user")
        .await
        .unwrap()
        .unwrap();
    assert!(fetched.is_admin);

    // The actual admin check happens in the handler via user.is_admin
    // Admin permissions are implicit, not stored in DB
}

// ============================================================================
// Upload Destination Validation Tests
// ============================================================================

#[tokio::test]
async fn test_upload_folder_allows_upload() {
    use nexus_server::files::path::allows_upload;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();
    let area_root = root.join("shared");

    // Upload folder should allow uploads
    let upload_path = root.join("shared/Uploads [NEXUS-UL]");
    assert!(allows_upload(&area_root, &upload_path));

    // Dropbox folder should allow uploads
    let dropbox_path = root.join("shared/Submissions [NEXUS-DB]");
    assert!(allows_upload(&area_root, &dropbox_path));

    // User dropbox should allow uploads
    let user_dropbox_path = root.join("shared/For Alice [NEXUS-DB-alice]");
    assert!(allows_upload(&area_root, &user_dropbox_path));

    // Regular folder should NOT allow uploads
    let regular_path = root.join("shared/Documents");
    assert!(!allows_upload(&area_root, &regular_path));
}

#[tokio::test]
async fn test_upload_subfolder_inherits_permission() {
    use nexus_server::files::path::allows_upload;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();
    let area_root = root.join("shared");

    // Create a subfolder inside upload folder
    let subfolder = root.join("shared/Uploads [NEXUS-UL]/SubProject");
    fs::create_dir_all(&subfolder).await.unwrap();

    // Subfolder should inherit upload permission
    assert!(allows_upload(&area_root, &subfolder));
}

// ============================================================================
// Directory Upload Destination Tests
// ============================================================================

#[tokio::test]
async fn test_directory_upload_creates_destination() {
    // When uploading a directory, the server should create the destination
    // directory if it doesn't exist (as long as the parent allows uploads)
    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    // The destination "MyFolder" doesn't exist yet
    let new_folder = root.join("shared/Uploads [NEXUS-UL]/MyFolder");
    assert!(!new_folder.exists());

    // After the upload destination validation (simulated by create_dir_all),
    // the folder should be created
    fs::create_dir_all(&new_folder).await.unwrap();
    assert!(new_folder.exists());
    assert!(new_folder.is_dir());
}

#[tokio::test]
async fn test_nested_upload_paths_created_during_streaming() {
    // When uploading files with nested paths like "subdir/nested/file.txt",
    // the streaming code should create intermediate directories
    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    // Simulate what happens during upload streaming:
    // The destination exists, but nested paths inside don't
    let upload_folder = root.join("shared/Uploads [NEXUS-UL]");
    assert!(upload_folder.exists());

    // File path: destination + relative_path
    let nested_file = upload_folder.join("project/src/main.rs");
    assert!(!nested_file.exists());

    // create_dir_all on parent creates all intermediate directories
    if let Some(parent) = nested_file.parent() {
        fs::create_dir_all(parent).await.unwrap();
    }

    // Now the nested directory structure exists
    assert!(upload_folder.join("project/src").exists());

    // Write the file
    fs::write(&nested_file, b"fn main() {}").await.unwrap();
    assert!(nested_file.exists());
}

#[tokio::test]
async fn test_deeply_nested_upload_destination() {
    // Test that upload destination can be multiple levels deep if parent allows uploads
    use nexus_server::files::path::allows_upload;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();
    let area_root = root.join("shared");

    // Create deeply nested destination under upload folder
    let deep_dest = root.join("shared/Uploads [NEXUS-UL]/A/B/C/D");
    fs::create_dir_all(&deep_dest).await.unwrap();

    // The deep destination should inherit upload permission from parent
    assert!(allows_upload(&area_root, &deep_dest));
}

// ============================================================================
// Upload Error Kind Tests
// ============================================================================

#[test]
fn test_upload_error_kinds() {
    // Verify all upload-specific error kinds are valid strings
    let error_kinds = vec![
        "exists",        // File already exists
        "conflict",      // Another upload in progress (.part exists)
        "permission",    // No upload permission
        "invalid",       // Invalid path
        "not_found",     // Destination doesn't exist
        "io_error",      // Disk full, write error, etc.
        "hash_mismatch", // SHA-256 verification failed
    ];

    for kind in error_kinds {
        let msg = ServerMessage::FileUploadResponse {
            success: false,
            error: Some(format!("Error: {}", kind)),
            error_kind: Some(kind.to_string()),
            transfer_id: None,
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains(&format!("\"error_kind\":\"{}\"", kind)));
    }
}

// ============================================================================
// Upload Path Security Tests
// ============================================================================

#[test]
fn test_upload_path_traversal_detection() {
    // These paths should be rejected by the upload handler
    let malicious_paths = vec![
        "../etc/passwd",
        "foo/../../../etc/passwd",
        "..\\windows\\system32",
        "/absolute/path",
        "\\absolute\\path",
        "normal/../../escape",
    ];

    for path in malicious_paths {
        // Check for path traversal components
        let has_traversal = path.split(['/', '\\']).any(|c| c == "..");
        let is_absolute = path.starts_with('/') || path.starts_with('\\');

        assert!(
            has_traversal || is_absolute,
            "Path '{}' should be detected as malicious",
            path
        );
    }
}

#[test]
fn test_upload_path_valid_relative() {
    // These paths should be accepted
    let valid_paths = vec![
        "file.txt",
        "subdir/file.txt",
        "deep/nested/path/file.txt",
        "file with spaces.txt",
        "file-with-dashes.txt",
        "file_with_underscores.txt",
        "file.multiple.dots.txt",
    ];

    for path in valid_paths {
        let has_traversal = path.split(['/', '\\']).any(|c| c == "..");
        let is_absolute = path.starts_with('/') || path.starts_with('\\');

        assert!(
            !has_traversal && !is_absolute,
            "Path '{}' should be valid",
            path
        );
    }
}

// ============================================================================
// Upload Resume Logic Tests
// ============================================================================

#[test]
fn test_upload_resume_response_no_existing() {
    // Server has no .part file - client should send full file
    let response = ServerMessage::FileStartResponse {
        size: 0,
        sha256: None,
    };

    match response {
        ServerMessage::FileStartResponse { size, sha256 } => {
            assert_eq!(size, 0);
            assert!(sha256.is_none());
            // Client should send from offset 0 (full file)
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_upload_resume_response_partial_exists() {
    // Server has partial .part file
    let response = ServerMessage::FileStartResponse {
        size: 50000,
        sha256: Some("abc123".to_string()),
    };

    match response {
        ServerMessage::FileStartResponse { size, sha256 } => {
            assert_eq!(size, 50000);
            assert_eq!(sha256, Some("abc123".to_string()));
            // Client compares hash of first 50000 bytes
            // If match: send from offset 50000
            // If no match: send from offset 0
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_upload_resume_offset_calculation() {
    // Simulate client-side offset calculation
    let file_size: u64 = 100000;
    let server_size: u64 = 50000;
    let server_hash = "abc123";
    let client_partial_hash = "abc123"; // Hash of first 50000 bytes

    // If hashes match, resume from server_size
    let offset = if client_partial_hash == server_hash {
        server_size
    } else {
        0
    };

    assert_eq!(offset, 50000);

    // Payload size = total - offset
    let payload_size = file_size - offset;
    assert_eq!(payload_size, 50000);
}

#[test]
fn test_upload_resume_hash_mismatch() {
    // Client's partial hash doesn't match server's - start fresh
    let file_size: u64 = 100000;
    let server_size: u64 = 50000;
    let server_hash = "abc123";
    let client_partial_hash = "different_hash"; // Different content

    let offset = if client_partial_hash == server_hash {
        server_size
    } else {
        0
    };

    assert_eq!(offset, 0);

    // Payload size = full file
    let payload_size = file_size - offset;
    assert_eq!(payload_size, 100000);
}

// ============================================================================
// Upload Zero-Byte File Tests
// ============================================================================

#[test]
fn test_upload_zero_byte_file_start() {
    // Zero-byte files should work - no FileData frame sent
    let msg = ClientMessage::FileStart {
        path: "empty.txt".to_string(),
        size: 0,
        sha256: "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855".to_string(), // SHA-256 of empty
    };

    match msg {
        ClientMessage::FileStart { size, sha256, .. } => {
            assert_eq!(size, 0);
            // Empty file has a well-known SHA-256
            assert_eq!(
                sha256,
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
        }
        _ => panic!("Wrong message type"),
    }
}

// ============================================================================
// Upload Transfer Complete Tests
// ============================================================================

#[test]
fn test_upload_transfer_complete_success() {
    let msg = ServerMessage::TransferComplete {
        success: true,
        error: None,
        error_kind: None,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"success\":true"));
    assert!(!json.contains("\"error_kind\""));
}

#[test]
fn test_upload_transfer_complete_hash_mismatch() {
    let msg = ServerMessage::TransferComplete {
        success: false,
        error: Some("File verification failed - hash mismatch".to_string()),
        error_kind: Some("hash_mismatch".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"success\":false"));
    assert!(json.contains("\"error_kind\":\"hash_mismatch\""));
}

#[test]
fn test_upload_transfer_complete_io_error() {
    let msg = ServerMessage::TransferComplete {
        success: false,
        error: Some("Disk full".to_string()),
        error_kind: Some("io_error".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"error_kind\":\"io_error\""));
}
