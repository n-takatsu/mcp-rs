# WordPress Integration Guide

## Overview

This guide covers the complete WordPress integration available in mcp-rs, including 27 MCP tools that provide comprehensive content management functionality for AI agents.

## Quick Start

### 1. Configuration

```toml
[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
timeout_seconds = 30
enabled = true
```

### 2. WordPress Application Password Setup

1. WordPress Admin → Users → Your Profile
2. Scroll to "Application Passwords" 
3. Create new application password for MCP-RS
4. Use this password in configuration

### 3. Test Connection

```bash
cargo run --example wordpress_test
```

## Available Tools (27 total)

### 📝 Content Management (10 tools)

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `wordpress_health_check` | Environment diagnostics | Connection, auth, API status |
| `get_posts` | List posts | Pagination, status filtering |
| `get_pages` | List pages | WordPress pages retrieval |
| `get_all_content` | All content | Combined posts + pages |
| `get_post` | Single post/page | By ID retrieval |
| `create_post` | Basic post creation | Title + content |
| `create_advanced_post` | Advanced creation | SEO, scheduling, taxonomy |
| `create_post_with_embeds` | Embedded content | YouTube, social media |
| `update_post` | Post modification | Structured updates |
| `delete_post` | Post removal | Trash or permanent |

### 🖼️ Media Management (7 tools)

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `upload_media` | File upload | Base64, accessibility metadata |
| `get_media` | List media | Library browsing with pagination |
| `get_media_item` | Single media | Detailed media information |
| `update_media` | Media metadata | Alt text, captions, descriptions |
| `delete_media` | Media removal | File deletion |
| `create_post_with_featured_image` | Post + image | Combined creation |
| `set_featured_image` | Featured image | Update existing posts |

### 📁 Taxonomy Management (8 tools)

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `get_categories` | List categories | Hierarchical structure |
| `create_category` | New category | Parent-child relationships |
| `update_category` | Category changes | Name, description, hierarchy |
| `delete_category` | Category removal | Cleanup |
| `get_tags` | List tags | All available tags |
| `create_tag` | New tag | Flat taxonomy |
| `update_tag` | Tag changes | Name and description |
| `delete_tag` | Tag removal | Cleanup |

### 🔗 Integration Tools (2 tools)

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `create_post_with_categories_tags` | Post + taxonomy | Integrated creation |
| `update_post_categories_tags` | Taxonomy updates | Relationship management |

## Advanced Features

### Post Types & Status

**Supported Post Types:**
- `post` - Blog articles, news posts
- `page` - Static pages (About, Contact, etc.)

**Available Statuses:**
- `publish` - Public, live content
- `draft` - Work in progress
- `private` - Logged-in users only
- `future` - Scheduled publication

### SEO Integration

#### Yoast SEO Support
```json
{
  "meta_description": "SEO meta description",
  "focus_keyword": "target keyword",
  "meta_robots_noindex": false,
  "meta_robots_nofollow": false
}
```

#### Scheduled Publishing
```json
{
  "status": "future",
  "date": "2024-12-25T10:00:00Z"
}
```

### Embedded Content

#### YouTube Integration
- **URL Formats**: `youtube.com/watch?v=`, `youtu.be/`, `youtube.com/embed/`
- **Auto iframe**: Generates proper embed code
- **Fallback**: WordPress oEmbed for unsupported URLs

#### Social Media Support
- **Twitter/X**: oEmbed integration
- **Instagram**: Automatic post embedding
- **Facebook**: Post and video embedding  
- **TikTok**: Video embedding

### Accessibility Features

#### Media Accessibility
```json
{
  "alt_text": "Descriptive text for screen readers",
  "caption": "Visible caption text", 
  "description": "Detailed image description"
}
```

#### Best Practices
- Always provide alt text for images
- Use descriptive captions
- Include detailed descriptions for complex images
- Follow WCAG guidelines

## Usage Examples

### Complete Content Workflow

```json
// 1. Health Check
{
  "tool": "wordpress_health_check",
  "arguments": {}
}

// 2. Create Category
{
  "tool": "create_category",
  "arguments": {
    "name": "Tutorials",
    "description": "How-to guides and tutorials"
  }
}

// 3. Upload Featured Image
{
  "tool": "upload_media",
  "arguments": {
    "file_data": "base64_encoded_image",
    "filename": "tutorial-header.jpg",
    "mime_type": "image/jpeg",
    "alt_text": "Tutorial header showing code example",
    "caption": "Code example for WordPress development"
  }
}

// 4. Create Advanced Post
{
  "tool": "create_advanced_post", 
  "arguments": {
    "title": "WordPress Development Tutorial",
    "content": "<p>Learn WordPress development...</p>",
    "post_type": "post",
    "status": "publish",
    "categories": [1],
    "meta_description": "Complete WordPress development guide",
    "focus_keyword": "WordPress tutorial"
  }
}
```

### Embedded Content Creation

```json
{
  "tool": "create_post_with_embeds",
  "arguments": {
    "title": "Video Tutorial: WordPress Basics",
    "content": "<p>Watch this comprehensive tutorial:</p>",
    "youtube_urls": [
      "https://www.youtube.com/watch?v=dQw4w9WgXcQ"
    ],
    "social_urls": [
      "https://twitter.com/wordpress/status/123456789"
    ],
    "status": "publish",
    "categories": [1]
  }
}
```

### Media Management Workflow

```json
// Upload with full accessibility
{
  "tool": "upload_media",
  "arguments": {
    "file_data": "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8/5+hHgAHggJ/PchI7wAAAABJRU5ErkJggg==",
    "filename": "accessible-chart.png",
    "mime_type": "image/png",
    "title": "Sales Data Chart",
    "alt_text": "Bar chart showing 25% sales increase from Q1 to Q2",
    "caption": "Q2 sales performance exceeded expectations",
    "description": "Detailed bar chart displaying quarterly sales data with Q1 at $100k and Q2 at $125k, representing a 25% growth"
  }
}

// Update media metadata
{
  "tool": "update_media",
  "arguments": {
    "media_id": 123,
    "alt_text": "Updated: Bar chart showing 25% sales increase with detailed quarterly breakdown"
  }
}
```

## Error Handling

### Common Error Types

```json
{
  "error": {
    "code": -32602,
    "message": "Invalid params", 
    "data": "Missing required parameter: title"
  }
}
```

**Error Categories:**
- **Authentication**: Wrong credentials, expired tokens
- **Validation**: Missing/invalid parameters
- **Network**: Connection timeouts, DNS issues
- **WordPress**: Plugin conflicts, permission issues
- **Media**: File size limits, unsupported formats

### Retry Strategy

- **Exponential backoff**: Delays increase with each retry
- **Maximum attempts**: 3 retries by default
- **Timeout handling**: 30-second default timeout
- **Error logging**: Structured logging with tracing

## Testing & Examples

### Available Test Examples

```bash
# Basic functionality
cargo run --example wordpress_test

# Complete CRUD operations  
cargo run --example wordpress_post_crud_test

# Media management with accessibility
cargo run --example wordpress_media_crud_test

# Embedded content creation
cargo run --example wordpress_embed_test

# Advanced post features (SEO, scheduling)
cargo run --example wordpress_advanced_post_test

# Taxonomy management
cargo run --example wordpress_categories_tags_test

# Integrated workflows
cargo run --example wordpress_posts_with_taxonomy_test
```

### Test Environment Setup

```bash
# Local WordPress with Docker
docker run -d \
  -p 8080:80 \
  -e WORDPRESS_DB_HOST=db \
  -e WORDPRESS_DB_PASSWORD=wordpress \
  wordpress:latest

# Or use existing WordPress site
# Configure in mcp-config.toml
```

## Performance Considerations

### Optimization Features

- **Connection pooling**: Reuse HTTP connections
- **Async operations**: Non-blocking I/O
- **Timeout handling**: Prevent hanging requests
- **Retry logic**: Handle transient failures
- **Structured logging**: Performance monitoring

### Best Practices

- **Batch operations**: Group related API calls
- **Pagination**: Handle large datasets efficiently  
- **Caching**: Cache taxonomy and media metadata
- **Error handling**: Graceful degradation
- **Resource cleanup**: Proper connection management

## Security Considerations

### Authentication

- **Application Passwords**: WordPress-recommended method
- **Environment Variables**: Secure credential storage
- **No plaintext logging**: Credentials excluded from logs
- **HTTPS enforcement**: Secure transport required

### Input Validation

- **JSON schema**: Strict parameter validation
- **Type safety**: Rust compile-time checks
- **SQL injection prevention**: Parameterized queries
- **XSS protection**: Content sanitization

### Access Control

- **Principle of least privilege**: Minimal required permissions
- **User role verification**: WordPress role-based access
- **API endpoint restrictions**: Limited to required endpoints
- **Audit logging**: Track all operations

## Troubleshooting

### Common Issues

1. **Connection refused**
   - Check WordPress URL and port
   - Verify WordPress is running
   - Check firewall settings

2. **Authentication failed** 
   - Verify Application Password
   - Check username spelling
   - Ensure user has required permissions

3. **Tool not found**
   - Verify WordPress handler is enabled
   - Check MCP server startup logs
   - Confirm tool name spelling

4. **Invalid parameters**
   - Review tool documentation
   - Validate JSON schema
   - Check required vs optional parameters

5. **Timeout errors**
   - Increase timeout_seconds in config
   - Check network connectivity
   - Verify WordPress performance

### Debug Mode

```toml
[handlers.wordpress]
# ... other settings
debug = true  # Enable verbose logging
timeout_seconds = 60  # Increase timeout
```

### Logging

```bash
# Set log level
RUST_LOG=debug cargo run

# Focus on WordPress handler
RUST_LOG=mcp_rs::handlers::wordpress=debug cargo run
```

## Contributing

### Adding New Tools

1. **Define tool schema** in `list_tools()`
2. **Implement handler** in `call_tool()`
3. **Add tests** in `examples/`
4. **Update documentation**

### WordPress API Extensions

1. **Research WordPress REST API** endpoints
2. **Design MCP tool interface**
3. **Implement with error handling**
4. **Test with real WordPress instance**
5. **Add comprehensive examples**

### Best Practices

- **Follow existing patterns** in codebase
- **Include accessibility features** for media
- **Provide comprehensive examples**
- **Test error conditions** thoroughly
- **Document all parameters** clearly