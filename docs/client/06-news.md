# News

This guide covers reading and posting news on Nexus BBS servers.

## News Panel

Access the news panel by clicking the **News** icon in the toolbar (newspaper icon). The panel shows all news posts from the server, newest first.

## Reading News

News posts appear as cards with:

- **Author** — Username and admin badge (if applicable)
- **Date** — When the post was created (or last updated)
- **Content** — Text and/or image
- **Actions** — Edit and delete buttons (if permitted)

### Markdown Support

News posts support markdown formatting:

- **Headings** — `# Heading`, `## Subheading`, etc.
- **Bold/Italic** — `**bold**`, `_italic_`
- **Lists** — Ordered (`1.`) and unordered (`-`, `*`)
- **Code** — Inline `` `code` `` and fenced code blocks
- **Links** — `[text](url)`
- **Blockquotes** — `> quoted text`
- **Tables** — Standard markdown table syntax

### Images

News posts can include images alongside or instead of text.

## Creating News

Requires `news_create` permission.

1. Click the **+** button at the top of the news panel
2. Enter your post content in the text area
3. Optionally add an image:
   - Click **Choose Image** to select an image file
   - Click **Clear Image** to remove a selected image
4. Click **Create** to publish

### Content Requirements

- Posts must have either text, an image, or both
- Text is limited to 4096 characters
- Images are limited to 700KB

### Supported Image Formats

- PNG
- JPEG
- WebP
- SVG

## Editing News

### Who Can Edit

- You can always edit your own posts
- Users with `news_edit` permission can edit posts by non-admin users
- Admins can edit any post

### How to Edit

1. Click the **pencil icon** on the news post
2. Modify the text and/or image
3. Click **Save**

The post will show "(edited)" with the update timestamp.

## Deleting News

### Who Can Delete

- You can always delete your own posts
- Users with `news_delete` permission can delete posts by non-admin users
- Admins can delete any post

### How to Delete

1. Click the **trash icon** on the news post
2. A confirmation dialog appears
3. Click **Delete** to confirm, or **Cancel** to abort

**Warning:** Deletions are permanent and cannot be undone.

## Notifications

You can receive notifications when new posts are published:

1. Open **Settings** (gear icon)
2. Go to the **Events** tab
3. Find **News Post**
4. Enable desktop notifications and/or sound

By default, you won't receive notifications for your own posts.

## Permissions

| Permission | Allows |
|------------|--------|
| `news_list` | View news posts |
| `news_create` | Create new posts |
| `news_edit` | Edit others' posts |
| `news_delete` | Delete others' posts |

**Note:** You can always edit and delete your own posts, regardless of `news_edit` and `news_delete` permissions.

Admins automatically have all permissions.

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Escape` | Cancel editing, close dialogs |
| `Enter` | Confirm dialogs |

## Troubleshooting

### Can't see the news panel

You may not have `news_list` permission. Contact the server admin.

### Can't create posts

You need `news_create` permission. Contact the server admin.

### Can't edit/delete someone else's post

You need `news_edit` or `news_delete` permission. Additionally, non-admins cannot modify posts by admin users.

### Image upload fails

Check that your image:
- Is under 700KB in size
- Is a supported format (PNG, JPEG, WebP, SVG)
- Is a valid image file

### "Content required" error

Posts must have either text content, an image, or both. You cannot create an empty post.

## Next Steps

- [Settings](07-settings.md) — Configure notifications and preferences
- [Troubleshooting](08-troubleshooting.md) — Common issues and solutions