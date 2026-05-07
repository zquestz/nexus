# Trackers

This guide covers the **Trackers** panel — a discovery service for finding
Nexus servers across the network. Trackers maintain a list of servers
that have registered with them; you query a tracker to see which servers
are online and click through to connect.

A tracker does not relay BBS traffic, mediate connections, or proxy your
account. It only maintains the list. When you pick a server from the
list, Nexus connects to that server directly.

## Overview

Use the Trackers panel when you want to discover new Nexus servers.
Bookmarks are great for places you already know about; trackers are how
you find new ones.

You configure one or more trackers (a tracker's hostname, port, and
optional password), and the panel queries them for their current server
list. Each tracker is independent — you choose which trackers to trust,
and Nexus does not aggregate across them.

## Opening the Panel

Click the **globe** icon in the top toolbar (right-hand group, between
Transfers and About). The Trackers panel is global — it is always
available, regardless of whether you are connected to any server.

The first time you open it, the panel will be empty. Add your first
tracker via the **`+`** button in the panel header.

## Tracker Management

The panel header reads **Trackers**, with a **`+`** button on the right
to add a new tracker. The toolbar row below has a tracker dropdown,
Edit / Remove / Refresh buttons, and a status text on the right.

### Adding a Tracker

1. Click the **`+`** button in the panel header.
2. Fill in:
   - **Name** — a display name. Required; must be unique among your
     configured trackers.
   - **Address** — the tracker's hostname or IP. Required.
   - **Port** — defaults to `7510` (the standard tracker port).
   - **Password** — optional, only if the tracker operator gave you a
     listing password.
   - **Fingerprint** — optional; usually left blank to let the
     trust-on-first-use (TOFU) check handle pinning automatically.
3. Click **Save**.

After saving, Nexus auto-fetches the tracker's server list. The panel
returns to the listing view.

### Editing a Tracker

1. Select the tracker from the dropdown.
2. Click the **pencil** icon.
3. Edit fields as needed.
4. Click **Save**.

Editing drops the tracker's cached server list, so the next view
re-fetches with the new configuration.

### Removing a Tracker

1. Select the tracker from the dropdown.
2. Click the **trash** icon.
3. Confirm the removal.

Removal is permanent. If you change your mind, you can re-add the
tracker as a new entry.

### Validation

Tracker fields are validated client-side:

- **Name** — required, up to 64 characters, no newlines or control
  characters. Names are case-insensitively unique.
- **Address** — required, must be a valid hostname or IP literal. URL
  fragments (`https://`, `/path`, `user@host`, embedded ports) are
  rejected.
- **Port** — always valid (`u16`, 1–65535).
- **Password** — optional, up to 256 bytes, otherwise unrestricted.
- **Fingerprint** — optional; when set, must be the canonical 95-byte
  uppercase hex form separated by colons.

Two trackers that share `(address, port)` (case-insensitive) are also
treated as duplicates and rejected.

## Browsing Servers

Once you have at least one tracker configured, the panel queries it
automatically and displays the results.

### Toolbar Row

| Element              | Description                                                                                                                         |
| -------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| **Tracker dropdown** | Pick which tracker's listing to view. Sorted alphabetically by name.                                                                |
| **Edit / Remove**    | Manage the currently selected tracker.                                                                                              |
| **Refresh**          | Re-fetch the listing for the currently selected tracker. Disabled while a fetch is in flight.                                       |
| **Status text**      | `Loading…` while fetching; `Servers: N` on success (reflecting the visible count after search filtering); error message on failure. |

### Search

The search box below the toolbar live-filters the listing as you type.
Match is case-insensitive substring against name and description, with
**name matches sorted first**, then description-only matches. The
status count updates to reflect how many servers are visible.

The search query resets when you switch trackers (each tracker has its
own search context).

### Listing Columns

| Column          | Description                                                                                             |
| --------------- | ------------------------------------------------------------------------------------------------------- |
| **Name**        | Server display name. Click to connect (see below).                                                      |
| **Description** | Free-form description set by the server operator.                                                       |
| **Users**       | Number of online users at last refresh.                                                                 |
| **Public**      | ✓ means guest login is enabled (anyone can connect without an account); ✕ means an account is required. |

Click any column header to sort by that column. Click the same header
again to reverse the direction.

## Connecting to a Server

There are three ways to act on a server in the listing:

### Click the Name

Click the **Name** of any server in the listing. The Connect form opens
on top of the Trackers panel, pre-filled with the server's name,
address, port, and fingerprint. Add your username, password, and
nickname (if needed), then click **Connect**.

If you change your mind, click **Cancel** (or press Escape) to return
to the Trackers panel.

### Right-Click Context Menu

Right-click a server row to see three menu items:

- **Connect** — same as clicking the Name.
- **Bookmark** — saves the server as a bookmark with blank credentials.
  You can edit the bookmark afterward to add your username and password.
  If a bookmark with the same name or endpoint already exists, you are
  notified via a toast.
- **Copy URI** — copies a `nexus://address:port` link to the clipboard,
  ready to share. IPv6 addresses are bracketed correctly. The
  BBS-default port is dropped from the URI.

## Refreshing

The panel auto-fetches in three situations:

- **On panel open**, if the selected tracker has no cached result yet.
- **On dropdown change**, if the newly selected tracker has no cache.
- **On click of the Refresh button**, regardless of cache state.

The cache is in-memory only and clears when Nexus exits. Switching
between trackers is instant if both already have cached results.

## Fingerprint Mismatches

The first time you query a tracker, Nexus pins the tracker's TLS
certificate fingerprint automatically (TOFU — Trust On First Use).
On subsequent queries, the observed fingerprint must match the
pinned value.

If a tracker's certificate changes (e.g., after a server reinstall),
the next query will trigger an **Accept Fingerprint** dialog showing
the previously stored and newly observed fingerprints. Verify
out-of-band with the tracker operator that the change is legitimate
before clicking **Accept**.

If you click **Cancel** instead, the panel returns to the listing
view and the new fingerprint is **not** pinned. The error remains
visible in the toolbar status row, and you can re-trigger the dialog
via Refresh, or remove the tracker entirely if you no longer trust
it.

## Empty and Error States

The panel surfaces several states:

| State                                          | What you see                                                                                  |
| ---------------------------------------------- | --------------------------------------------------------------------------------------------- |
| No trackers configured                         | The list area shows "No trackers configured." Add one via **`+`**.                            |
| Tracker has no registered servers              | The list area shows "This tracker has no registered servers."                                 |
| Search filter matches nothing                  | The list area shows "No servers match your search." (The status text shows `Servers: 0`.)     |
| Fetch in progress (no prior result)            | The status text shows `Loading…` in a muted style.                                            |
| Fetch failed (no prior result)                 | The status text shows `Error: <message>`. The list area shows the previous-state placeholder. |
| Fetch failed after a previous successful fetch | The status text shows the error, but the previously fetched listing remains visible.          |

A failed fetch never overwrites a previously successful listing — you
keep seeing the last good results until a fresh fetch succeeds.

## Notes on Trust

The fingerprint shown for each server in a tracker's listing is a
display aid, not a trust assertion. When you actually connect to a
server discovered via a tracker, Nexus performs its own two-stage TLS
fingerprint verification (see the [Connections](02-connections.md)
chapter, "Certificate Management"). A malicious tracker cannot
silently route you to an imposter server — the BBS-side verification
catches it.

That said, you choose which trackers to trust. A compromised or
malicious tracker can serve a list slanted toward adversary-run
servers. If a tracker behaves suspiciously, remove it from your
configured list.
