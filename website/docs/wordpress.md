---
layout: page
title: WordPress Integration
permalink: /docs/wordpress/
nav_order: 4
---

# WordPress Integration

{: .no_toc }

**[Home](../) > [Documentation](./) > WordPress Integration**

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

## 📝 Content Management

- **Advanced Post Creation**: Create posts and pages with comprehensive options
- **Post Type Support**: Both posts (blog articles) and pages (static content)
- **Status Control**: Draft, publish, private, and scheduled posts
- **SEO Integration**: Meta fields for Yoast SEO and other plugins
- **Post Retrieval**: Get all published posts with metadata
- **Complete CRUD**: Create, read, update, and delete operations
- **Comment Management**: Retrieve and manage post comments

## 🖼️ Media Management

- **File Upload**: Upload images, documents, and media files
- **Featured Images**: Set and manage post featured images
- **Multiple Formats**: Support for JPEG, PNG, GIF, PDF, and more
- **Base64 Processing**: Handle base64-encoded file data

## 🔐 Security

- **Application Passwords**: Secure authentication with WordPress
- **Timeout Handling**: Configurable request timeouts
- **Retry Logic**: Automatic retry with exponential backoff
- **Error Handling**: Comprehensive error reporting

## 🏷️ Category & Tag Management

- **Category Operations**: Create, read, update, and delete categories
- **Tag Operations**: Create, read, update, and delete tags
- **Hierarchical Categories**: Support for parent-child category relationships
- **Bulk Operations**: Efficient management of multiple categories and tags

## ⚙️ Advanced Features

- **Structured API**: Clean parameter structures for maintainable code
- **Flexible Updates**: Partial updates with optional parameters
- **Meta Data Support**: Custom fields and SEO metadata
- **Post Scheduling**: Future publication with ISO8601 timestamps

## Configuration

Add WordPress configuration to your `mcp-config.toml`:

```toml
[wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
timeout_seconds = 30
```

## Setting up Application Passwords

1. Go to **WordPress Admin** → **Users** → **Profile**
2. Scroll to **Application Passwords** section
3. Enter application name (e.g., "MCP-RS Integration")
4. Click **Add New Application Password**
5. Copy the generated password to your config

## Available Tools

## Core Content Management

### `create_post`

Create a new WordPress post (basic version).

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

### `create_advanced_post`

Create a new WordPress post or page with advanced options.

**Parameters:**
- `title` (string): Post/page title
- `content` (string): Post/page content
- `post_type` (string): "post" (投稿) or "page" (固定ページ) [default: "post"]
- `status` (string): "publish" (公開), "draft" (下書き), "private" (非公開), "future" (予約投稿) [default: "publish"]
- `date` (string, optional): Publication date (ISO8601 format, required for "future" status)
- `categories` (array, optional): Category IDs (posts only)
- `tags` (array, optional): Tag IDs (posts only)
- `featured_media_id` (number, optional): Featured image media ID
- `meta` (object, optional): Meta fields for SEO (e.g., Yoast SEO fields)

**Example Usage:**
```json
{
  "tool": "create_advanced_post",
  "arguments": {
    "title": "SEO-Optimized Post",
    "content": "<p>Content with SEO optimization</p>",
    "post_type": "post",
    "status": "draft",
    "categories": [1, 5],
    "tags": [10, 15, 20],
    "featured_media_id": 123,
    "meta": {
      "_yoast_wpseo_metadesc": "Custom meta description",
      "_yoast_wpseo_meta-robots-noindex": "1"
    }
  }
}
```

### `update_post`

Update an existing WordPress post.

**Parameters:**
- `post_id` (number): ID of the post to update
- `title` (string, optional): New post title
- `content` (string, optional): New post content
- `status` (string, optional): New post status
- `categories` (array, optional): New category IDs
- `tags` (array, optional): New tag IDs
- `featured_media_id` (number, optional): New featured image ID
- `meta` (object, optional): New meta fields

**Example Usage:**
```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "title": "Updated Title",
    "status": "publish",
    "categories": [1, 2, 3]
  }
}
```

### `delete_post`

Delete a WordPress post.

**Parameters:**
- `post_id` (number): ID of the post to delete
- `force` (boolean, optional): Permanently delete (true) or move to trash (false) [default: false]

## Content Retrieval

### `get_posts`

Retrieve WordPress posts.

### `get_pages`

Retrieve WordPress pages.

### `get_all_content`

Retrieve both WordPress posts and pages.

### `get_post`

Retrieve a single WordPress post by ID.

**Parameters:**
- `post_id` (number): ID of the post to retrieve

## Media Management

## Media Management

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

### `get_media`

Retrieve all WordPress media files.

**Parameters:** None

### `get_media_item`

Retrieve a single WordPress media item by ID.

**Parameters:**
- `media_id` (number): Media ID to retrieve

### `update_media`

Update WordPress media metadata (title, alt text, caption, description).

**Parameters:**
- `media_id` (number): Media ID to update
- `title` (string, optional): Media title
- `alt_text` (string, optional): Alternative text for accessibility
- `caption` (string, optional): Media caption
- `description` (string, optional): Media description
- `post` (number, optional): Post ID to attach media to

**Example Usage:**
```json
{
  "tool": "update_media",
  "arguments": {
    "media_id": 123,
    "alt_text": "Product image showing red handbag",
    "caption": "Spring 2024 collection",
    "description": "Detailed view of our latest handbag design"
  }
}
```

### `delete_media`

Delete WordPress media file.

**Parameters:**
- `media_id` (number): Media ID to delete
- `force` (boolean, optional): Force delete (bypass trash) [default: false]

### `create_post_with_featured_image`

Create a post with a featured image in one operation.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content
- `featured_media_id` (number): Media ID from uploaded image

### `set_featured_image`

Set featured image for an existing post.

**Parameters:**
- `post_id` (number): ID of the post to update
- `media_id` (number): ID of the media to set as featured image

## Category & Tag Management

**Returns:** Array of post objects with metadata

## `get_post`

Retrieve a single WordPress post by ID.

**Parameters:**
- `post_id` (number): ID of the post to retrieve

**Example Usage:**
```json
{
  "tool": "get_post",
  "arguments": {
    "post_id": 123
  }
}
```

## `update_post`

Update an existing WordPress post with comprehensive options.

**Parameters:**
- `post_id` (number): ID of post to update
- `title` (string, optional): New post title
- `content` (string, optional): New post content
- `status` (string, optional): Post status (publish, draft, private)
- `categories` (array, optional): Array of category IDs
- `tags` (array, optional): Array of tag IDs
- `featured_media_id` (number, optional): Featured image media ID

**Example Usage:**
```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "title": "Updated Post Title",
    "content": "<p>Updated content here...</p>",
    "status": "publish",
    "categories": [5, 12],
    "tags": [23, 45]
  }
}
```

## `delete_post`

Delete a WordPress post.

**Parameters:**
- `post_id` (number): ID of post to delete
- `force` (boolean, optional): Force delete (permanently delete, bypass trash)

**Example Usage:**
```json
{
  "tool": "delete_post",
  "arguments": {
    "post_id": 123,
    "force": false
  }
}
```

**Note:** When `force` is `false` (default), the post is moved to trash. When `true`, it's permanently deleted.

## `get_comments`

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

## `get_categories`

Retrieve all WordPress categories.

**Parameters:** None

**Returns:** Array of category objects with metadata

## `create_category`

Create a new WordPress category.

**Parameters:**
- `name` (string): Category name
- `description` (string, optional): Category description
- `parent` (number, optional): Parent category ID for hierarchical structure

**Example Usage:**
```json
{
  "tool": "create_category",
  "arguments": {
    "name": "Technology",
    "description": "Posts about technology and software",
    "parent": 5
  }
}
```

## `update_category`

Update an existing WordPress category.

**Parameters:**
- `category_id` (number): ID of category to update
- `name` (string, optional): New category name
- `description` (string, optional): New category description

**Example Usage:**
```json
{
  "tool": "update_category",
  "arguments": {
    "category_id": 10,
    "name": "Web Development",
    "description": "Updated description"
  }
}
```

## `delete_category`

Delete a WordPress category.

**Parameters:**
- `category_id` (number): ID of category to delete
- `force` (boolean, optional): Force delete (bypass trash)

**Example Usage:**
```json
{
  "tool": "delete_category",
  "arguments": {
    "category_id": 10,
    "force": true
  }
}
```

## `get_tags`

Retrieve all WordPress tags.

**Parameters:** None

**Returns:** Array of tag objects with metadata

## `create_tag`

Create a new WordPress tag.

**Parameters:**
- `name` (string): Tag name
- `description` (string, optional): Tag description

**Example Usage:**
```json
{
  "tool": "create_tag",
  "arguments": {
    "name": "rust",
    "description": "Posts about Rust programming language"
  }
}
```

## `update_tag`

Update an existing WordPress tag.

**Parameters:**
- `tag_id` (number): ID of tag to update
- `name` (string, optional): New tag name
- `description` (string, optional): New tag description

**Example Usage:**
```json
{
  "tool": "update_tag",
  "arguments": {
    "tag_id": 15,
    "name": "programming",
    "description": "Updated tag description"
  }
}
```

## `delete_tag`

Delete a WordPress tag.

**Parameters:**
- `tag_id` (number): ID of tag to delete
- `force` (boolean, optional): Force delete (bypass trash)

**Example Usage:**
```json
{
  "tool": "delete_tag",
  "arguments": {
    "tag_id": 15,
    "force": true
  }
}
```

## `create_post_with_categories_tags`

Create a new WordPress post with categories and tags.

**Parameters:**
- `title` (string): Post title
- `content` (string): Post content
- `categories` (array, optional): Array of category IDs
- `tags` (array, optional): Array of tag IDs
- `featured_media_id` (number, optional): Featured image media ID

**Example Usage:**
```json
{
  "tool": "create_post_with_categories_tags",
  "arguments": {
    "title": "Complete Guide to Rust",
    "content": "<p>This comprehensive guide covers Rust programming...</p>",
    "categories": [5, 12],
    "tags": [23, 45, 67],
    "featured_media_id": 89
  }
}
```

## `update_post_categories_tags`

Update categories and tags for an existing WordPress post.

**Parameters:**
- `post_id` (number): ID of post to update
- `categories` (array, optional): Array of category IDs to set
- `tags` (array, optional): Array of tag IDs to set

**Example Usage:**
```json
{
  "tool": "update_post_categories_tags",
  "arguments": {
    "post_id": 123,
    "categories": [5, 8],
    "tags": [15, 20, 25]
  }
}
```

## Workflow Examples

## Basic Blog Post

```
User: "Create a blog post about Rust programming"
AI automatically:
1. Uses create_post tool
2. Generates title and content
3. Returns post URL and ID
```

## Featured Image Workflow

```
User: "Upload this image and create a post with it as featured image"
AI automatically:
1. Uses upload_media with base64 image data
2. Uses create_post_with_featured_image with returned media ID
3. Creates complete post with featured image
```

## Update Existing Post

```
User: "Add a featured image to post #123"
AI automatically:
1. Uploads image using upload_media
2. Uses set_featured_image to update post
3. Confirms successful update
```

## Category Management Workflow

```
User: "Create a new category for web development tutorials"
AI automatically:
1. Uses create_category with name and description
2. Returns category ID and details
3. Can be used for organizing posts

User: "Create a subcategory under Technology"
AI automatically:
1. Uses get_categories to find Technology category ID
2. Uses create_category with parent parameter
3. Creates hierarchical category structure
```

## Tag Management Workflow

```
User: "Create tags for a Rust programming post"
AI automatically:
1. Uses create_tag for each relevant tag (rust, programming, tutorial)
2. Returns tag IDs for future reference
3. Tags can be applied to posts during creation

User: "Update the description of the 'rust' tag"
AI automatically:
1. Uses get_tags to find tag ID
2. Uses update_tag with new description
3. Confirms update success
```

## Smart Content Creation Workflow

```
User: "Create a post about web development in the technology category"
AI intelligently:
1. Uses get_categories to find existing categories
2. Finds "Technology" category (avoids creating duplicates)
3. Suggests relevant tags from get_tags
4. Uses create_post_with_categories_tags with proper taxonomy
5. Creates well-organized content

User: "Add the 'tutorial' tag to post #456"
AI automatically:
1. Gets current post categories/tags using get_posts
2. Adds new tag to existing taxonomy
3. Uses update_post_categories_tags to preserve existing data
4. Confirms successful update
```

## Complete Content Management Workflow

```
User: "Edit post #123 to change the title and add categories"
AI automatically:
1. Uses get_post to retrieve current post details
2. Shows current title, content, and taxonomy
3. Uses update_post with new title and categories
4. Preserves existing content and other metadata
5. Confirms successful update

User: "Delete the draft post about outdated technology"
AI automatically:
1. Uses get_posts to find draft posts
2. Identifies the specific post by content analysis
3. Uses delete_post with force=false (moves to trash)
4. Confirms post moved to trash for recovery if needed
```

## Content Lifecycle Management

```
User: "Publish the draft post and add featured image"
AI automatically:
1. Uses get_post to retrieve draft details
2. Uses upload_media for featured image
3. Uses update_post with status="publish" and featured_media_id
4. Complete publication workflow in one operation

User: "Archive old posts from 2022"
AI automatically:
1. Uses get_posts to find posts from 2022
2. Shows list for user confirmation
3. Uses update_post to change status to "private"
4. Bulk content management operation
```

## AI-Assisted Taxonomy Management

```
User: "Create a post about 'ウェブ開発' (Japanese for web development)"
AI intelligently:
1. Uses get_categories to scan existing categories
2. Finds similar: "Web Development", "ウェブ技術", "webdev"
3. Suggests: "Should I use existing 'Web Development' or create new 'ウェブ開発'?"
4. User confirms choice
5. Creates post with appropriate categorization

This workflow prevents duplicate/similar categories and maintains clean taxonomy structure.
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

## Images

- JPEG (.jpg, .jpeg)
- PNG (.png)
- GIF (.gif)
- WebP (.webp)
- SVG (.svg)

## Documents

- PDF (.pdf)
- Microsoft Word (.doc, .docx)
- Text files (.txt)

## Media

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

## Common Issues

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

## Debug Mode

Enable detailed logging by setting environment variable:
```bash
RUST_LOG=debug cargo run
```

This will show detailed HTTP requests and responses for troubleshooting.

---

## Health Check System

The WordPress MCP integration includes a comprehensive health check system to validate your environment before performing operations. This ensures reliable operation and helps identify configuration issues early.

## `wordpress_health_check`

Perform comprehensive environment validation.

**Parameters:** None

**Usage Example:**
```json
{
  "tool": "wordpress_health_check",
  "arguments": {}
}
```

## Check Process

The health check performs five critical validation stages:

### 1. 🌐 Site Accessibility

- **Purpose**: Verify WordPress site is reachable
- **Test**: HTTP GET request to site URL
- **Success**: Site responds with 200 OK status
- **Failure**: Network timeout, DNS resolution, or server errors

### 2. 🔌 REST API Availability

- **Purpose**: Confirm WordPress REST API is enabled and accessible
- **Test**: GET request to `/wp-json/wp/v2/` endpoint
- **Success**: API responds with namespace information
- **Failure**: REST API disabled, blocked, or misconfigured

### 3. 🔐 Authentication Validation

- **Purpose**: Verify credentials are correct and user exists
- **Test**: Authenticated request to `/wp-json/wp/v2/users/me`
- **Success**: Returns user profile information
- **Failure**: Invalid username, incorrect password, or user not found

### 4. ✅ Permission Verification

- **Purpose**: Check user has required capabilities
- **Test**: Validate publish_posts, upload_files, and edit_posts permissions
- **Success**: User has all required capabilities
- **Failure**: Insufficient permissions for content operations

### 5. 📁 Media Upload Capability

- **Purpose**: Test file upload functionality end-to-end
- **Test**: Upload small test image to media library
- **Success**: File uploaded successfully and media ID returned
- **Failure**: Upload restrictions, storage issues, or API limitations

## Health Report

The health check returns a comprehensive report with the following information:

```json
{
  "status": "healthy",
  "site_info": {
    "url": "https://your-site.com",
    "name": "Your Site Name",
    "version": "6.3.2",
    "api_version": "2.0"
  },
  "user_info": {
    "id": 1,
    "username": "admin",
    "display_name": "Site Administrator",
    "email": "admin@yoursite.com",
    "roles": ["administrator"]
  },
  "capabilities": {
    "publish_posts": true,
    "upload_files": true,
    "edit_posts": true,
    "manage_options": true
  },
  "media_upload": {
    "supported": true,
    "test_upload_id": 789,
    "max_upload_size": "64M"
  },
  "performance": {
    "response_time_ms": 245,
    "api_response_time_ms": 89
  }
}
```

## Status Levels

**🟢 Healthy**
- All checks passed
- System ready for full operation
- No configuration issues detected

**🟡 Warning**
- Minor issues detected
- Basic functionality available
- Some features may be limited

**🔴 Critical**
- Major issues prevent operation
- Configuration required
- System not usable

## Troubleshooting Guide

### Site Accessibility Issues

```
❌ Error: Site not accessible
🔧 Solutions:
   • Verify URL is correct and includes https://
   • Check site is online and responding
   • Ensure firewall/security plugins allow access
   • Test URL in browser manually
```

### REST API Problems

```
❌ Error: REST API not available
🔧 Solutions:
   • Enable REST API in WordPress settings
   • Check security plugins aren't blocking API
   • Verify .htaccess isn't blocking /wp-json/
   • Test API endpoint in browser: /wp-json/wp/v2/
```

### Authentication Failures

```
❌ Error: Authentication failed
🔧 Solutions:
   • Regenerate application password
   • Verify username is exact match
   • Check user account is active
   • Ensure application passwords are enabled
```

### Permission Issues

```
❌ Error: Insufficient permissions
🔧 Solutions:
   • Assign Editor or Administrator role
   • Check user has publish_posts capability
   • Verify upload_files permission enabled
   • Review role-based access restrictions
```

### Upload Problems

```
❌ Error: Media upload failed
🔧 Solutions:
   • Check upload_max_filesize in php.ini
   • Verify post_max_size setting
   • Ensure uploads directory is writable
   • Check disk space availability
```

## Best Practices

### Before Production Use

1. **Run Health Check**: Always validate environment first
2. **Monitor Performance**: Check response times regularly
3. **Test Permissions**: Verify all required capabilities
4. **Validate Uploads**: Confirm media functionality works

### Production Operations

#### Application Password Management

WordPress application passwords may be invalidated by:
- **Hosting Provider Policies**: Automatic expiration by hosting services
- **Security Plugin Policies**: Plugins like SiteGuard enforcing rotation
- **WordPress Updates**: Major core updates affecting authentication
- **Server Changes**: PHP or environment configuration modifications

**Monitoring Strategy:**
```bash

## Daily health monitoring (recommended)

cargo run --example comprehensive_test

## Deep diagnostic for authentication issues

cargo run --example settings_api_deep_diagnosis

## Authentication verification

cargo run --example auth_diagnosis
```

#### Maintenance Mode Operations

**LightStart Plugin Configuration:**
When using WordPress maintenance mode plugins, configure exclusions for REST API access:

```

## Add to LightStart exclusions (slug format, no leading slash):

wp-json/*
```

This ensures MCP-RS can continue content operations during maintenance windows.

**Benefits:**
- ✅ Content management continuity during maintenance
- ✅ Zero-downtime WordPress updates
- ✅ Emergency content access during site maintenance

#### Production Monitoring

**Alert Criteria:**
- **HTTP 401 Errors**: Application password expiration → Regenerate password
- **Connection Timeouts**: Network/hosting issues → Check infrastructure
- **API Changes**: Plugin/core updates → Verify endpoint compatibility
- **SSL Issues**: Certificate problems → Update certificates

**Incident Response:**
1. **Detection**: Automated monitoring or error reports
2. **Diagnosis**: Run `settings_api_deep_diagnosis` for detailed analysis
3. **Classification**:
   - Password issue → WordPress Admin → Generate new application password
   - Plugin interference → Configure maintenance mode exclusions
   - Network problems → Infrastructure team investigation
4. **Resolution**: Apply fix and verify with comprehensive test
5. **Documentation**: Update operational logs and procedures

### Automated Monitoring

```rust
// Example: Periodic health validation
use mcp_rs::handlers::wordpress::WordPressHandler;

async fn monitor_wordpress_health() {
    let handler = WordPressHandler::new(config).await?;

    match handler.health_check().await {
        Ok(report) if report.status == "healthy" => {
            println!("✅ WordPress system healthy");
        },
        Ok(report) => {
            println!("⚠️ Issues detected: {}", report.status);
            // Handle warnings or critical issues
        },
        Err(e) => {
            println!("❌ Health check failed: {}", e);
            // Alert administrators
        }
    }
}
```

### Integration Workflow

```
1. Environment Setup
   ├── Configure mcp-config.toml
   ├── Run wordpress_health_check
   └── Address any issues found

2. Validation Success
   ├── Proceed with content operations
   ├── Monitor performance metrics
   └── Schedule regular health checks

3. Issue Detection
   ├── Review detailed error messages
   ├── Apply recommended solutions
   └── Re-run health check to verify fixes

4. Production Monitoring
   ├── Weekly comprehensive tests
   ├── Application password rotation (as needed)
   ├── Maintenance mode coordination
   └── Incident response procedures
```

This health check system ensures your WordPress integration is production-ready and helps prevent common configuration issues that could interrupt your workflow.
