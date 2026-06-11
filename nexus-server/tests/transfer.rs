//! Integration tests for the file transfer protocol (port 7501): downloads,
//! uploads, auth, permission checks, streaming, and resume logic.

mod common;

use std::io::Cursor;

use common::create_test_db;
use nexus_common::framing::{FrameContexts, FrameReader, FrameWriter, MessageId, RawFrame};
use nexus_common::names::fold_name;
use nexus_common::protocol::{ClientMessage, ServerMessage};
use nexus_server::db::{self, CreateUserParams, Permission, Permissions};
use tempfile::TempDir;
use tokio::fs;
use tokio::io::BufReader;

/// Build a temp file area populated with shared dirs (default, upload,
/// generic/user dropbox), per-user dirs, and a few test files.
async fn create_test_file_area() -> TempDir {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let root = temp_dir.path();

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

#[test]
fn test_dropbox_path_parsing() {
    use nexus_server::files::folder_type::{FolderType, parse_folder_type};

    assert!(matches!(
        parse_folder_type("Documents"),
        FolderType::Default
    ));

    assert!(matches!(
        parse_folder_type("Uploads [NEXUS-UL]"),
        FolderType::Upload
    ));

    assert!(matches!(
        parse_folder_type("Submissions [NEXUS-DB]"),
        FolderType::DropBox
    ));

    match parse_folder_type("For Alice [NEXUS-DB-alice]") {
        FolderType::UserDropBox(owner) => assert_eq!(fold_name(&owner), "alice"),
        _ => panic!("Expected UserDropBox"),
    }

    // Suffix matching is case-insensitive.
    assert!(matches!(
        parse_folder_type("uploads [nexus-ul]"),
        FolderType::Upload
    ));
}

#[tokio::test]
async fn test_resolve_user_area_with_personal_folder() {
    use nexus_server::files::area::resolve_user_area;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    // Alice has a personal folder; charlie falls back to shared.
    let alice_area = resolve_user_area(root, "alice").await;
    assert!(alice_area.ends_with("users/alice"));

    let charlie_area = resolve_user_area(root, "charlie").await;
    assert!(charlie_area.ends_with("shared"));
}

#[tokio::test]
async fn test_path_validation_rejects_traversal() {
    use nexus_server::files::path::build_and_validate_candidate_path;

    let temp_dir = create_test_file_area().await;
    let area_root = temp_dir.path().join("shared");

    let result = build_and_validate_candidate_path(&area_root, "/Documents/readme.txt").await;
    assert!(result.is_ok());

    let result = build_and_validate_candidate_path(&area_root, "/../../../etc/passwd").await;
    assert!(result.is_err());

    // Embedded traversal should fail too.
    let result =
        build_and_validate_candidate_path(&area_root, "/Documents/../../../etc/passwd").await;
    assert!(result.is_err());
}

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

#[tokio::test]
async fn test_blake3_known_value() {
    let mut hasher = nexus_common::hash::StreamingHasher::new();
    hasher.update(b"Hello, World!");
    let hash = hasher.finalize();

    assert_eq!(
        hash,
        "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
    );
}

#[tokio::test]
async fn test_file_download_permission_in_db() {
    let db = create_test_db().await;

    let hashed = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::FileDownload);
    perms.add(Permission::FileList);

    let user = db
        .users
        .create_user(CreateUserParams {
            username: "downloader",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    let stored_perms = db.users.get_user_permissions(user.id).await.unwrap();
    let perm_vec = stored_perms.to_vec();
    assert!(perm_vec.contains(&Permission::FileDownload));
    assert!(perm_vec.contains(&Permission::FileList));
    assert!(!perm_vec.contains(&Permission::FileRoot));
}

#[tokio::test]
async fn test_admin_has_implicit_permissions() {
    let db = create_test_db().await;

    // Admin with no explicit permissions.
    let hashed = db::hash_password(
        "password",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let admin = db
        .users
        .create_user(CreateUserParams {
            username: "admin",
            hashed_password: &hashed,
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    assert!(admin.is_admin);

    let fetched = db
        .users
        .get_user_by_username("admin")
        .await
        .unwrap()
        .unwrap();
    assert!(fetched.is_admin);
}

#[test]
fn test_file_download_message_serialization() {
    let msg = ClientMessage::FileDownload {
        path: "/Documents/readme.txt".to_string(),
        root: false,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FileDownload\""));
    assert!(json.contains("\"path\":\"/Documents/readme.txt\""));

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
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FileStart\""));
    assert!(json.contains("\"path\":\"Documents/readme.txt\""));
    assert!(json.contains("\"size\":1024"));
}

#[test]
fn test_file_start_response_no_local_file() {
    let msg = ClientMessage::FileStartResponse {
        size: 0,
        blake3: None,
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"size\":0"));
    assert!(!json.contains("\"blake3\"")); // None should be omitted
    assert!(!json.contains("\"sha256\""));
}

#[test]
fn test_file_start_response_with_partial() {
    let msg = ClientMessage::FileStartResponse {
        size: 512,
        blake3: Some("partial_hash_here".to_string()),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"size\":512"));
    assert!(json.contains("\"blake3\":\"partial_hash_here\""));
    assert!(!json.contains("\"sha256\""));
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

#[test]
fn test_server_file_hash_message() {
    // Server sends FileHash after streaming download data (or alone if file was skipped)
    let msg = ServerMessage::FileHash {
        blake3: "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FileHash\""));
    assert!(json.contains(
        "\"blake3\":\"288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8\""
    ));
    assert!(!json.contains("\"sha256\""));
}

#[test]
fn test_client_file_hash_message() {
    // Client sends FileHash after streaming upload data (or alone if file was skipped)
    let msg = ClientMessage::FileHash {
        blake3: "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262".to_string(),
    };

    let json = serde_json::to_string(&msg).unwrap();
    assert!(json.contains("\"type\":\"FileHash\""));
    assert!(json.contains(
        "\"blake3\":\"af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262\""
    ));
    assert!(!json.contains("\"sha256\""));
}

#[test]
fn test_file_hash_deserialization() {
    let json = r#"{"type":"FileHash","blake3":"abc123def456"}"#;

    let server_msg: ServerMessage = serde_json::from_str(json).unwrap();
    match server_msg {
        ServerMessage::FileHash { blake3 } => {
            assert_eq!(blake3, "abc123def456");
        }
        _ => panic!("Expected FileHash"),
    }

    let client_msg: ClientMessage = serde_json::from_str(json).unwrap();
    match client_msg {
        ClientMessage::FileHash { blake3 } => {
            assert_eq!(blake3, "abc123def456");
        }
        _ => panic!("Expected FileHash"),
    }
}

#[tokio::test]
async fn test_frame_roundtrip_file_download() {
    let msg = ClientMessage::FileDownload {
        path: "/test/file.txt".to_string(),
        root: false,
    };
    let payload = serde_json::to_vec(&msg).unwrap();
    let id = MessageId::new();

    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileDownload".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader
        .read_frame_in_context(FrameContexts::TRANSFER_CLIENT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.message_id, id);
    assert_eq!(frame.message_type, "FileDownload");

    let parsed: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
    match parsed {
        ClientMessage::FileDownload { path, root } => {
            assert_eq!(path, "/test/file.txt");
            assert!(!root);
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_transfer_id_in_response() {
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

#[test]
fn test_error_kinds() {
    // error_kind values the client must handle.
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

#[tokio::test]
async fn test_empty_directory_handling() {
    let temp_dir = TempDir::new().unwrap();
    let empty_dir = temp_dir.path().join("empty");
    fs::create_dir(&empty_dir).await.unwrap();

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

#[cfg(unix)]
#[tokio::test]
async fn test_symlink_in_file_area() {
    use std::os::unix::fs::symlink;

    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path();

    let real_file = root.join("real.txt");
    fs::write(&real_file, b"Real content").await.unwrap();

    let link_path = root.join("link.txt");
    symlink(&real_file, &link_path).unwrap();

    let metadata = fs::symlink_metadata(&link_path).await.unwrap();
    assert!(metadata.file_type().is_symlink());

    // Reading through the symlink resolves to the target's content.
    let content = fs::read_to_string(&link_path).await.unwrap();
    assert_eq!(content, "Real content");
}

#[test]
fn test_resume_response_new_download() {
    // Client has no local file - should start from beginning
    let response = ClientMessage::FileStartResponse {
        size: 0,
        blake3: None,
    };

    match response {
        ClientMessage::FileStartResponse { size, blake3 } => {
            assert_eq!(size, 0);
            assert!(blake3.is_none());
        }
        _ => panic!("Wrong type"),
    }
}

#[test]
fn test_resume_response_partial_file() {
    // Client has partial file with hash
    let response = ClientMessage::FileStartResponse {
        size: 512,
        blake3: Some("abc123def456".to_string()),
    };

    match response {
        ClientMessage::FileStartResponse { size, blake3 } => {
            assert_eq!(size, 512);
            assert_eq!(blake3.unwrap(), "abc123def456");
        }
        _ => panic!("Wrong type"),
    }
}

#[test]
fn test_resume_response_complete_file() {
    // Client has complete file (size matches server's file)
    let response = ClientMessage::FileStartResponse {
        size: 1024, // Same as server's file
        blake3: Some("full_file_hash".to_string()),
    };

    match response {
        ClientMessage::FileStartResponse { size, blake3 } => {
            assert_eq!(size, 1024);
            assert!(blake3.is_some());
        }
        _ => panic!("Wrong type"),
    }
}

#[tokio::test]
async fn test_directory_scan_filters_dropbox_contents() {
    // scan_directory_recursive checks can_access_for_download per entry, so a
    // parent-dir download filters out dropbox contents the user can't access.
    // This test only verifies the fixtures exist.
    use nexus_server::files::folder_type::{FolderType, parse_folder_type};

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    assert!(matches!(
        parse_folder_type("Submissions [NEXUS-DB]"),
        FolderType::DropBox
    ));
    assert!(matches!(
        parse_folder_type("For Alice [NEXUS-DB-alice]"),
        FolderType::UserDropBox(_)
    ));

    let dropbox_file = root.join("shared/Submissions [NEXUS-DB]/secret.txt");
    assert!(dropbox_file.exists());

    let alice_dropbox_file = root.join("shared/For Alice [NEXUS-DB-alice]/alice_file.txt");
    assert!(alice_dropbox_file.exists());

    let regular_file = root.join("shared/Documents/readme.txt");
    assert!(regular_file.exists());
}

#[test]
fn test_dropbox_access_rules() {
    use nexus_server::files::folder_type::{FolderType, parse_folder_type};

    // Access rules scan_directory_recursive enforces per folder type:

    // Generic dropbox: admins only.
    assert!(matches!(
        parse_folder_type("Submissions [NEXUS-DB]"),
        FolderType::DropBox
    ));

    // User dropbox: named user and admins only.
    match parse_folder_type("For Alice [NEXUS-DB-alice]") {
        FolderType::UserDropBox(owner) => {
            assert_eq!(fold_name(&owner), "alice");
        }
        _ => panic!("Expected UserDropBox"),
    }

    // Upload and default folders: everyone can download.
    assert!(matches!(
        parse_folder_type("Uploads [NEXUS-UL]"),
        FolderType::Upload
    ));

    assert!(matches!(
        parse_folder_type("Documents"),
        FolderType::Default
    ));
}

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
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"FileStart\""));
    assert!(json.contains("\"path\":\"documents/report.pdf\""));
    assert!(json.contains("\"size\":2048"));
}

#[test]
fn test_server_file_start_response() {
    // Server responds with resume info
    let msg = ServerMessage::FileStartResponse {
        size: 1024,
        blake3: Some("partial_hash_here".to_string()),
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"type\":\"FileStartResponse\""));
    assert!(json.contains("\"size\":1024"));
    assert!(json.contains("\"blake3\":\"partial_hash_here\""));
    assert!(!json.contains("\"sha256\""));
}

#[test]
fn test_server_file_start_response_no_existing() {
    // Server has no existing .part file
    let msg = ServerMessage::FileStartResponse {
        size: 0,
        blake3: None,
    };
    let json = serde_json::to_string(&msg).unwrap();

    assert!(json.contains("\"size\":0"));
    // blake3 should be null or absent
}

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

    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileUpload".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader
        .read_frame_in_context(FrameContexts::TRANSFER_CLIENT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.message_id, id);
    assert_eq!(frame.message_type, "FileUpload");

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
    };
    let payload = serde_json::to_vec(&msg).unwrap();
    let id = MessageId::new();

    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileStart".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader
        .read_frame_in_context(FrameContexts::TRANSFER_CLIENT)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.message_type, "FileStart");

    let parsed: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
    match parsed {
        ClientMessage::FileStart { path, size } => {
            assert_eq!(path, "subdir/file.txt");
            assert_eq!(size, 4096);
        }
        _ => panic!("Wrong message type"),
    }
}

#[tokio::test]
async fn test_frame_roundtrip_file_hash() {
    // Both server and client FileHash variants must survive a frame roundtrip.
    let server_msg = ServerMessage::FileHash {
        blake3: "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8".to_string(),
    };
    let payload = serde_json::to_vec(&server_msg).unwrap();
    let id = MessageId::new();

    let mut buffer = Vec::new();
    {
        let cursor = Cursor::new(&mut buffer);
        let mut writer = FrameWriter::new(cursor);
        let frame = RawFrame::new(id, "FileHash".to_string(), payload.clone());
        writer.write_frame(&frame).await.unwrap();
    }

    let cursor = Cursor::new(buffer);
    let buf_reader = BufReader::new(cursor);
    let mut reader = FrameReader::new(buf_reader);

    let frame = reader
        .read_frame_in_context(FrameContexts::TRANSFER_SERVER)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frame.message_type, "FileHash");

    let parsed: ServerMessage = serde_json::from_slice(&frame.payload).unwrap();
    match parsed {
        ServerMessage::FileHash { blake3 } => {
            assert_eq!(
                blake3,
                "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
            );
        }
        _ => panic!("Wrong message type"),
    }

    // The client variant deserializes from the same payload.
    let client_parsed: ClientMessage = serde_json::from_slice(&frame.payload).unwrap();
    match client_parsed {
        ClientMessage::FileHash { blake3 } => {
            assert_eq!(
                blake3,
                "288a86a79f20a3d6dccdca7713beaed178798296bdfa7913fa2a62d9727bf8f8"
            );
        }
        _ => panic!("Wrong message type"),
    }
}

#[tokio::test]
async fn test_file_upload_permission_in_db() {
    let db = create_test_db().await;

    let hashed = db::hash_password(
        "pass123",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let mut perms = Permissions::new();
    perms.add(Permission::FileUpload);
    perms.add(Permission::FileList);

    let user = db
        .users
        .create_user(CreateUserParams {
            username: "uploader",
            hashed_password: &hashed,
            is_admin: false,
            is_shared: false,
            enabled: true,
            permissions: &perms,
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    let stored_perms = db.users.get_user_permissions(user.id).await.unwrap();
    let perm_vec = stored_perms.to_vec();
    assert!(perm_vec.contains(&Permission::FileUpload));
    assert!(perm_vec.contains(&Permission::FileList));
    assert!(!perm_vec.contains(&Permission::FileDownload));
}

#[tokio::test]
async fn test_admin_has_implicit_upload_permission() {
    let db = create_test_db().await;

    let hashed = db::hash_password(
        "adminpass",
        nexus_common::validators::PasswordStrength::Weak,
        true,
    )
    .unwrap();
    let admin = db
        .users
        .create_user(CreateUserParams {
            username: "admin_user",
            hashed_password: &hashed,
            is_admin: true,
            is_shared: false,
            enabled: true,
            permissions: &Permissions::new(),
            group_id: None,
            revokes: &[],
            bandwidth_weight: None,
        })
        .await
        .unwrap();

    assert!(admin.is_admin);

    let fetched = db
        .users
        .get_user_by_username("admin_user")
        .await
        .unwrap()
        .unwrap();
    assert!(fetched.is_admin);

    // Admin permissions are implicit (handler checks user.is_admin), not in DB.
}

#[tokio::test]
async fn test_upload_folder_allows_upload() {
    use nexus_server::files::path::allows_upload;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();
    let area_root = root.join("shared");

    // Upload, generic dropbox, and user dropbox folders allow uploads.
    let upload_path = root.join("shared/Uploads [NEXUS-UL]");
    assert!(allows_upload(&area_root, &upload_path));

    let dropbox_path = root.join("shared/Submissions [NEXUS-DB]");
    assert!(allows_upload(&area_root, &dropbox_path));

    let user_dropbox_path = root.join("shared/For Alice [NEXUS-DB-alice]");
    assert!(allows_upload(&area_root, &user_dropbox_path));

    // A regular folder does not.
    let regular_path = root.join("shared/Documents");
    assert!(!allows_upload(&area_root, &regular_path));
}

#[tokio::test]
async fn test_upload_subfolder_inherits_permission() {
    use nexus_server::files::path::allows_upload;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();
    let area_root = root.join("shared");

    // A subfolder inherits the parent upload folder's upload permission.
    let subfolder = root.join("shared/Uploads [NEXUS-UL]/SubProject");
    fs::create_dir_all(&subfolder).await.unwrap();

    assert!(allows_upload(&area_root, &subfolder));
}

#[tokio::test]
async fn test_directory_upload_creates_destination() {
    // The server creates a missing destination dir when the parent allows
    // uploads (simulated here by create_dir_all).
    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    let new_folder = root.join("shared/Uploads [NEXUS-UL]/MyFolder");
    assert!(!new_folder.exists());

    fs::create_dir_all(&new_folder).await.unwrap();
    assert!(new_folder.exists());
    assert!(new_folder.is_dir());
}

#[tokio::test]
async fn test_nested_upload_paths_created_during_streaming() {
    // Uploading a nested path ("subdir/nested/file.txt") creates intermediate
    // directories during streaming.
    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();

    let upload_folder = root.join("shared/Uploads [NEXUS-UL]");
    assert!(upload_folder.exists());

    // File path = destination + relative path; intermediate dirs don't exist yet.
    let nested_file = upload_folder.join("project/src/main.rs");
    assert!(!nested_file.exists());

    if let Some(parent) = nested_file.parent() {
        fs::create_dir_all(parent).await.unwrap();
    }

    assert!(upload_folder.join("project/src").exists());

    fs::write(&nested_file, b"fn main() {}").await.unwrap();
    assert!(nested_file.exists());
}

#[tokio::test]
async fn test_deeply_nested_upload_destination() {
    // A multi-level destination inherits upload permission from its parent.
    use nexus_server::files::path::allows_upload;

    let temp_dir = create_test_file_area().await;
    let root = temp_dir.path();
    let area_root = root.join("shared");

    let deep_dest = root.join("shared/Uploads [NEXUS-UL]/A/B/C/D");
    fs::create_dir_all(&deep_dest).await.unwrap();

    assert!(allows_upload(&area_root, &deep_dest));
}

#[test]
fn test_upload_error_kinds() {
    // All upload-specific error kinds must be valid strings.
    let error_kinds = vec![
        "exists",        // File already exists
        "conflict",      // Another upload in progress (.part exists)
        "permission",    // No upload permission
        "invalid",       // Invalid path
        "not_found",     // Destination doesn't exist
        "io_error",      // Disk full, write error, etc.
        "hash_mismatch", // BLAKE3 verification failed
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

#[test]
fn test_upload_path_traversal_detection() {
    // The upload handler must reject these paths.
    let malicious_paths = vec![
        "../etc/passwd",
        "foo/../../../etc/passwd",
        "..\\windows\\system32",
        "/absolute/path",
        "\\absolute\\path",
        "normal/../../escape",
    ];

    for path in malicious_paths {
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
    // The upload handler must accept these paths.
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

#[test]
fn test_upload_resume_response_no_existing() {
    // No .part file on server: client sends the full file.
    let response = ServerMessage::FileStartResponse {
        size: 0,
        blake3: None,
    };

    match response {
        ServerMessage::FileStartResponse { size, blake3 } => {
            assert_eq!(size, 0);
            assert!(blake3.is_none());
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_upload_resume_response_partial_exists() {
    // Partial .part file on server: client compares the hash of the first
    // `size` bytes, then resumes from `size` on match or restarts on mismatch.
    let response = ServerMessage::FileStartResponse {
        size: 50000,
        blake3: Some("abc123".to_string()),
    };

    match response {
        ServerMessage::FileStartResponse { size, blake3 } => {
            assert_eq!(size, 50000);
            assert_eq!(blake3, Some("abc123".to_string()));
        }
        _ => panic!("Wrong message type"),
    }
}

#[test]
fn test_upload_resume_offset_calculation() {
    // Client-side offset calc: matching partial hash resumes from server_size.
    let file_size: u64 = 100000;
    let server_size: u64 = 50000;
    let server_hash = "abc123";
    let client_partial_hash = "abc123"; // Hash of first 50000 bytes

    let offset = if client_partial_hash == server_hash {
        server_size
    } else {
        0
    };

    assert_eq!(offset, 50000);

    let payload_size = file_size - offset;
    assert_eq!(payload_size, 50000);
}

#[test]
fn test_upload_resume_hash_mismatch() {
    // Mismatched partial hash restarts from offset 0 (full file).
    let file_size: u64 = 100000;
    let server_size: u64 = 50000;
    let server_hash = "abc123";
    let client_partial_hash = "different_hash";

    let offset = if client_partial_hash == server_hash {
        server_size
    } else {
        0
    };

    assert_eq!(offset, 0);

    let payload_size = file_size - offset;
    assert_eq!(payload_size, 100000);
}

#[test]
fn test_upload_zero_byte_file_start() {
    // Zero-byte files send no FileData frame.
    let msg = ClientMessage::FileStart {
        path: "empty.txt".to_string(),
        size: 0,
    };

    match msg {
        ClientMessage::FileStart { size, .. } => {
            assert_eq!(size, 0);
        }
        _ => panic!("Wrong message type"),
    }
}

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
