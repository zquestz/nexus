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

| Platform | Default Path |
|----------|--------------|
| Linux | `~/.local/share/nexusd/files/` |
| macOS | `~/Library/Application Support/nexusd/files/` |
| Windows | `%APPDATA%\nexusd\files\` |

Override with the `--file-root` option:

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

## Creating Personal Areas

Personal folders are created manually by the admin:

```bash
mkdir -p ~/.local/share/nexusd/files/users/alice
mkdir -p ~/.local/share/nexusd/files/users/bob
```

Once created, those users will see their personal folder instead of the shared area.

## Folder Types

Control folder behavior using name suffixes:

| Suffix | Type | Users Can |
|--------|------|-----------|
| *(none)* | Default | Browse, download |
| ` [NEXUS-UL]` | Upload | Browse, download, upload |
| ` [NEXUS-DB]` | Drop Box | Upload only (blind) |
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

### Client Display

Clients strip the suffix for display:
- `Uploads [NEXUS-UL]` appears as "Uploads"
- `For Alice [NEXUS-DB-alice]` appears as "For Alice"

## Drop Box Visibility

| User | `[NEXUS-DB]` | `[NEXUS-DB-alice]` | `[NEXUS-DB-bob]` |
|------|--------------|---------------------|-------------------|
| Alice | Upload only | Full access | Upload only |
| Bob | Upload only | Upload only | Full access |
| Admin | Full access | Full access | Full access |
| Others | Upload only | Upload only | Upload only |

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

| Permission | Allows |
|------------|--------|
| `file_list` | Browse directories |
| `file_download` | Download files |
| `file_upload` | Upload files (to upload folders) |
| `file_info` | View file details |
| `file_create_dir` | Create directories |
| `file_rename` | Rename files/directories |
| `file_move` | Move files/directories |
| `file_copy` | Copy files/directories |
| `file_delete` | Delete files/directories |
| `file_root` | Access entire file root (admin) |

Admins have all permissions automatically.

## Root Mode

Users with `file_root` permission (typically admins) can toggle "Root Mode" to see the entire file structure, including all user areas.

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

### Uploads not working

1. Verify the folder has the `[NEXUS-UL]` suffix
2. Check the user has `file_upload` permission
3. Verify disk space is available

### Drop box contents not visible

Drop boxes are only visible to:
- Admins (for `[NEXUS-DB]`)
- The named user and admins (for `[NEXUS-DB-username]`)

Access contents via the filesystem or as an admin in Root Mode.

## Next Steps

- [User Management](05-user-management.md) — Configure user permissions
- [Troubleshooting](06-troubleshooting.md) — Common issues and solutions