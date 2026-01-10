# Files

File browsing provides access to the server's file area with support for directories, uploads, and various file operations.

## Flow

### Listing Files

```
Client                                        Server
   │                                             │
   │  FileList { path, root, show_hidden }       │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileListResponse { entries }        │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Getting File Information

```
Client                                        Server
   │                                             │
   │  FileInfo { path, root }                    │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileInfoResponse { info }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Creating a Directory

```
Client                                        Server
   │                                             │
   │  FileCreateDir { path, name, root }         │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileCreateDirResponse { path }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Renaming a File/Directory

```
Client                                        Server
   │                                             │
   │  FileRename { path, new_name, root }        │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileRenameResponse { success }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Moving a File/Directory

```
Client                                        Server
   │                                             │
   │  FileMove { source, dest, ... }             │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileMoveResponse { success, ... }   │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Copying a File/Directory

```
Client                                        Server
   │                                             │
   │  FileCopy { source, dest, ... }             │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileCopyResponse { success, ... }   │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Searching Files

```
Client                          Server
  |                               |
  |-- FileSearch ---------------->|
  |                               | Search index
  |<-- FileSearchResponse --------|
  |                               |
```

### Reindexing Files (Admin)

```
Client                          Server
  |                               |
  |-- FileReindex --------------->|
  |                               | Trigger rebuild
  |<-- FileReindexResponse -------|
  |                               |
```

### Deleting a File/Directory

```
Client                                        Server
   │                                             │
   │  FileDelete { path, root }                  │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         FileDeleteResponse { success }      │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

## Messages

### FileList (Client → Server)

Request directory contents.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Directory path (empty or `/` for root) |
| `root` | boolean | No | If true, path is relative to file root (default: false) |
| `show_hidden` | boolean | No | If true, include dotfiles (default: false) |

**List user's root:**

```json
{
  "path": ""
}
```

**List subdirectory:**

```json
{
  "path": "/Documents"
}
```

**With options:**

```json
{
  "path": "/",
  "root": true,
  "show_hidden": true
}
```

**Full frame:**

```
NX|8|FileList|a1b2c3d4e5f6|25|{"path":"/Documents"}
```

### FileListResponse (Server → Client)

Response containing directory entries.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `path` | string | If success | Resolved directory path |
| `entries` | array | If success | Array of `FileEntry` objects |
| `can_upload` | boolean | If success | Whether uploads are allowed in this directory |

**Success example:**

```json
{
  "success": true,
  "path": "/Documents",
  "entries": [
    {
      "name": "Reports",
      "size": 0,
      "modified": 1703001234,
      "dir_type": "default",
      "can_upload": false
    },
    {
      "name": "Uploads [NEXUS-UL]",
      "size": 0,
      "modified": 1703002000,
      "dir_type": "upload",
      "can_upload": true
    },
    {
      "name": "readme.txt",
      "size": 1024,
      "modified": 1703003000,
      "can_upload": false
    }
  ],
  "can_upload": false
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Directory not found"
}
```

### FileInfo (Client → Server)

Request detailed information about a file or directory.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Path to the file or directory |
| `root` | boolean | No | If true, path is relative to file root (default: false) |

**Example:**

```json
{
  "path": "/Documents/readme.txt"
}
```

### FileInfoResponse (Server → Client)

Response containing detailed file information.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `info` | object | If success | `FileInfoDetails` object |

**File example:**

```json
{
  "success": true,
  "info": {
    "name": "readme.txt",
    "size": 1024,
    "created": 1702900000,
    "modified": 1703003000,
    "is_directory": false,
    "is_symlink": false,
    "mime_type": "text/plain",
    "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }
}
```

**Directory example:**

```json
{
  "success": true,
  "info": {
    "name": "Documents",
    "size": 0,
    "created": 1702900000,
    "modified": 1703001234,
    "is_directory": true,
    "is_symlink": false,
    "item_count": 15
  }
}
```

**Symlink example:**

```json
{
  "success": true,
  "info": {
    "name": "link-to-docs",
    "size": 0,
    "modified": 1703001234,
    "is_directory": true,
    "is_symlink": true,
    "item_count": 15
  }
}
```

### FileCreateDir (Client → Server)

Create a new directory.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Parent directory path |
| `name` | string | Yes | Name of the new directory |
| `root` | boolean | No | If true, path is relative to file root (default: false) |

**Example:**

```json
{
  "path": "/Uploads",
  "name": "My Folder"
}
```

### FileCreateDirResponse (Server → Client)

Response after creating a directory.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether creation succeeded |
| `error` | string | If failure | Error message |
| `path` | string | If success | Full path of the created directory |

**Success example:**

```json
{
  "success": true,
  "path": "/Uploads/My Folder"
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Directory already exists"
}
```

### FileRename (Client → Server)

Rename a file or directory.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Current path of the item |
| `new_name` | string | Yes | New name (filename only, not path) |
| `root` | boolean | No | If true, path is relative to file root (default: false) |

**Example:**

```json
{
  "path": "/Documents/old-name.txt",
  "new_name": "new-name.txt"
}
```

### FileRenameResponse (Server → Client)

Response after renaming.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether rename succeeded |
| `error` | string | If failure | Error message |

**Success example:**

```json
{
  "success": true
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "A file or directory with that name already exists"
}
```

### FileMove (Client → Server)

Move a file or directory to a new location.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source_path` | string | Yes | Path of the item to move |
| `destination_dir` | string | Yes | Destination directory |
| `overwrite` | boolean | No | If true, overwrite existing item (default: false) |
| `source_root` | boolean | No | If true, source path is relative to file root |
| `destination_root` | boolean | No | If true, destination path is relative to file root |

**Example:**

```json
{
  "source_path": "/Documents/file.txt",
  "destination_dir": "/Archive"
}
```

**With overwrite:**

```json
{
  "source_path": "/Documents/file.txt",
  "destination_dir": "/Archive",
  "overwrite": true
}
```

### FileMoveResponse (Server → Client)

Response after moving.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether move succeeded |
| `error` | string | If failure | Human-readable error message |
| `error_kind` | string | If failure | Machine-readable error type |

**Success example:**

```json
{
  "success": true
}
```

**Exists error (client can offer overwrite):**

```json
{
  "success": false,
  "error": "A file or directory with that name already exists at the destination",
  "error_kind": "exists"
}
```

### FileCopy (Client → Server)

Copy a file or directory to a new location.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `source_path` | string | Yes | Path of the item to copy |
| `destination_dir` | string | Yes | Destination directory |
| `overwrite` | boolean | No | If true, overwrite existing item (default: false) |
| `source_root` | boolean | No | If true, source path is relative to file root |
| `destination_root` | boolean | No | If true, destination path is relative to file root |

**Example:**

```json
{
  "source_path": "/Documents/file.txt",
  "destination_dir": "/Backup"
}
```

### FileCopyResponse (Server → Client)

Response after copying.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether copy succeeded |
| `error` | string | If failure | Human-readable error message |
| `error_kind` | string | If failure | Machine-readable error type |

**Success example:**

```json
{
  "success": true
}
```

### FileDelete (Client → Server)

Delete a file or empty directory.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `path` | string | Yes | Path to delete |
| `root` | boolean | No | If true, path is relative to file root (default: false) |

**Example:**

```json
{
  "path": "/Documents/old-file.txt"
}
```

### FileDeleteResponse (Server → Client)

Response after deletion.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether deletion succeeded |
| `error` | string | If failure | Error message |

**Success example:**

```json
{
  "success": true
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Directory is not empty"
}
```

### FileSearch (Client → Server)

Search for files and directories by name.

**Fields:**

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `query` | string | Yes | Search query (3-256 bytes, see Search Behavior) |
| `root` | bool | No | Search entire file root (default: false) |

**Example - search user's area:**
```json
{
  "type": "FileSearch",
  "query": "report"
}
```

**Example - search entire file root (admin):**
```json
{
  "type": "FileSearch",
  "query": "report",
  "root": true
}
```

### FileSearchResponse (Server → Client)

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `success` | bool | Whether search succeeded |
| `error` | string? | Error message if failed |
| `results` | array? | Search results (max 100) |

**Example - success:**
```json
{
  "type": "FileSearchResponse",
  "success": true,
  "results": [
    {
      "path": "/Documents/report.pdf",
      "name": "report.pdf",
      "size": 12345,
      "modified": 1700000000,
      "is_directory": false
    },
    {
      "path": "/Archives/old-reports",
      "name": "old-reports",
      "size": 0,
      "modified": 1699000000,
      "is_directory": true
    }
  ]
}
```

**Example - error:**
```json
{
  "type": "FileSearchResponse",
  "success": false,
  "error": "Search query is too short (min 3 characters)"
}
```

### FileReindex (Client → Server)

Trigger a file index rebuild. Requires `file_reindex` permission.

**Fields:** None (unit message)

**Example:**
```json
{
  "type": "FileReindex"
}
```

### FileReindexResponse (Server → Client)

**Fields:**

| Field | Type | Description |
|-------|------|-------------|
| `success` | bool | Whether reindex was triggered |
| `error` | string? | Error message if failed |

**Example - success:**
```json
{
  "type": "FileReindexResponse",
  "success": true
}
```

**Note:** Returns success even if a reindex is already in progress.

## Data Structures

### FileEntry

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Filesystem name (includes folder type suffix) |
| `size` | integer | File size in bytes (0 for directories) |
| `modified` | integer | Last modified time (Unix timestamp) |
| `dir_type` | string or null | Directory type (null for files, see below) |
| `can_upload` | boolean | Whether uploads are allowed here |

### FileSearchResult

Represents a single search result.

| Field | Type | Description |
|-------|------|-------------|
| `path` | string | Full path relative to user's root |
| `name` | string | Filename only |
| `size` | u64 | File size in bytes (0 for directories) |
| `modified` | i64 | Last modified time (Unix timestamp) |
| `is_directory` | bool | True if directory |

### FileInfoDetails

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | File or directory name |
| `size` | integer | Size in bytes (0 for directories) |
| `created` | integer or null | Creation timestamp (null if unavailable) |
| `modified` | integer | Last modified timestamp |
| `is_directory` | boolean | True if directory |
| `is_symlink` | boolean | True if symbolic link |
| `mime_type` | string or null | MIME type (null for directories) |
| `item_count` | integer or null | Number of items (null for files) |
| `sha256` | string or null | SHA-256 hash (null for directories) |

## Directory Types

Directories can have special types indicated by name suffixes:

| Suffix | Type | List | Download | Upload |
|--------|------|------|----------|--------|
| *(none)* | `default` | ✅ | ✅ | ❌ |
| ` [NEXUS-UL]` | `upload` | ✅ | ✅ | ✅ |
| ` [NEXUS-DB]` | `dropbox` | Admins only | Admins only | ✅ |
| ` [NEXUS-DB-user]` | `dropbox_user` | User + Admins | User + Admins | ✅ |

**Notes:**
- Space is required before the bracket
- Suffixes are case-insensitive
- Client should strip suffix for display (e.g., `Uploads [NEXUS-UL]` → "Uploads")
- Upload permission is inherited by subdirectories

## User Areas

Each user has a file area root:

1. If `{file_root}/users/{username}/` exists → user's root is that folder
2. Otherwise → user's root is `{file_root}/shared/`

Users cannot see or access other users' areas. Paths are presented as absolute from `/`.

**Example with personal area:**
- User `alice` sees `/` which maps to `{file_root}/users/alice/`
- `/Documents/file.txt` maps to `{file_root}/users/alice/Documents/file.txt`

**Example with shared area:**
- User `bob` (no personal folder) sees `/` which maps to `{file_root}/shared/`
- `/Documents/file.txt` maps to `{file_root}/shared/Documents/file.txt`

## Root Mode

When `root: true`, paths are relative to the file root instead of the user's area. This requires the `file_root` permission and is intended for admin file management.

## Permissions

| Permission | Required For |
|------------|--------------|
| `file_list` | Browse files and directories |
| `file_info` | View detailed file information |
| `file_create_dir` | Create directories (in upload folders) |
| `file_copy` | Copy files and directories |
| `file_delete` | Delete files and empty directories |
| `file_download` | Download files (see [transfers](08-transfers.md)) |
| `file_upload` | Upload files (see [transfers](08-transfers.md)) |
| `file_move` | Move files and directories |
| `file_rename` | Rename files and directories |
| `file_root` | Access entire file root (admin) |
| `file_search` | Search files by name |
| `file_reindex` | Trigger file index rebuild |

Admins have all permissions automatically.

### Permission Combinations

| Operation | Base Permission | Additional for `overwrite: true` | Additional for `root: true` |
|-----------|-----------------|----------------------------------|----------------------------|
| Move | `file_move` | `file_delete` | `file_root` |
| Copy | `file_copy` | `file_delete` | `file_root` |
| Search | `file_search` | - | `file_root` |

## Path Validation

| Rule | Description |
|------|-------------|
| Max length | 4096 characters |
| No `..` | Parent directory references forbidden |
| No null bytes | `\0` not allowed |
| No control chars | ASCII control characters forbidden |
| Within area | Must stay within user's file area |

## Error Kinds

The `error_kind` field in move/copy responses allows programmatic handling:

| Value | Description | Client Action |
|-------|-------------|---------------|
| `exists` | Destination already exists | Offer overwrite option |
| `not_found` | Source doesn't exist | Show error, clear clipboard |
| `permission` | Permission denied | Show error |
| `invalid_path` | Invalid path format | Show error |

## Error Handling

### Common File Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Permission denied | Missing required permission | Stays connected |
| Path not found | Invalid path | Stays connected |
| Directory not found | Parent directory doesn't exist | Stays connected |
| Directory is not empty | Delete on non-empty directory | Stays connected |
| File or directory already exists | Name conflict | Stays connected |
| Cannot move/copy into itself | Circular operation | Stays connected |
| Invalid path | Path validation failed | Stays connected |

## Symlink Handling

- Symlinks are trusted (created by admin only, not via BBS protocol)
- Operations on symlinks affect the link, not the target
- Directory listings follow symlinks transparently
- `is_symlink` field indicates when an entry is a symlink

## File Search

### Index

The server maintains a CSV index of all files for fast searching:

| Aspect | Value |
|--------|-------|
| Location | `~/.local/share/nexusd/files.idx` |
| Format | CSV: `path,name,size,modified,is_directory` |
| Rebuild | On startup, on dirty flag, via `/reindex` command |

### Search Behavior

| Setting | Value |
|---------|-------|
| Max results | 100 |
| Min query length | 3 bytes total (after trimming) |
| Max query length | 256 bytes |
| Case sensitivity | Case-insensitive |
| Search type | Literal (regex chars escaped) |

**Search modes:**

- **AND mode**: If ANY term is 3+ bytes → include all 2+ byte terms, AND logic
- **Literal mode**: If ALL terms are < 3 bytes → treat entire query as literal phrase

Note: Length is measured in bytes, not characters. ASCII characters are 1 byte each,
but Unicode characters vary (e.g., CJK characters are typically 3 bytes each).

**Examples:**

| Query | Mode | Terms Used |
|-------|------|------------|
| `test file` | AND | "test", "file" |
| `mr carter` | AND | "mr", "carter" |
| `a test b` | AND | "test" only (single-char filtered) |
| `mr dj` | Literal | "mr dj" (entire phrase) |
| `a b c` | Literal | "a b c" (entire phrase) |
| `ab` | Invalid | Too short (< 3 bytes) |

### Reindex Triggers

| Event | Action |
|-------|--------|
| Server start | Background reindex |
| File upload | Mark dirty |
| File delete | Mark dirty |
| File rename/move/copy | Mark dirty |
| Timer interval | Rebuild if dirty |
| `/reindex` command | Force rebuild |

The reindex interval is configurable via `ServerInfoUpdate` (default: 5 minutes, 0 to disable).

## Notes

- File operations use the main BBS port (7500)
- Actual file transfers use port 7501 (see [transfers](08-transfers.md))
- Hidden files (dotfiles) are excluded by default
- Only empty directories can be deleted
- Directories are copied recursively
- Move uses `rename()` (atomic, fails across filesystems)
- Path `/` and empty string both refer to the user's root

## Next Step

- Transfer files with [downloads and uploads](08-transfers.md)
- Manage server with [admin commands](09-admin.md)