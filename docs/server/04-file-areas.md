# File Areas

This guide covers setting up and managing file areas on your Nexus BBS server.

## Overview

File areas provide shared storage for your BBS users. The server organizes files into:

- **Shared area** — Default location for all users without personal folders
- **Personal areas** — Per-user folders created by the admin

## Directory Structure

The file area root contains:

```
files/
├── shared/           # Default area for users without personal folders
└── users/
    ├── alice/        # Alice's personal area
    ├── bob/          # Bob's personal area
    └── guest/        # Shared guest account area
```

## Default Location

The file area lives at `<data-dir>/files/` by default. With the data
directory at its platform default, this resolves to:

| Platform | Default Path                                  |
| -------- | --------------------------------------------- |
| Linux    | `~/.local/share/nexusd/files/`                |
| macOS    | `~/Library/Application Support/nexusd/files/` |
| Windows  | `%APPDATA%\nexusd\files\`                     |

Setting `--data-dir` moves the default to `<data-dir>/files/`. Override
with `--file-root` to place the file area outside the data directory
entirely (e.g., bulk storage on a different volume):

```bash
nexusd --file-root /srv/nexus/files
```

## Automatic Setup

On first run, the server creates:

- `files/` — Root directory
- `files/shared/` — Default shared area
- `files/users/` — Container for personal areas

## User Area Resolution

When a user browses files:

1. Server checks if `files/users/{username}/` exists
2. If yes → user sees their personal area
3. If no → user sees `files/shared/`

Users see their area as `/` — they don't know which physical location they're in.

### Username Renames

When an account username changes, the server also handles that account's
personal area:

- If `files/users/{old_username}/` exists and `files/users/{new_username}/`
  does not, the directory is renamed to the new username.
- If both old and new personal-area directories exist as distinct filesystem
  entries, the account rename fails. The server never merges or overwrites
  personal areas.
- If the old personal-area directory does not exist, the server leaves the
  filesystem alone. This allows admins to pre-create `files/users/{new_username}/`
  before renaming an account.
- If the old or new personal area is busy because a file operation or transfer
  is active there, the account rename fails immediately instead of waiting.
- This applies to both regular accounts and shared accounts.

User drop boxes (`[NEXUS-DB-username]`) are an admin-managed naming convention.
They are not renamed automatically when an account username changes; update those
folder names manually if you want the ownership suffix to follow the new username.

## Creating Personal Areas

Personal folders are created manually by the admin:

```bash
mkdir -p ~/.local/share/nexusd/files/users/alice
mkdir -p ~/.local/share/nexusd/files/users/bob
```

Once created, those users will see their personal folder instead of the shared area.

## Folder Types

Control folder behavior using name suffixes:

| Suffix                 | Type          | Users Can                     |
| ---------------------- | ------------- | ----------------------------- |
| _(none)_               | Default       | Browse, download              |
| ` [NEXUS-UL]`          | Upload        | Browse, download, upload      |
| ` [NEXUS-DB]`          | Drop Box      | Upload only (blind)           |
| ` [NEXUS-DB-username]` | User Drop Box | Upload; named user can browse |

**Important:** A space is required before the bracket.

### Examples

```bash
# Regular folder (read-only)
mkdir "Software"

# Upload folder (anyone can upload)
mkdir "Community Uploads [NEXUS-UL]"

# Drop box (admins see contents, users upload blindly)
mkdir "Submissions [NEXUS-DB]"

# User drop box (alice and admins see contents)
mkdir "For Alice [NEXUS-DB-alice]"
```

### Suffix Rules

- Space required before bracket: `Uploads [NEXUS-UL]` ✓
- Case-insensitive: `[NEXUS-UL]` = `[nexus-ul]`
- Must be at end of folder name
- Subfolders inherit upload permission from parent
- User drop-box owner suffixes are not automatically changed by account
  username renames

### Client Display

Clients strip the suffix for display:

- `Uploads [NEXUS-UL]` appears as "Uploads"
- `For Alice [NEXUS-DB-alice]` appears as "For Alice"

## Drop Box Visibility

| User   | `[NEXUS-DB]` | `[NEXUS-DB-alice]` | `[NEXUS-DB-bob]` |
| ------ | ------------ | ------------------ | ---------------- |
| Alice  | Upload only  | Full access        | Upload only      |
| Bob    | Upload only  | Upload only        | Full access      |
| Admin  | Full access  | Full access        | Full access      |
| Others | Upload only  | Upload only        | Upload only      |

### Owner Cleanup of `[NEXUS-DB-username]`

The named user of a `[NEXUS-DB-username]` folder can delete and rename files (and empty subdirectories) _inside_ their drop box even when they don't hold the `file_delete` or `file_rename` permissions. This lets a user clean up and label what someone dropped for them — for example, renaming an `IMG_2391.jpg` upload to `beach-trip.jpg` before grabbing it — without needing admin rights.

The bypass is scoped strictly to drop-box contents:

- The owner **cannot** delete or rename the drop-box folder itself — that remains an admin operation.
- The bypass does not extend to other folders (no global delete or rename).
- Other users (non-owners) still need the corresponding global permission.
- The bypass covers delete and rename only. Move and copy are excluded — they can target paths outside the drop box.

## Example File Structure

```
files/
├── shared/
│   ├── Software/                       # Read-only downloads
│   │   ├── Games/
│   │   └── Utilities/
│   ├── Documents/                      # Read-only
│   ├── Community Uploads [NEXUS-UL]/   # Anyone can upload
│   ├── Submissions [NEXUS-DB]/         # Blind uploads for admins
│   └── For Alice [NEXUS-DB-alice]/     # Others drop files for Alice here
└── users/
    └── bob/
        ├── My Files/                   # Bob's read-only files
        └── Incoming [NEXUS-UL]/        # Bob's upload folder
```

## Shared Accounts

Shared account users (including guests) share a folder based on the account username:

- All users logged into "guest" share `files/users/guest/`
- If `files/users/guest/` doesn't exist, they use `files/shared/`

## Symlinks

Symlinks are allowed and trusted. Use them to link external storage:

```bash
# Link external media storage
ln -s /mnt/nas/videos ~/.local/share/nexusd/files/shared/Videos
```

Symlinks can point outside the file root. Only admins can create symlinks (via filesystem access, not the BBS protocol).

## Permissions

File operations require specific permissions:

| Permission             | Allows                                 |
| ---------------------- | -------------------------------------- |
| `file_list`            | Browse directories                     |
| `file_download`        | Download files                         |
| `file_upload`          | Upload files to upload/dropbox folders |
| `file_upload_anywhere` | Upload files to any directory          |
| `file_info`            | View file details                      |
| `file_create_dir`      | Create directories                     |
| `file_rename`          | Rename files/directories               |
| `file_move`            | Move files/directories                 |
| `file_copy`            | Copy files/directories                 |
| `file_delete`          | Delete files/directories               |
| `file_root`            | Access entire file root (admin)        |
| `file_search`          | Search files by name                   |
| `file_reindex`         | Trigger index rebuild (admin)          |

Admins have all permissions automatically.

## Root Mode

Users with `file_root` permission (typically admins) can toggle "Root Mode" to see the entire file structure, including all user areas.

Root-mode operations against a specific personal area are serialized against that
account's username changes. Whole-tree operations such as listing, searching, or
downloading `/` or `/users` do not lock every personal area. If an account rename
happens at the same time, the root-mode operation may see a partial snapshot or
return an error for a path that moved.

## Admin Responsibilities

As a server admin, you're responsible for:

1. **Creating user folders** — `mkdir users/username`
2. **Setting up folder types** — Name folders with appropriate suffixes
3. **Managing disk space** — Monitor and clean up as needed
4. **Retrieving drop box contents** — Check drop boxes via filesystem
5. **Cleaning orphaned folders** — User folders remain after account deletion
6. **Cleaning stale uploads** — Remove old `.part` files from interrupted transfers

### Cleanup Commands

```bash
# Find old partial uploads (older than 7 days)
find /path/to/files -name "*.part" -mtime +7

# Remove them
find /path/to/files -name "*.part" -mtime +7 -delete

# Find large files
find /path/to/files -size +100M -type f
```

## File Search Index

The server maintains a search index for fast file lookups.

### Index Location

The index file is `<data-dir>/files.idx`. With the data directory at its
platform default, this resolves to:

| Platform | Default Path                                     |
| -------- | ------------------------------------------------ |
| Linux    | `~/.local/share/nexusd/files.idx`                |
| macOS    | `~/Library/Application Support/nexusd/files.idx` |
| Windows  | `%APPDATA%\nexusd\files.idx`                     |

### Automatic Rebuilds

The index rebuilds automatically when:

- Server starts (background rebuild)
- Files are uploaded, deleted, renamed, moved, or copied
- The reindex timer fires (if dirty)

### Configuration

Configure the reindex interval via the Server Info panel (admin only) or programmatically:

| Setting                 | Default   | Description                                                                                  |
| ----------------------- | --------- | -------------------------------------------------------------------------------------------- |
| `file_reindex_interval` | 5 minutes | How often to check for changes and rebuild if dirty. Set to 0 to disable automatic rebuilds. |

### Manual Rebuild

Admins with `file_reindex` permission can force a rebuild:

- Use the `/reindex` command in chat
- Useful after adding files directly to the filesystem

### Index Format

The index is a CSV file containing:

- File path (relative to file root)
- File name
- Size in bytes
- Last modified timestamp
- Directory flag

### Notes

- The index file has restrictive permissions (0600 on Unix)
- If the index is corrupted, it's automatically deleted and rebuilt
- Files added directly to the filesystem won't appear until the next reindex

## Security Notes

- Users cannot traverse outside their area (no `..` attacks)
- Path components are validated before filesystem access
- File permissions are enforced regardless of filesystem permissions
- Symlinks are trusted — only create them intentionally

## Troubleshooting

### User sees shared area instead of personal folder

Verify the folder exists and matches the username exactly (case-sensitive on most systems):

```bash
ls -la ~/.local/share/nexusd/files/users/
```

If the user was renamed, verify that the rename either migrated the old personal
folder or that a new folder was intentionally pre-created under the new username.
Account renames fail rather than overwrite a distinct existing target folder.

### Uploads not working

1. Verify the folder has the `[NEXUS-UL]` suffix, or the user has `file_upload_anywhere`
2. Check the user has `file_upload` or `file_upload_anywhere` permission
3. Verify disk space is available

### Drop box contents not visible

Drop boxes are only visible to:

- Admins (for `[NEXUS-DB]`)
- The named user and admins (for `[NEXUS-DB-username]`)

Access contents via the filesystem or as an admin in Root Mode.

## Next Steps

- [User Management](05-user-management.md) — Configure user permissions
- [Troubleshooting](06-troubleshooting.md) — Common issues and solutions
