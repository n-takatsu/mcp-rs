# WordPress MCP Tools - API Reference

## Quick Reference

### Content Management
```json
// Basic post creation
{"tool": "create_post", "arguments": {"title": "Title", "content": "Content"}}

// Advanced post with SEO
{"tool": "create_advanced_post", "arguments": {
  "title": "Title", "content": "Content", "post_type": "post", 
  "status": "publish", "meta_description": "SEO description"
}}

// Post with embeds
{"tool": "create_post_with_embeds", "arguments": {
  "title": "Title", "content": "Content",
  "youtube_urls": ["https://youtube.com/watch?v=VIDEO_ID"],
  "social_urls": ["https://twitter.com/user/status/123"]
}}

// Update post
{"tool": "update_post", "arguments": {
  "post_id": 123, "params": {"title": "New Title", "status": "draft"}
}}
```

### Media Management
```json
// Upload media with accessibility
{"tool": "upload_media", "arguments": {
  "file_data": "base64_content", "filename": "image.jpg", "mime_type": "image/jpeg",
  "alt_text": "Description for screen readers", "caption": "Image caption"
}}

// Update media metadata
{"tool": "update_media", "arguments": {
  "media_id": 123, "alt_text": "Updated alt text", "caption": "New caption"
}}
```

### Taxonomy Management
```json
// Create category
{"tool": "create_category", "arguments": {"name": "Category Name", "description": "Description"}}

// Create tag  
{"tool": "create_tag", "arguments": {"name": "Tag Name", "description": "Description"}}

// Post with taxonomy
{"tool": "create_post_with_categories_tags", "arguments": {
  "title": "Title", "content": "Content",
  "category_names": ["Category1", "Category2"], "tag_names": ["tag1", "tag2"]
}}
```

## Tool Parameters

### create_advanced_post
**Required:**
- `title` (string): Post title
- `content` (string): Post content

**Optional:**
- `post_type` (string): "post" | "page" (default: "post")
- `status` (string): "publish" | "draft" | "private" | "future" (default: "publish")
- `date` (string): ISO8601 timestamp for scheduling
- `categories` (array): Category IDs
- `tags` (array): Tag IDs
- `meta_description` (string): SEO meta description
- `focus_keyword` (string): SEO focus keyword
- `meta_robots_noindex` (boolean): SEO noindex setting
- `meta_robots_nofollow` (boolean): SEO nofollow setting

### upload_media
**Required:**
- `file_data` (string): Base64 encoded file content
- `filename` (string): Original filename
- `mime_type` (string): File MIME type

**Optional:**
- `title` (string): Media title
- `alt_text` (string): Alt text for accessibility
- `caption` (string): Media caption
- `description` (string): Media description

### create_category
**Required:**
- `name` (string): Category name

**Optional:**
- `description` (string): Category description
- `parent` (number): Parent category ID

## Response Formats

### Success Response
```json
{
  "content": [{
    "type": "text", 
    "text": "Operation completed successfully"
  }],
  "isError": false
}
```

### Error Response
```json
{
  "error": {
    "code": -32602,
    "message": "Invalid params",
    "data": "Missing required parameter: title"
  }
}
```

## Status Codes

| Status | Description | Usage |
|--------|-------------|-------|
| `publish` | Public content | Live, visible to all |
| `draft` | Work in progress | Not public, editable |
| `private` | Restricted access | Logged-in users only |
| `future` | Scheduled | Auto-publish at specified time |

## MIME Types

| Extension | MIME Type | Usage |
|-----------|-----------|-------|
| `.jpg`, `.jpeg` | `image/jpeg` | Photos, images |
| `.png` | `image/png` | Graphics, screenshots |
| `.gif` | `image/gif` | Animations |
| `.pdf` | `application/pdf` | Documents |
| `.mp4` | `video/mp4` | Videos |
| `.mp3` | `audio/mpeg` | Audio files |

## URL Formats

### YouTube
- `https://www.youtube.com/watch?v=VIDEO_ID`
- `https://youtu.be/VIDEO_ID`
- `https://www.youtube.com/embed/VIDEO_ID`

### Social Media
- **Twitter**: `https://twitter.com/user/status/123456789`
- **X**: `https://x.com/user/status/123456789`
- **Instagram**: `https://instagram.com/p/POST_ID/`
- **Facebook**: `https://facebook.com/user/posts/123456789`
- **TikTok**: `https://tiktok.com/@user/video/123456789`

## Error Codes

| Code | Message | Cause |
|------|---------|-------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid request | Malformed request |
| -32601 | Method not found | Unknown tool |
| -32602 | Invalid params | Missing/wrong parameters |
| -32603 | Internal error | Server error |

## Limits

| Item | Limit | Notes |
|------|-------|-------|
| File size | 64MB | WordPress default, configurable |
| Title length | 255 chars | WordPress limitation |
| Content length | Unlimited | Practical limit ~64KB for performance |
| Categories per post | Unlimited | Recommended: 2-5 |
| Tags per post | Unlimited | Recommended: 5-10 |
| API requests | Rate limited | Depends on WordPress hosting |