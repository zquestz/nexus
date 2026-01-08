# Transfers

File transfers use a dedicated port (7501) for uploads and downloads. This separation allows different QoS/traffic shaping policies and keeps large file transfers from blocking chat.

## Overview

- **Port 7500:** Main BBS protocol (chat, users, news, file browsing)
- **Port 7501:** File transfers (uploads and downloads)

Both ports use the same TLS certificate, frame format, and authentication system.

## Connection Model

**One connection = one transfer.** After a transfer completes, the server closes the connection. Clients reconnect for each new transfer.

**Certificate verification:** Clients MUST verify that port 7501 presents the same certificate fingerprint as port 7500.

## Download Flow

```
Client                                        Server
   │                                             │
   │  ─────── Connect TLS to port 7501 ─────►    │
   │                                             │
   │  Handshake { version }                      │
   │ ───────────────────────────────────────►    │
   │         HandshakeResponse { version }       │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  Login { username, password, ... }          │
   │ ───────────────────────────────────────►    │
   │         LoginResponse { success }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  FileDownload { path, root }                │
   │ ───────────────────────────────────────►    │
   │         FileDownloadResponse { size, ... }  │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  ┌──── For each file: ──────────────────┐   │
   │  │                                      │   │
   │  │     FileStart { path, size, sha256 } │   │
   │  │ ◄────────────────────────────────────│   │
   │  │                                      │   │
   │  │  FileStartResponse { size, sha256 }  │   │
   │  │ ────────────────────────────────────►│   │
   │  │                                      │   │
   │  │     FileData [raw bytes]             │   │
   │  │ ◄────────────────────────────────────│   │
   │  │                                      │   │
   │  └──────────────────────────────────────┘   │
   │                                             │
   │         TransferComplete { success }        │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  ─────── Server closes connection ─────     │
```

## Upload Flow

```
Client                                        Server
   │                                             │
   │  ─────── Connect TLS to port 7501 ─────►    │
   │                                             │
   │  Handshake { version }                      │
   │ ───────────────────────────────────────►    │
   │         HandshakeResponse { version }       │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  Login { username, password, ... }          │
   │ ───────────────────────────────────────►    │
   │         LoginResponse { success }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  FileUpload { destination, file_count, ...} │
   │ ───────────────────────────────────────►    │
   │         FileUploadResponse { success, ... } │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  ┌──── For each file: ──────────────────┐   │
   │  │                                      │   │
   │  │  FileStart { path, size, sha256 }    │   │
   │  │ ────────────────────────────────────►│   │
   │  │                                      │   │
   │  │    FileStartResponse { size, sha256 }│   │
   │  │ ◄────────────────────────────────────│   │
   │  │                                      │   │
   │  │  FileData [raw bytes]                │   │
   │  │ ────────────────────────────────────►│   │
   │  │                                      │   │
   │  └──────────────────────────────────────┘   │
   │                                             │
   │         TransferComplete { success }        │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │  ─────── Server closes connection ─────     │
```

## Messages

### FileDownload (Client → Server)

Request to download a file or directory.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Path to download (file or directory) |
| `root` | boolean | No | If true, path is relative to file root (default: false) |

**Single file:**

```json
{
  "path": "/Documents/report.pdf"
}
```

**Directory:**

```json
{
  "path": "/Games"
}
```

**With root mode:**

```json
{
  "path": "/shared/Software",
  "root": true
}
```

### FileDownloadResponse (Server → Client)

Response to download request.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Human-readable error message |
| `error_kind` | string | If failure | Machine-readable error type |
| `size` | integer | If success | Total size of all files in bytes |
| `file_count` | integer | If success | Number of files to transfer |
| `transfer_id` | string | If success | Transfer ID for logging (8 hex chars) |

**Success example:**

```json
{
  "success": true,
  "size": 1048576,
  "file_count": 5,
  "transfer_id": "a1b2c3d4"
}
```

**Empty directory:**

```json
{
  "success": true,
  "size": 0,
  "file_count": 0,
  "transfer_id": "e5f6a7b8"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "File not found",
  "error_kind": "not_found"
}
```

### FileUpload (Client → Server)

Request to upload files.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `destination` | string | Yes | Destination directory on server |
| `file_count` | integer | Yes | Number of files to upload |
| `total_size` | integer | Yes | Total size of all files in bytes |
| `root` | boolean | No | If true, destination is relative to file root |

**Example:**

```json
{
  "destination": "/Uploads",
  "file_count": 3,
  "total_size": 5242880
}
```

### FileUploadResponse (Server → Client)

Response to upload request.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request is accepted |
| `error` | string | If failure | Human-readable error message |
| `error_kind` | string | If failure | Machine-readable error type |
| `transfer_id` | string | If success | Transfer ID for logging (8 hex chars) |

**Success example:**

```json
{
  "success": true,
  "transfer_id": "c3d4e5f6"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Upload not allowed in this directory",
  "error_kind": "permission"
}
```

### FileStart (Bidirectional)

Announces a file to transfer. Sent by server for downloads, by client for uploads.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Relative path (e.g., `"subdir/file.txt"`) |
| `size` | integer | Yes | File size in bytes |
| `sha256` | string | Yes | SHA-256 hash of complete file |

**Example:**

```json
{
  "path": "Games/app.zip",
  "size": 1048576,
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

**Notes:**
- Path is relative (no leading slash)
- Path uses forward slashes regardless of OS
- 0-byte files are valid (sha256 is the empty file hash)

### FileStartResponse (Bidirectional)

Reports local file state for resume. Sent by client for downloads, by server for uploads.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `size` | integer | Yes | Size of local file (0 if none exists) |
| `sha256` | string | If size > 0 | SHA-256 hash of local file |

**No local file:**

```json
{
  "size": 0
}
```

**Partial file (for resume):**

```json
{
  "size": 524288,
  "sha256": "a1b2c3d4e5f6..."
}
```

**Complete file:**

```json
{
  "size": 1048576,
  "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
}
```

### FileData (Bidirectional)

Raw file bytes. The frame payload contains the binary file data.

- Sent by server for downloads
- Sent by client for uploads
- Payload length indicates bytes in this chunk
- May be skipped entirely if file is already complete

**Frame format:**

```
NX|8|FileData|a1b2c3d4e5f6|65536|[binary data]
```

### FileHashing (Bidirectional)

Keepalive sent while computing SHA-256 hash for large files.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `file` | string | Yes | File being hashed (for logging) |

**Example:**

```json
{
  "file": "large-archive.zip"
}
```

This message is sent every 10 seconds during hash computation to prevent idle timeouts. Receivers should reset their idle timer but otherwise ignore it.

### TransferComplete (Server → Client)

Signals transfer completion.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether transfer succeeded |
| `error` | string | If failure | Human-readable error message |
| `error_kind` | string | If failure | Machine-readable error type |

**Success:**

```json
{
  "success": true
}
```

**Failure:**

```json
{
  "success": false,
  "error": "SHA-256 verification failed",
  "error_kind": "hash_mismatch"
}
```

## Resume Logic

### Download Resume

1. Server sends `FileStart` with file metadata
2. Client checks local `.part` file (or completed file)
3. Client responds with `FileStartResponse`:
   - `size: 0` if no local file
   - `size: N, sha256: "..."` if partial/complete file exists
4. Server computes offset:
   - If `size: 0` → send entire file
   - If hash matches first N bytes → resume from offset N
   - If hash mismatch → send entire file (start over)
5. If file already complete, `FileData` is skipped

### Upload Resume

1. Client sends `FileStart` with file metadata
2. Server checks local `.part` file (or completed file)
3. Server responds with `FileStartResponse`:
   - `size: 0` if no server file
   - `size: N, sha256: "..."` if partial/complete file exists
4. Client computes offset:
   - If `size: 0` → send entire file
   - If hash matches first N bytes → resume from offset N
   - If hash mismatch → send entire file (start over)
5. If file already complete, `FileData` is skipped

### Partial Files

- Downloads use `.part` suffix until complete
- Uploads use `.part` suffix on server until verified
- After successful SHA-256 verification, `.part` is renamed to final name

## Error Kinds

| Value | Description |
|-------|-------------|
| `not_found` | Path doesn't exist |
| `permission` | Permission denied |
| `invalid` | Invalid input (malformed path) |
| `unsupported_version` | Protocol version not supported |
| `disk_full` | Disk full |
| `hash_mismatch` | SHA-256 verification failed |
| `io_error` | File I/O error |
| `protocol_error` | Invalid/unexpected data |
| `exists` | File already exists (upload only) |
| `conflict` | Concurrent upload in progress |

## Timeouts

| Context | Timeout | Description |
|---------|---------|-------------|
| Connection | 30 seconds | TLS handshake must complete |
| Idle | 30 seconds | Time waiting for first byte of frame |
| Frame | 60 seconds | Frame must complete within 60s of first byte |
| FileData progress | 60 seconds | Must receive some bytes within 60s |

**Note:** Unlike port 7500, port 7501 does not allow indefinite idle connections.

## Permissions

| Permission | Required For |
|------------|--------------|
| `file_download` | Downloading files |
| `file_upload` | Uploading files |
| `file_root` | Using `root: true` for file root access |

### Upload Destination Requirements

Uploads are only allowed to:
- `[NEXUS-UL]` folders (upload folders)
- `[NEXUS-DB]` folders (dropbox folders)

The server creates parent directories automatically if they don't exist (within the upload folder).

## Port 7501 Authentication

The login flow on port 7501 is identical to port 7500, but `LoginResponse` only includes:

| Field | Type | Description |
|-------|------|-------------|
| `success` | boolean | Whether login succeeded |
| `error` | string | Error message (if failed) |

No `session_id`, `permissions`, `server_info`, or `chat_info` is returned on the transfer port.

## Path Handling

### Downloads

- `FileDownload.path`: Server path with leading slash (e.g., `/Games`)
- `FileStart.path`: Relative path, no leading slash (e.g., `Games/app.zip`)
- Client saves to: `{download_destination}/{FileStart.path}`

### Uploads

- `FileUpload.destination`: Server directory (e.g., `/Uploads`)
- `FileStart.path`: Relative path, no leading slash (e.g., `subdir/file.txt`)
- Server saves to: `{destination}/{FileStart.path}`

## Special Cases

### Empty Directories

- `FileDownloadResponse` with `file_count: 0`
- No `FileStart` or `FileData` messages
- Immediate `TransferComplete`

### Zero-Byte Files

- `FileStart` sent with `size: 0` and empty-file SHA-256
- `FileStartResponse` sent as normal
- No `FileData` message (nothing to transfer)
- Proceed to next file

### No Overwrite

If a file already exists with different content:
- Upload fails with `error_kind: "exists"`
- Admin must delete existing file for replacement

## Notes

- Transfer port is communicated in `LoginResponse.server_info.transfer_port` (always present)
- SHA-256 is computed with hardware acceleration when available
- Large files use streaming (64KB buffers)
- Symlinks are followed transparently
- Directories are downloaded recursively

## Next Step

- Manage server and users with [admin commands](09-admin.md)
- Handle [errors](10-errors.md)