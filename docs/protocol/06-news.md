# News

News provides a bulletin board for server announcements and posts. News items support markdown content and optional images.

## Flow

### Listing News

```
Client                                        Server
   │                                             │
   │  NewsList                                   │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         NewsListResponse { items }          │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Viewing a Single News Item

```
Client                                        Server
   │                                             │
   │  NewsShow { id }                            │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         NewsShowResponse { news }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
```

### Creating News

```
Client                                        Server
   │                                             │
   │  NewsCreate { body, image }                 │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         NewsCreateResponse { news }         │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         NewsUpdated { action: Created }     │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

### Editing News

```
Client                                        Server
   │                                             │
   │  NewsEdit { id }                            │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         NewsEditResponse { news }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │                                             │
   │  NewsUpdate { id, body, image }             │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         NewsUpdateResponse { news }         │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         NewsUpdated { action: Updated }     │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

### Deleting News

```
Client                                        Server
   │                                             │
   │  NewsDelete { id }                          │
   │ ───────────────────────────────────────►    │
   │                                             │
   │         NewsDeleteResponse { id }           │
   │ ◄───────────────────────────────────────    │
   │                                             │
   │         NewsUpdated { action: Deleted }     │
   │ ◄─────────── (broadcast to all) ────────    │
   │                                             │
```

## Messages

### NewsList (Client → Server)

Request the list of all news items.

This message has no fields.

**Example:**

```json
{}
```

**Full frame:**

```
NX|8|NewsList|a1b2c3d4e5f6|2|{}
```

### NewsListResponse (Server → Client)

Response containing all news items.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `items` | array | If success | Array of `NewsItem` objects (newest first) |

**Success example:**

```json
{
  "success": true,
  "items": [
    {
      "id": 3,
      "body": "# Welcome!\n\nWelcome to the server.",
      "image": null,
      "author": "admin",
      "author_is_admin": true,
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": null
    },
    {
      "id": 1,
      "body": "Server rules: be nice!",
      "image": "data:image/png;base64,...",
      "author": "admin",
      "author_is_admin": true,
      "created_at": "2024-01-10T08:00:00Z",
      "updated_at": "2024-01-12T14:20:00Z"
    }
  ]
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "Permission denied"
}
```

### NewsShow (Client → Server)

Request a single news item by ID.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | integer | Yes | News item ID |

**Example:**

```json
{
  "id": 3
}
```

### NewsShowResponse (Server → Client)

Response containing the requested news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `news` | object | If success | `NewsItem` object |

**Success example:**

```json
{
  "success": true,
  "news": {
    "id": 3,
    "body": "# Welcome!\n\nWelcome to the server.",
    "image": null,
    "author": "admin",
    "author_is_admin": true,
    "created_at": "2024-01-15T10:30:00Z",
    "updated_at": null
  }
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "News item not found"
}
```

### NewsCreate (Client → Server)

Create a new news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `body` | string | No | Markdown content (max 4096 characters) |
| `image` | string | No | Image as data URI (max 700KB) |

At least one of `body` or `image` must be provided.

**Text-only example:**

```json
{
  "body": "# Server Update\n\nNew features available!"
}
```

**With image example:**

```json
{
  "body": "Check out this screenshot!",
  "image": "data:image/png;base64,iVBORw0KGgo..."
}
```

**Image-only example:**

```json
{
  "image": "data:image/png;base64,iVBORw0KGgo..."
}
```

### NewsCreateResponse (Server → Client)

Response after creating a news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether creation succeeded |
| `error` | string | If failure | Error message |
| `news` | object | If success | Created `NewsItem` object |

**Success example:**

```json
{
  "success": true,
  "news": {
    "id": 4,
    "body": "# Server Update\n\nNew features available!",
    "image": null,
    "author": "alice",
    "author_is_admin": false,
    "created_at": "2024-01-16T12:00:00Z",
    "updated_at": null
  }
}
```

### NewsEdit (Client → Server)

Request a news item for editing.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | integer | Yes | News item ID |

**Example:**

```json
{
  "id": 4
}
```

### NewsEditResponse (Server → Client)

Response containing the news item data for editing.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether the request succeeded |
| `error` | string | If failure | Error message |
| `news` | object | If success | `NewsItem` object to edit |

**Success example:**

```json
{
  "success": true,
  "news": {
    "id": 4,
    "body": "# Server Update\n\nNew features available!",
    "image": null,
    "author": "alice",
    "author_is_admin": false,
    "created_at": "2024-01-16T12:00:00Z",
    "updated_at": null
  }
}
```

**Failure example (not author):**

```json
{
  "success": false,
  "error": "You can only edit your own news posts"
}
```

### NewsUpdate (Client → Server)

Update an existing news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | integer | Yes | News item ID |
| `body` | string | No | New markdown content |
| `image` | string | No | New image as data URI |

At least one of `body` or `image` must be provided after update.

**Example:**

```json
{
  "id": 4,
  "body": "# Server Update v2\n\nEven more features!",
  "image": null
}
```

### NewsUpdateResponse (Server → Client)

Response after updating a news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether update succeeded |
| `error` | string | If failure | Error message |
| `news` | object | If success | Updated `NewsItem` object |

**Success example:**

```json
{
  "success": true,
  "news": {
    "id": 4,
    "body": "# Server Update v2\n\nEven more features!",
    "image": null,
    "author": "alice",
    "author_is_admin": false,
    "created_at": "2024-01-16T12:00:00Z",
    "updated_at": "2024-01-16T14:30:00Z"
  }
}
```

### NewsDelete (Client → Server)

Delete a news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | integer | Yes | News item ID |

**Example:**

```json
{
  "id": 4
}
```

### NewsDeleteResponse (Server → Client)

Response after deleting a news item.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `success` | boolean | Yes | Whether deletion succeeded |
| `error` | string | If failure | Error message |
| `id` | integer | If success | Deleted news item ID |

**Success example:**

```json
{
  "success": true,
  "id": 4
}
```

**Failure example:**

```json
{
  "success": false,
  "error": "You can only delete your own news posts"
}
```

### NewsUpdated (Server → Client)

Broadcast to all users when news changes.

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `action` | string | Yes | One of: `"Created"`, `"Updated"`, `"Deleted"` |
| `id` | integer | Yes | Affected news item ID |

**Created example:**

```json
{
  "action": "Created",
  "id": 5
}
```

**Updated example:**

```json
{
  "action": "Updated",
  "id": 3
}
```

**Deleted example:**

```json
{
  "action": "Deleted",
  "id": 4
}
```

## Data Structures

### NewsItem

| Field | Type | Description |
|-------|------|-------------|
| `id` | integer | Unique news item ID |
| `body` | string | Markdown content (null if image-only) |
| `image` | string | Image as data URI (null if text-only) |
| `author` | string | Username of the creator |
| `author_is_admin` | boolean | Whether author is an admin |
| `created_at` | string | ISO 8601 creation timestamp |
| `updated_at` | string | ISO 8601 last update timestamp (null if never updated) |

### NewsAction

| Value | Description |
|-------|-------------|
| `"Created"` | A new news item was created |
| `"Updated"` | An existing news item was modified |
| `"Deleted"` | A news item was deleted |

## Permissions

| Permission | Required For |
|------------|--------------|
| `news_list` | Viewing news (`NewsList`, `NewsShow`, receiving `NewsUpdated`) |
| `news_create` | Creating news (`NewsCreate`) |
| `news_edit` | Editing news (`NewsEdit`, `NewsUpdate`) |
| `news_delete` | Deleting news (`NewsDelete`) |

### Ownership Rules

Non-admin users can only edit/delete their own posts:

| User | Can Edit | Can Delete |
|------|----------|------------|
| Author (non-admin) | Own posts only | Own posts only |
| Admin | All posts | All posts |

Admins have all permissions automatically.

## Content Validation

### Body

| Rule | Value | Error |
|------|-------|-------|
| Max length | 4096 characters | Body too long |
| No control chars | Except `\n`, `\r`, `\t` | Invalid characters |
| Empty allowed | Can be null if image provided | — |

News body supports full markdown including:
- Headings (`#`, `##`, etc.)
- Bold/italic (`**bold**`, `_italic_`)
- Lists (ordered and unordered)
- Code blocks (inline and fenced)
- Links (`[text](url)`)
- Blockquotes (`> quote`)
- Tables

### Image

| Constraint | Value |
|------------|-------|
| Max size | 700KB (as data URI) |
| Max decoded | 512KB (binary) |
| Formats | PNG, WebP, JPEG, SVG |
| Empty allowed | Can be null if body provided |

Images are transmitted as data URIs:

```
data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAA...
```

### Content Requirement

At least one of `body` or `image` must be provided. A news item cannot have both fields empty or null.

## Ordering

News items are returned newest first (descending by creation date).

## Error Handling

### Common Errors

| Error | Cause | Connection |
|-------|-------|------------|
| Not logged in | Sent before authentication | Disconnected |
| Permission denied | Missing required permission | Stays connected |
| News item not found | Invalid ID | Stays connected |
| Body too long | Exceeds 4096 characters | Stays connected |
| Invalid characters | Control characters in body | Stays connected |
| Image too large | Exceeds 700KB | Stays connected |
| Invalid image format | Not PNG/WebP/JPEG/SVG | Stays connected |
| Content required | Both body and image are empty | Stays connected |
| You can only edit your own news posts | Non-admin editing others' posts | Stays connected |
| You can only delete your own news posts | Non-admin deleting others' posts | Stays connected |

## Broadcast Behavior

When news changes, all users with `news_list` permission receive a `NewsUpdated` broadcast:

- **Created:** Clients should refresh their news list or fetch the new item
- **Updated:** Clients should refresh the affected item
- **Deleted:** Clients should remove the item from their list

The broadcast only contains the action and ID, not the full content. Clients must fetch updated content separately if needed.

## Notes

- News items are persisted in the database and survive server restart
- The author field is set automatically from the session username
- `author_is_admin` reflects the author's admin status at creation time
- Timestamps are in ISO 8601 format (e.g., `"2024-01-15T10:30:00Z"`)
- `updated_at` is null until the first edit
- Deleting a news item is permanent (no soft delete)
- Markdown is rendered client-side; the server stores raw markdown

## Next Step

- Browse [files](07-files.md)
- Manage server and users with [admin commands](09-admin.md)