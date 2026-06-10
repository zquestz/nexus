# Server Info

This guide covers the Server Info panel for viewing server identity and
configuration, and (for admins) managing the server's tracker registrations.

## Overview

The Server Info panel has two parts:

- An **identity block** at the top — server image, name, description,
  shareable `nexus://` URI, and certificate fingerprint.
- A **tabbed body** with two tabs:
  - **Config** — server configuration grouped into General / Chat / Files
    sections (visibility per-row by permission).
  - **Trackers** — tracker registrations the server publishes itself to
    (admin-gated; only visible with `tracker_list` permission).

## Accessing Server Info

Click the **Server Info** icon in the toolbar. You must be connected to a
server.

You can also use the `/sinfo` command (aliases: `/si`, `/serverinfo`) to
display server information in chat.

## Identity Block

The identity block sits above the tabs and is always visible:

| Field           | Description                                                                                                                                                                                                          |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Image**       | Server logo or banner (if set by the admin).                                                                                                                                                                         |
| **Name**        | Server display name.                                                                                                                                                                                                 |
| **Description** | Server description text.                                                                                                                                                                                             |
| **Public URI**  | A clickable `nexus://` URI pointing at this server. Click to copy. Uses the admin-set **Public Address** when present, else falls back to your connection address. Port shown only when it differs from the default. |
| **Fingerprint** | Server certificate fingerprint (SHA-256). Click anywhere on the value to copy the full uppercase colon-separated form to your clipboard. Wraps to multiple lines on narrow windows.                                  |

## Config Tab

The Config tab shows server configuration in four sections, each with its
own heading. A section appears only if at least one of its fields is
visible to you under your current permissions; an entirely-hidden section
disappears, heading and all. Within each section, rows are alphabetical.

### General

| Field                      | Description                                           |
| -------------------------- | ----------------------------------------------------- |
| **Log Level**              | Server logging level (None, Error, Warn, Info, Debug) |
| **Max Connections per IP** | Maximum concurrent connections per IP address         |
| **Max Transfers per IP**   | Maximum concurrent file transfers per IP address      |
| **Min Password**           | Minimum password strength required for accounts       |
| **Version**                | Server software version                               |

### Bandwidth

| Field                    | Visible To  | Description                                                              |
| ------------------------ | ----------- | ------------------------------------------------------------------------ |
| **Max Outbound (Mbps)**  | All users   | Server-wide outbound bandwidth cap. Renders `Unlimited` when set to `0`. |
| **Scheduler Chunk Size** | Admins only | Egress scheduler packet size in bytes (internal tuning knob).            |

### Chat

| Field                   | Visible To                                                      | Description                                      |
| ----------------------- | --------------------------------------------------------------- | ------------------------------------------------ |
| **Auto-Join Channels**  | Admins, or users with `chat` enabled and `chat_join` permission | Channels users automatically join on login       |
| **Chat Burst Limit**    | All users                                                       | Maximum messages in a burst before rate limiting |
| **Chat Rate Limit**     | All users                                                       | Messages per minute rate limit (0 = disabled)    |
| **Persistent Channels** | Admins only                                                     | Channels that persist even when empty            |

### Files

| Field                     | Description                                                                                                       |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| **File Reindex Interval** | File index rebuild interval in minutes (or Disabled). Visible to admins and users with `file_reindex` permission. |

## Edit Mode

The bottom of the Config tab has two buttons:

- **Edit** — admin only. Swaps the Config tab into the edit form
  described below.
- **Close** — closes the Server Info panel.

The Trackers tab has no bottom button row — its actions are in the
top-of-tab toolbar and per-row context menus (see _Tracker Management_
below).

### Editable Fields

| Field                    | Description                                                          |
| ------------------------ | -------------------------------------------------------------------- |
| **Auto-Join Channels**   | Space-separated channel names users auto-join on login               |
| **Chat Burst Limit**     | Maximum messages in a burst before rate limiting (0 = capacity of 1) |
| **Chat Rate Limit**      | Messages per minute rate limit (0 = flood protection disabled)       |
| **Description**          | Server description (0–512 characters)                                |
| **File Reindex**         | File index rebuild interval in minutes (0 to disable)                |
| **Image**                | Server logo (PNG, JPEG, WebP, SVG; max 700KB)                        |
| **Max Connections**      | Maximum connections per IP address                                   |
| **Max Outbound (Mbps)**  | Server-wide outbound bandwidth cap; `0` = unlimited                  |
| **Max Transfers**        | Maximum file transfers per IP address                                |
| **Min Password**         | Minimum password strength for accounts                               |
| **Name**                 | Server display name (1–64 characters)                                |
| **Persistent Channels**  | Space-separated channel names (e.g., `#general #support`)            |
| **Public Address**       | Hostname or IP advertised for shareable `nexus://` URIs (optional)   |
| **Scheduler Chunk Size** | Egress scheduler packet size in bytes (admin-only tuning knob)       |

The edit form groups fields under the same **General / Bandwidth / Chat / Files**
subheadings as the display view, so finding a field in one mode prepares
you for the other.

**Note:** Log Level is set by the server operator via command-line
options. Version is determined by the server software. Neither is
editable from the client.

**Public Address**: set this to the hostname (or IP) that users connecting
from outside your network should see in shareable `nexus://` URIs.
Accepts DNS hostnames, IPv4 literals, bare IPv6 literals, and
internationalized domain names (Unicode). Reject cases include URL
schemes, brackets, paths, `@`, whitespace, ports, and IPv6 zone
identifiers — the field is just a host, never a full URL. Leave empty
to fall back to whatever address each user connected to.

### Editing

1. Click **Edit** to enter edit mode.
2. Modify the desired fields.
3. Click **Save** to apply changes, or **Cancel** to discard.

Changes are broadcast to all connected users immediately.

## Tracker Management

The **Trackers** tab is visible only to users with `tracker_list`
permission. It shows the trackers this server publishes itself to, with
their live connection status, and (for admins with the right permissions)
lets you add, edit, or remove tracker registrations.

### List view

The tracker list is a sortable table with three columns:

| Column      | Notes                                                                                                                  |
| ----------- | ---------------------------------------------------------------------------------------------------------------------- |
| **Status**  | Themed indicator bullet — green = connected, yellow = fingerprint mismatch pending review, red = disconnected or error |
| **Name**    | Admin-set tracker name                                                                                                 |
| **Address** | Tracker hostname/IP. Shows `host:port` when the tracker uses a non-default port; otherwise just the host               |

Hovering the status bullet shows a tooltip with the most recent
operational error (or "Connected" / "Fingerprint mismatch — review and
accept" for the corresponding states). Sort by clicking any column
header. Default sort is **Name ascending**.

The toolbar above the table has **Add Tracker** and **Refresh** icons.
Add is disabled without `tracker_add`. Refresh re-fetches the list from
the server.

### Right-click context menu

Right-clicking a row opens a context menu:

- **Accept Fingerprint** — only present when the tracker has a pending
  Stage-1 fingerprint mismatch (yellow indicator). Gated on
  `tracker_edit`.
- **Edit** — opens the Edit Tracker subview. Gated on `tracker_edit`.
- **Remove** — opens the Remove Tracker confirm modal. Gated on
  `tracker_remove`. Styled as a destructive action.

### Add Tracker

The Add toolbar icon swaps the panel into a full-form Add Tracker
subview. Fields:

| Field           | Description                                                                                                                               |
| --------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| **Name**        | Display name for this registration. Required.                                                                                             |
| **Address**     | Tracker hostname or IP. Required. Same accept/reject rules as Public Address.                                                             |
| **Port**        | Tracker TCP port. Defaults to the well-known tracker port.                                                                                |
| **Fingerprint** | Optional SHA-256 fingerprint pin. Empty allows TOFU (trust on first use); subsequent mismatches will pause registration until you review. |
| **Password**    | Optional shared registration password (if the tracker requires one).                                                                      |
| **Enabled**     | Whether to start the registration loop after saving.                                                                                      |

Click **Add** to submit, or **Cancel** to return to the list. Validation
errors render in a banner above the form. The form is disabled while
the request is in flight.

### Edit Tracker

Right-click → Edit re-fetches the tracker's current state from the
server (so you see the latest operational error and any pending
fingerprint observation), then opens an Edit Tracker subview prefilled
with current values.

The form is identical to Add Tracker. If the registration recently
failed, the form shows the most recent error in an informational banner
above the fields, so you have operational context while editing.

### Accept Fingerprint dialog

Trackers do TOFU on first connection, then verify the tracker's
fingerprint on each subsequent connection. There are two failure stages:

- **Stage 1** — TLS-observed fingerprint differs from the pin you've
  saved. The registration pauses; the row turns yellow; the context menu
  offers **Accept Fingerprint**. Choosing Accept opens a side-by-side
  comparison dialog (your pinned fingerprint vs. the new one observed).
  Only accept after confirming the new fingerprint via a trusted
  out-of-band channel — this is the same security model as the BBS
  server's certificate accept dialog.
- **Stage 2** — the tracker's TLS-observed fingerprint differs from the
  fingerprint the tracker self-reports in its handshake response. This
  is treated as active interception; the row turns red, **no Accept
  affordance is offered**, and there is no in-product recovery path.
  Stage 2 doesn't compare against the saved fingerprint pin, so
  editing or clearing the pin won't help — the mismatch is structural
  to the connection. The fix has to happen at the network layer
  (resolve the MITM / compromised intermediate / DNS poisoning).
  Disable or remove the registration until the network situation is
  resolved; the server's operator log records the TLS-observed
  fingerprint for forensic investigation.

### Remove Tracker

Right-click → Remove opens a confirmation modal. Confirm to remove the
registration; cancel to dismiss. The modal stays open until the server
responds — errors render in the modal so you can retry or cancel.
Removal does _not_ delete the tracker — it only removes this server's
registration with it.

## Permissions

| Permission       | Gates                                                     |
| ---------------- | --------------------------------------------------------- |
| `tracker_list`   | Trackers tab visibility, Refresh button                   |
| `tracker_add`    | Add Tracker toolbar button                                |
| `tracker_edit`   | Edit + Accept Fingerprint context menu items, Edit submit |
| `tracker_remove` | Remove context menu item, Remove confirm submit           |

Permission changes apply live — losing `tracker_list` mid-session hides
the Trackers tab on the next render.

## Keyboard Shortcuts

| Context                           | Shortcut | Action                                                                         |
| --------------------------------- | -------- | ------------------------------------------------------------------------------ |
| Anywhere on the panel             | `Escape` | Close panel from list/display mode; cancel back to list from any subview/modal |
| Config tab, Edit mode             | `Enter`  | Save changes                                                                   |
| Tracker Add subview               | `Enter`  | Submit (or show validation if incomplete)                                      |
| Tracker Edit subview              | `Enter`  | Submit (or show validation if incomplete)                                      |
| Tracker Remove confirm modal      | `Enter`  | Confirm removal                                                                |
| Tracker Accept Fingerprint dialog | `Enter`  | Accept the new fingerprint                                                     |

## Troubleshooting

### Some Config sections are missing

A section appears only when at least one row in it is visible to you. For
example, the Files section requires `file_reindex` permission or admin
status; if you have neither, the entire Files heading disappears.

### Trackers tab is missing

The Trackers tab requires the `tracker_list` permission. Ask an admin to
grant it (or check whether you've been moved out of a group that had it).

### Can't edit server info

Only administrators can edit server configuration. The Edit button at
the bottom of the Config tab is hidden for non-admin users.

### A tracker row stays yellow

The tracker has reported a Stage-1 fingerprint mismatch. The
registration is paused until an admin reviews the new fingerprint and
chooses Accept Fingerprint (or edits/removes the row). Verify the new
fingerprint via a trusted out-of-band channel before accepting.

### A tracker row is red and Accept is missing

Either the registration is disconnected with a transient error (look at
the status tooltip) or it's hit a Stage-2 fingerprint mismatch (active
interception signal). Stage-2 mismatches have no in-product fix:
editing the fingerprint or clearing it won't recover the registration,
because Stage 2 compares the live TLS cert against the tracker's
self-reported fingerprint in its handshake response — not against
anything in your tracker row. Investigate the network path (your DNS,
upstream link, anything that could intercept TLS to that hostname).
Until the network issue is resolved, disable or remove the
registration. The server's operator log captures the TLS-observed
fingerprint for forensics.

## Next Steps

- [Settings](11-settings.md) — Configure client preferences
- [Commands](04-commands.md) — Use `/sinfo` to view server info in chat
