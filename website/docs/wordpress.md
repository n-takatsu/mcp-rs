---
layout: page
title: WordPress Integration
nav_order: 4
---

# WordPress Integration
{: .no_toc }

Complete WordPress REST API integration with advanced media management capabilities.
{: .fs-6 .fw-300 }

## Table of contents
{: .no_toc .text-delta }

1. TOC
{:toc}

---

## Overview

MCP-RS provides comprehensive WordPress integration through the WordPress REST API, enabling AI agents to perform content management tasks with full featured image support.

## Features

### 📝 Content Management
- **Post Creation**: Create new WordPress posts with rich content
- **Post Retrieval**: Get all published posts with metadata
- **Comment Management**: Retrieve and manage post comments
- **Status Control**: Publish, draft, or schedule posts

### 🖼️ Media Management
- **File Upload**: Upload images, documents, and media files
- **Featured Images**: Set and manage post featured images
- **Multiple Formats**: Support for JPEG, PNG, GIF, PDF, and more
- **Base64 Processing**: Handle base64-encoded file data

### 🔐 Security
- **Application Passwords**: Secure authentication with WordPress
- **Timeout Handling**: Configurable request timeouts
- **Retry Logic**: Automatic retry with exponential backoff
- **Error Handling**: Comprehensive error reporting

## Configuration

Add WordPress configuration to your `mcp-config.toml`:

```toml
[wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
timeout_seconds = 30
```

### Setting up Application Passwords

1. Go to **WordPress Admin** → **Users** → **Profile**
2. Scroll to **Application Passwords** section
3. Enter application name (e.g., "MCP-RS Integration")
4. Click **Add New Application Password**
5. Copy the generated password to your config

## Available Tools

### `create_post`
Create a new WordPress post.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content (HTML supported)

**Example Usage:**
```json
{
  "tool": "create_post",
  "arguments": {
    "title": "My New Blog Post",
    "content": "<p>This is the content of my post with <strong>HTML formatting</strong>.</p>"
  }
}
```

### `upload_media`
Upload a media file to WordPress media library.

**Parameters:**
- `file_data` (string): Base64-encoded file data
- `filename` (string): Original filename with extension  
- `mime_type` (string): MIME type (e.g., "image/jpeg")

**Example Usage:**
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

### `create_post_with_featured_image`
Create a post with a featured image in one operation.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content
- `featured_media_id` (number): Media ID from uploaded image

**Example Usage:**
```json
{
  "tool": "create_post_with_featured_image",
  "arguments": {
    "title": "Post with Featured Image", 
    "content": "<p>This post has a beautiful featured image.</p>",
    "featured_media_id": 123
  }
}
```

### `set_featured_image`
Set featured image for an existing post.

**Parameters:**
- `post_id` (number): ID of the post to update
- `media_id` (number): ID of the media to set as featured image

**Example Usage:**
```json
{
  "tool": "set_featured_image",
  "arguments": {
    "post_id": 456,
    "media_id": 123
  }
}
```

### `get_posts`
Retrieve all WordPress posts.

**Parameters:** None

**Returns:** Array of post objects with metadata

### `get_comments`
Retrieve WordPress comments.

**Parameters:**
- `post_id` (number, optional): Filter comments by specific post

**Example Usage:**
```json
{
  "tool": "get_comments",
  "arguments": {
    "post_id": 123
  }
}
```

## Workflow Examples

### Basic Blog Post
```
User: "Create a blog post about Rust programming"
AI automatically:
1. Uses create_post tool
2. Generates title and content
3. Returns post URL and ID
```

### Featured Image Workflow  
```
User: "Upload this image and create a post with it as featured image"
AI automatically:
1. Uses upload_media with base64 image data
2. Uses create_post_with_featured_image with returned media ID
3. Creates complete post with featured image
```

### Update Existing Post
```
User: "Add a featured image to post #123"
AI automatically:
1. Uploads image using upload_media
2. Uses set_featured_image to update post
3. Confirms successful update
```

## Error Handling

The WordPress integration includes comprehensive error handling:

- **Authentication Errors**: Invalid credentials or permissions
- **API Errors**: WordPress REST API specific errors  
- **Network Errors**: Timeout, connection failures
- **Validation Errors**: Invalid parameters or data format
- **File Upload Errors**: Unsupported file types or size limits

All errors include detailed messages for debugging and user feedback.

## Supported File Types

### Images
- JPEG (.jpg, .jpeg)
- PNG (.png) 
- GIF (.gif)
- WebP (.webp)
- SVG (.svg)

### Documents  
- PDF (.pdf)
- Microsoft Word (.doc, .docx)
- Text files (.txt)

### Media
- MP3 (.mp3)
- MP4 (.mp4)
- WAV (.wav)

File size limits depend on your WordPress configuration (`upload_max_filesize` and `post_max_size`).

## Performance Optimization

- **Connection Pooling**: Reuses HTTP connections for better performance
- **Timeout Configuration**: Configurable request timeouts
- **Retry Logic**: Automatic retry with exponential backoff
- **Efficient Uploads**: Streaming multipart uploads for large files

## Troubleshooting

### Common Issues

**Authentication Failed**
- Verify application password is correct
- Check username matches WordPress user
- Ensure user has appropriate permissions

**Upload Failed**
- Check file size against WordPress limits
- Verify MIME type is supported  
- Ensure proper base64 encoding

**Connection Timeout**
- Increase `timeout_seconds` in configuration
- Check network connectivity
- Verify WordPress URL is accessible

### Debug Mode

Enable detailed logging by setting environment variable:
```bash
RUST_LOG=debug cargo run
```

This will show detailed HTTP requests and responses for troubleshooting.