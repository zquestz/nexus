# Files

This guide covers browsing, searching, downloading, and uploading files on Nexus BBS servers.

## File Browser

Access the file browser by clicking the **Files** icon in the toolbar (folder icon). The browser shows your file area on the server.

### Interface Overview

The file browser includes:

- **Toolbar** — Navigation and action buttons
- **Breadcrumb bar** — Shows current path, click segments to navigate
- **File list** — Files and folders in the current directory
- **Tabs** — Multiple browser tabs (like a web browser)

### Navigation

| Action | How |
|--------|-----|
| Open folder | Double-click or click folder name |
| Go up one level | Click **↑** button or breadcrumb segment |
| Go to root | Click **Home** button |
| Refresh | Click **Refresh** button |

### Toolbar Buttons

| Button | Description |
|--------|-------------|
| **Home** | Go to your file area root |
| **Root** | Toggle between your area and server root (admin) |
| **Refresh** | Reload current directory |
| **Eye** | Show/hide hidden files (dotfiles) |
| **Download** | Download entire current directory |
| **Upload** | Upload files to current directory |
| **New Folder** | Create a new directory |
| **Paste** | Paste cut/copied items |
| **Up** | Go to parent directory |

Some buttons may be disabled based on your permissions or the current folder type.

### File List Columns

| Column | Description |
|--------|-------------|
| **Name** | File or folder name (click to sort) |
| **Size** | File size (folders show "—") |
| **Modified** | Last modification date |

Click a column header to sort. Click again to reverse the sort order.

**Note:** When sorting by Name, directories always appear first. When sorting by Size or Modified, directories and files are mixed together.

## Searching Files

If you have the `file_search` permission, a search bar appears below the toolbar.

### Basic Search

1. Type your search query in the search box
2. Press **Enter** or click the **🔍** button
3. Results appear in a table showing matching files and folders

### Search Requirements

- Minimum 3 characters (after trimming whitespace)
- Maximum 256 characters
- No control characters allowed

### Search Results

Results display in a 4-column table:

| Column | Description |
|--------|-------------|
| **Name** | File or folder name with icon |
| **Path** | Parent directory location |
| **Size** | File size (folders show "—") |
| **Modified** | Last modification date |

Click any column header to sort. When sorting by Name, directories appear first.

### Opening Results

- **Left-click** a result to open it in a new tab
  - Files: Opens the parent directory with the file visible
  - Folders: Opens the folder itself
- **Right-click** for context menu:
  - **Download** — Download the file or folder
  - **Info** — View detailed information
  - **Open** — Same as left-click

### Search Scope

- By default, search covers your file area (personal or shared)
- Admins with `file_root` permission can toggle **Root** mode to search the entire server

### Exiting Search Mode

- Clear the search box and press **Enter**
- Click the **Home** button
- Press **Escape**

Search results are preserved per-tab, so you can switch tabs and return to your search.

### Tabs

Open multiple browser tabs to work with different locations:

- Click **+** to open a new tab
- Click a tab to switch to it
- Click **×** on a tab to close it
- `Ctrl+Tab` / `Cmd+Tab` — Next tab
- `Ctrl+Shift+Tab` / `Cmd+Shift+Tab` — Previous tab

Each tab maintains its own location and history.

## File Areas

Servers organize files into areas. Your view depends on your account:

### Personal Area

If the admin created a personal folder for your username, you see only your files. The path `/` represents your personal area root.

### Shared Area

If you don't have a personal folder, you see the server's shared files. All users without personal folders share this area.

### Root Mode (Admin)

Admins with `file_root` permission can toggle **Root** mode to see the entire file structure, including all user areas and shared files.

## Folder Types

Servers use special folder suffixes to control permissions:

| Folder Appearance | Type | You Can |
|-------------------|------|---------|
| Regular folder | Default | Browse, download |
| Folder with upload indicator | Upload | Browse, download, upload |
| "Drop Box" | Drop Box | Upload only (can't see contents) |

**Upload folders** — You can upload files here. Indicated visually in the file list.

**Drop boxes** — You can upload but can't see what's inside. Used for blind submissions.

**User drop boxes** — Like drop boxes, but you (and admins) can see your own uploads.

## Downloading

### Download a Single File

1. Right-click the file
2. Select **Download**

Or: Select the file and use the context menu.

### Download a Folder

1. Right-click the folder
2. Select **Download**

This downloads all files in the folder and its subfolders.

### Download Current Directory

Click the **Download** button in the toolbar to download everything in the current directory.

### Download Location

Downloads are saved to your system's Downloads folder by default. You can change this in **Settings > Files > Download Location**.

## Uploading

Uploading requires:
1. `file_upload` permission
2. Being in an upload-enabled folder

### Upload Files

1. Navigate to an upload folder
2. Click the **Upload** button in the toolbar
3. Select files in the file picker
4. Files are queued for upload

### Upload Limitations

- You can only upload to folders marked as upload folders
- The upload button is disabled in read-only folders
- Some servers may have file size limits

## File Operations

Right-click a file or folder for these options:

| Action | Description | Permission |
|--------|-------------|------------|
| **Download** | Download to your computer | `file_download` |
| **Cut** | Cut for moving | `file_move` |
| **Copy** | Copy for pasting | `file_copy` |
| **Paste** | Paste cut/copied item (folders only) | `file_move` or `file_copy` |
| **Info** | View detailed information | `file_info` |
| **Rename** | Rename the item | `file_rename` |
| **Delete** | Delete the item | `file_delete` |

### Cut, Copy, and Paste

1. Right-click a file or folder
2. Select **Cut** (to move) or **Copy**
3. Navigate to the destination folder
4. Click **Paste** in the toolbar or right-click and select **Paste**

Cut items appear dimmed until pasted. Press **Escape** to cancel a cut/copy operation.

### Rename

1. Right-click the item
2. Select **Rename**
3. Enter the new name
4. Press **Enter** or click **Rename**

### Delete

1. Right-click the item
2. Select **Delete**
3. Confirm the deletion

**Warning:** Deletions cannot be undone.

### Create Directory

1. Click the **New Folder** button in the toolbar
2. Enter a name
3. Press **Enter** or click **Create**

Creating directories in upload folders may require only `file_upload` permission, while other locations require `file_create_dir`.

### File Info

Right-click and select **Info** to view:

- File name
- Size
- Created date
- Modified date
- Whether it's a directory or symlink
- MIME type (for files)
- SHA-256 hash (for files)

## Transfers Panel

View and manage active transfers by clicking the **Transfers** icon in the toolbar.

### Transfer States

| Status | Description |
|--------|-------------|
| **Queued** | Waiting to start |
| **Connecting** | Establishing connection |
| **Transferring** | Actively transferring data |
| **Paused** | Paused by user |
| **Completed** | Successfully finished |
| **Failed** | Error occurred |

### Transfer Actions

| Button | Available When | Action |
|--------|----------------|--------|
| **Pause** | Transferring | Pause the transfer |
| **Resume** | Paused, Failed | Resume the transfer |
| **Cancel** | Any active state | Cancel and remove |
| **Open Folder** | Completed, Failed | Open download location |
| **Remove** | Completed, Failed | Remove from list |

### Resume Support

Transfers can be resumed after:
- Pausing manually
- Connection interruption
- Application restart

Partial downloads are saved with a `.part` extension until complete.

### Queue Settings

Configure transfer behavior in **Settings > Files**:

- **Queue transfers** — Limit concurrent transfers per server
- **Download limit** — Maximum simultaneous downloads (default: 2)
- **Upload limit** — Maximum simultaneous uploads (default: 2)

Set limits to 0 for unlimited concurrent transfers.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+Tab` (`Cmd+Tab` on macOS) | Next browser tab |
| `Ctrl+Shift+Tab` (`Cmd+Shift+Tab` on macOS) | Previous browser tab |
| `Escape` | Cancel cut/copy, exit search mode, close dialog |
| `Enter` | Confirm dialog, submit search |

## Permissions

Your available actions depend on server permissions:

| Permission | Allows |
|------------|--------|
| `file_list` | Browse files and directories |
| `file_download` | Download files |
| `file_upload` | Upload files (to upload folders) |
| `file_info` | View detailed file information |
| `file_create_dir` | Create directories |
| `file_rename` | Rename files and directories |
| `file_move` | Move files and directories |
| `file_copy` | Copy files and directories |
| `file_delete` | Delete files and directories |
| `file_root` | Access entire file root (admin) |
| `file_search` | Search files across your area |
| `file_reindex` | Trigger file index rebuild (admin) |

Admins automatically have all permissions.

## Troubleshooting

### "Permission denied" errors

You don't have the required permission for that action. Contact the server admin.

### Upload button is disabled

You're not in an upload-enabled folder. Navigate to a folder that allows uploads (look for the upload indicator).

### Transfer stuck at "Connecting"

- Check your internet connection
- The server's transfer port (7501) may be blocked
- Try pausing and resuming the transfer

### Transfer failed

Click **Resume** to retry. If it fails repeatedly:
- Check available disk space
- Verify the file still exists on the server
- Check your connection to the server

### Can't see a folder's contents

The folder may be a drop box. You can upload to it but can't view its contents unless you're the designated user or an admin.

## Next Steps

- [News](06-news.md) — Reading and posting news articles
- [Settings](07-settings.md) — Configure download location and transfer limits