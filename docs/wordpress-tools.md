# WordPress MCP Tools Documentation

## Overview
This document describes the WordPress tools available through the MCP (Model Context Protocol) interface.

## Available Tools

### 📝 Content Management

#### `create_post`
Create a new WordPress post.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content (HTML or plain text)

**Example:**
```json
{
  "tool": "create_post",
  "arguments": {
    "title": "My New Post",
    "content": "<p>This is the content of my post.</p>"
  }
}
```

#### `get_posts`
Retrieve WordPress posts.

**Parameters:** None

**Returns:** List of all published posts

---

### 🖼️ Media Management

#### `upload_media`
Upload a media file to WordPress media library.

**Parameters:**
- `file_data` (string): Base64-encoded file data
- `filename` (string): Original filename with extension
- `mime_type` (string): MIME type (e.g., "image/jpeg", "image/png")

**Example:**
```json
{
  "tool": "upload_media",
  "arguments": {
    "file_data": "iVBORw0KGgoAAAANSUhEUgAA...",
    "filename": "hero-image.jpg",
    "mime_type": "image/jpeg"
  }
}
```

**Returns:** Media object with ID for use as featured image

#### `create_post_with_featured_image`
Create a post with a featured image.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content
- `featured_media_id` (number): Media ID from uploaded image

**Example:**
```json
{
  "tool": "create_post_with_featured_image",
  "arguments": {
    "title": "Post with Featured Image",
    "content": "<p>This post has a featured image.</p>",
    "featured_media_id": 123
  }
}
```

#### `set_featured_image`
Set featured image for an existing post.

**Parameters:**
- `post_id` (number): ID of the post to update
- `media_id` (number): ID of the media to set as featured image

**Example:**
```json
{
  "tool": "set_featured_image",
  "arguments": {
    "post_id": 456,
    "media_id": 123
  }
}
```

---

### 💬 Comment Management

#### `get_comments`
Retrieve WordPress comments.

**Parameters:**
- `post_id` (number, optional): Filter comments by post ID

**Example:**
```json
{
  "tool": "get_comments",
  "arguments": {
    "post_id": 123
  }
}
```

## Workflow Examples

### Basic Blog Post Creation
1. **Create post**: Use `create_post` with title and content
2. **Result**: Returns post ID and URL

### Blog Post with Featured Image
1. **Upload image**: Use `upload_media` with base64 image data
2. **Create post**: Use `create_post_with_featured_image` with returned media ID
3. **Result**: Post created with featured image

### Add Featured Image to Existing Post
1. **Upload image**: Use `upload_media` if not already uploaded
2. **Set featured image**: Use `set_featured_image` with post ID and media ID
3. **Result**: Existing post updated with featured image

## Authentication
All tools require WordPress Application Passwords configured in `mcp-config.toml`:

```toml
[wordpress]
url = "https://your-site.com"
username = "your_username"
password = "your_application_password"
```

## Error Handling
All tools return structured error responses:
- Invalid parameters
- Authentication failures
- WordPress API errors
- Network timeouts

## Supported File Types
Media upload supports:
- **Images**: JPEG, PNG, GIF, WebP
- **Documents**: PDF, DOC, DOCX
- **Audio**: MP3, WAV, OGG
- **Video**: MP4, AVI, MOV

File size limits depend on WordPress configuration.