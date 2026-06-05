# Transfers

File transfers use a dedicated port (7501) for uploads and downloads. This separation allows different QoS/traffic shaping policies and keeps large file transfers from blocking chat.

## Overview

- **Port 7500:** Main BBS protocol (chat, users, news, file browsing)
- **Port 7501:** File transfers (uploads and downloads)

Both ports use the same TLS certificate, frame format, and authentication system.

## Connection Model

**One connection = one transfer.** After a transfer completes, the server closes the connection. Clients reconnect for each new transfer.

**Certificate verification:** Both ports use the same TLS certificate, so the fingerprint is identical. The transfer-port handshake also returns the server's `fingerprint` in `HandshakeResponse`, allowing the same staged verification used on port 7500 — clients abort before sending credentials if either stage fails. See [01-handshake.md](01-handshake.md#handshakeresponse-server--client) for the field; see the main [README](README.md#tls) for the staged-verification model.

## Download Flow

```
Client                                        Server
   │                                             │
   │  ─────── Connect TLS to port 7501 ─────►    │
   │                                             │
   │  Handshake { version }                      │
   │ ───────────────────────────────────────►    │
   │  HandshakeResponse { version, fingerprint } │
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
   │  │     FileStart { path, size }         │   │
   │  │ ◄────────────────────────────────────│   │
   │  │                                      │   │
   │  │  FileStartResponse { size, sha256 }  │   │
   │  │ ────────────────────────────────────►│   │
   │  │                                      │   │
   │  │     FileData [raw bytes]             │   │
   │  │ ◄────────────────────────────────────│   │
   │  │                                      │   │
   │  │     FileHash { sha256 }              │   │
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
   │  HandshakeResponse { version, fingerprint } │
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
   │  │  FileStart { path, size }            │   │
   │  │ ────────────────────────────────────►│   │
   │  │                                      │   │
   │  │    FileStartResponse { size, sha256 }│   │
   │  │ ◄────────────────────────────────────│   │
   │  │                                      │   │
   │  │  FileData [raw bytes]                │   │
   │  │ ────────────────────────────────────►│   │
   │  │                                      │   │
   │  │  FileHash { sha256 }                 │   │
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

| Field  | Type    | Required | Description                                             |
| ------ | ------- | -------- | ------------------------------------------------------- |
| `path` | string  | Yes      | Path to download (file or directory)                    |
| `root` | boolean | No       | If true, path is relative to file root (default: false) |

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

| Field         | Type    | Required   | Description                           |
| ------------- | ------- | ---------- | ------------------------------------- |
| `success`     | boolean | Yes        | Whether the request succeeded         |
| `error`       | string  | If failure | Human-readable error message          |
| `error_kind`  | string  | If failure | Machine-readable error type           |
| `size`        | integer | If success | Total size of all files in bytes      |
| `file_count`  | integer | If success | Number of files to transfer           |
| `transfer_id` | string  | If success | Transfer ID for logging (8 hex chars) |

For directory downloads, the server scans the directory before this response so
it can report `size` and `file_count`. If that scan exceeds the server's bounded
scan window, the server returns a failed `FileDownloadResponse` with
`error_kind: "io_error"` and closes the transfer.

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

| Field         | Type    | Required | Description                                   |
| ------------- | ------- | -------- | --------------------------------------------- |
| `destination` | string  | Yes      | Destination directory on server               |
| `file_count`  | integer | Yes      | Number of files to upload                     |
| `total_size`  | integer | Yes      | Total size of all files in bytes              |
| `root`        | boolean | No       | If true, destination is relative to file root |

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

| Field         | Type    | Required   | Description                           |
| ------------- | ------- | ---------- | ------------------------------------- |
| `success`     | boolean | Yes        | Whether the request is accepted       |
| `error`       | string  | If failure | Human-readable error message          |
| `error_kind`  | string  | If failure | Machine-readable error type           |
| `transfer_id` | string  | If success | Transfer ID for logging (8 hex chars) |

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

Announces a file to transfer. Sent by server for downloads, by client for uploads. The file hash is not included here — it is sent separately in `FileHash` after the data (or alone if no data is sent).

| Field  | Type    | Required | Description                               |
| ------ | ------- | -------- | ----------------------------------------- |
| `path` | string  | Yes      | Relative path (e.g., `"subdir/file.txt"`) |
| `size` | integer | Yes      | File size in bytes                        |

**Example:**

```json
{
  "path": "Games/app.zip",
  "size": 1048576
}
```

**Notes:**

- Path is relative (no leading slash)
- Path uses forward slashes regardless of OS
- 0-byte files are valid (`size: 0`)

### FileStartResponse (Bidirectional)

Reports local file state for resume. Sent by client for downloads, by server for uploads.

| Field    | Type    | Required    | Description                           |
| -------- | ------- | ----------- | ------------------------------------- |
| `size`   | integer | Yes         | Size of local file (0 if none exists) |
| `sha256` | string  | If size > 0 | SHA-256 hash of local file            |

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

Keepalive sent while computing SHA-256 hash for large files (e.g., during resume verification of existing data).

| Field  | Type   | Required | Description                     |
| ------ | ------ | -------- | ------------------------------- |
| `file` | string | Yes      | File being hashed (for logging) |

**Example:**

```json
{
  "file": "large-archive.zip"
}
```

This message is sent every 10 seconds during hash computation to prevent idle timeouts. Receivers should reset their idle timer but otherwise ignore it. Multiple consecutive `FileHashing` frames may arrive if the hash computation takes a long time.

### FileHash (Bidirectional)

Carries the sender's SHA-256 hash of the complete file. Sent after `FileData` (normal transfer) or alone without `FileData` (file already complete or zero-byte).

| Field    | Type   | Required | Description                       |
| -------- | ------ | -------- | --------------------------------- |
| `sha256` | string | Yes      | SHA-256 hash of the complete file |

**Example:**

```json
{
  "sha256": "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f"
}
```

**Notes:**

- Both sides independently compute the hash during streaming (single-pass)
- The receiver compares its own computed hash against the sender's `FileHash`
- A mismatch means data corruption — the receiver should delete the `.part` file and report an error
- For zero-byte files, the hash is the SHA-256 of empty input (`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`)

**Per-file frame dispatch after `FileStartResponse`:**

The receiver reads the next frame and dispatches based on type:

| Frame Type         | Meaning                                                   |
| ------------------ | --------------------------------------------------------- |
| `FileHashing`      | Keepalive — consume payload, skip, continue reading       |
| `FileData`         | Data transfer — stream to disk, then read `FileHash` next |
| `FileHash`         | File was skipped (already complete or zero-byte)          |
| `TransferComplete` | Transfer terminated early (error during resume, etc.)     |

After `FileData` is received and streamed, the very next meaningful frame MUST be `FileHash`. Any other frame type is a protocol error.

### TransferComplete (Server → Client)

Signals transfer completion.

| Field        | Type    | Required   | Description                  |
| ------------ | ------- | ---------- | ---------------------------- |
| `success`    | boolean | Yes        | Whether transfer succeeded   |
| `error`      | string  | If failure | Human-readable error message |
| `error_kind` | string  | If failure | Machine-readable error type  |

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

Both sides use a `StreamingHasher` that supports clone-and-finalize (`partial_hash()`) without consuming the hasher. This enables single-pass hashing: the hasher accumulates bytes during resume verification, provides an intermediate hash for comparison, and then continues accumulating bytes during streaming. When the transfer completes, the same hasher is finalized to produce the full file hash for `FileHash`.

### Download Resume

1. Server sends `FileStart { path, size }` (no hash)
2. Client checks local `.part` file (or completed file)
3. Client hashes its local data into a `StreamingHasher`, sends `FileHashing` keepalives during hashing
4. Client responds with `FileStartResponse { size: N, sha256: partial_hash }` (clone-and-finalize)
5. Server hashes first N bytes of its file into a `StreamingHasher`, sends `FileHashing` keepalives
6. Server compares via `partial_hash()`:
   - Hash match → resume from offset N. Hasher retains 0..N state for continued use
   - Hash mismatch → send `TransferComplete { success: false, error_kind: "hash_mismatch" }`
   - `size: 0` → send entire file (fresh hasher)
7. Server streams `FileData` from offset, feeding bytes to hasher via `HashingReader`
8. Server sends `FileHash { sha256: hasher.finalize() }` — full file hash (0..end)
9. Client compares its independently computed hash against server's `FileHash`

### Upload Resume

1. Client sends `FileStart { path, size }` (no hash)
2. Server checks local `.part` file (or completed file)
3. Server hashes its local data into a `StreamingHasher`, sends `FileHashing` keepalives during hashing
4. Server responds with `FileStartResponse { size: N, sha256: partial_hash }` (clone-and-finalize)
5. Client hashes first N bytes of its file into a `StreamingHasher`, sends `FileHashing` keepalives
6. Client compares via `partial_hash()`:
   - Hash match → resume from offset N. Hasher retains 0..N state for continued use
   - Hash mismatch → reset to offset 0 (send full file). Server detects concurrent upload conflict and rejects
   - `size: 0` → send entire file (fresh hasher)
7. Client streams `FileData` from offset, feeding bytes to hasher
8. Client sends `FileHash { sha256: hasher.finalize() }` — full file hash (0..end)
9. Server compares its independently computed hash against client's `FileHash`. Mismatch → delete `.part`, return error

### Partial Files

- Downloads use `.part` suffix until complete
- Uploads use `.part` suffix on server until verified
- After successful SHA-256 verification via `FileHash`, `.part` is renamed to final name

## Error Kinds

| Value                 | Description                       |
| --------------------- | --------------------------------- |
| `not_found`           | Path doesn't exist                |
| `permission`          | Permission denied                 |
| `invalid`             | Invalid input (malformed path)    |
| `unsupported_version` | Protocol version not supported    |
| `disk_full`           | Disk full                         |
| `hash_mismatch`       | SHA-256 verification failed       |
| `io_error`            | File I/O error                    |
| `protocol_error`      | Invalid/unexpected data           |
| `exists`              | File already exists (upload only) |
| `conflict`            | Source or target path busy        |

## Timeouts

| Context                 | Timeout    | Description                                                  |
| ----------------------- | ---------- | ------------------------------------------------------------ |
| Connection              | 30 seconds | TLS handshake must complete                                  |
| Download directory scan | 50 seconds | Server-side scan before `FileDownloadResponse` must complete |
| Download response wait  | 60 seconds | Client wait for initial `FileDownloadResponse`               |
| Idle                    | 30 seconds | Time waiting for first byte of any other frame               |
| Frame                   | 60 seconds | Frame must complete within 60s of first byte                 |
| FileData progress       | 60 seconds | Must receive some bytes within 60s                           |

**Note:** Unlike port 7500, port 7501 does not allow indefinite idle connections.

## Permissions

| Permission             | Required For                              |
| ---------------------- | ----------------------------------------- |
| `file_download`        | Downloading files                         |
| `file_upload`          | Uploading files to upload/dropbox folders |
| `file_upload_anywhere` | Uploading files to any directory          |
| `file_root`            | Using `root: true` for file root access   |

### Upload Destination Requirements

Uploads are allowed to:

- `[NEXUS-UL]` folders (upload folders) — requires `file_upload` or `file_upload_anywhere`
- `[NEXUS-DB]` folders (dropbox folders) — requires `file_upload` or `file_upload_anywhere`
- Any other directory — requires `file_upload_anywhere`

The server creates parent directories automatically if they don't exist.

## Port 7501 Authentication

The login flow on port 7501 is identical to port 7500, but `LoginResponse` only includes:

| Field     | Type    | Description               |
| --------- | ------- | ------------------------- |
| `success` | boolean | Whether login succeeded   |
| `error`   | string  | Error message (if failed) |

No `session_id`, `permissions`, `server_info`, or `chat_info` is returned on the transfer port.

When the transfer is registered, the server resolves the current account identity
by the stable account id from login. Regular accounts use the current username as
both `username` and `nickname`; shared accounts use the current account username
but keep the nickname supplied at transfer login.

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

- `FileStart` sent with `size: 0`
- `FileStartResponse` sent as normal
- No `FileData` message (nothing to transfer)
- `FileHash` sent with SHA-256 of empty input
- Proceed to next file

### No Overwrite

If a file already exists with different content:

- Upload fails with `error_kind: "exists"`
- Admin must delete existing file for replacement

## Notes

- Transfer port is communicated in `LoginResponse.server_info.transfer_port` (always present)
- SHA-256 is computed inline during streaming (single-pass, no post-transfer re-read)
- Both sides independently maintain a `StreamingHasher` — the sender hashes while reading from disk, the receiver hashes while writing to disk
- Hardware-accelerated SHA-256 when available (SHA-NI on x86_64, crypto extensions on ARM64)
- Large files use streaming (64KB buffers)
- Symlinks are followed transparently
- Directories are downloaded recursively

## Next Step

- Manage server and users with [admin commands](09-admin.md)
- Handle [errors](16-errors.md)
