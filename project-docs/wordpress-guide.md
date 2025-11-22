# WordPress Integration Guide 🛡️ Enterprise Security Edition

## Overview

This guide covers the complete WordPress integration available in mcp-rs, featuring 27 MCP tools with enterprise-grade security protection. All WordPress operations are secured by the 6-layer security architecture providing comprehensive protection against modern web threats.

## 🚀 Quick Start

## 1. Secure Configuration

```toml
[handlers.wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"  

## Encrypted at rest with AES-GCM-256

timeout_seconds = 30
enabled = true

## Security Features (Auto-enabled)

rate_limiting = true
sql_injection_protection = true
xss_protection = true
audit_logging = true
```

## 2. WordPress Application Password Setup (Security Best Practices)

1. WordPress Admin → Users → Your Profile
2. Scroll to "Application Passwords"
3. Create new application password for MCP-RS
4. Use this password in configuration (automatically encrypted)
5. **Security Note**: Application passwords provide better security than regular passwords

## 3. Test Secure Connection

```bash

## Run comprehensive security test

cargo run --example wordpress_security_integration

## Run basic connection test

cargo run --example wordpress_test
```

## 🔑 WordPress Permissions & Administrative Access

## Current Permission Status (2025-11-03 Updated)

### Detailed Diagnosis Results

```
✅ Working Functions (wpmaster: Administrator privileges confirmed):
- Category API (/wp/v2/categories) - 8 items retrieved successfully
- Posts API (/wp/v2/posts) - 1 post, 9 pages retrieved successfully
- Media API (/wp/v2/media) - 2 items retrieved successfully
- Tags API (/wp/v2/tags) - 6 items retrieved successfully

❌ Restricted Functions:
- Settings API (/wp/v2/settings) - 401 Unauthorized

🔍 Issue Identification:
Administrator privileges are normal, but only Settings API is individually restricted
```

## New Identified Issue: Settings API Individual Restriction

### Problem Occurring Even with Confirmed Administrator Privileges

Despite the wpmaster user having **administrator privileges**, only the `/wp/v2/settings` endpoint returns 401 errors. This could be caused by:

### Possible Causes and Solutions

#### 1. WordPress REST API Settings Individual Restriction

**Problem**: Settings API is specially restricted
```php
// Possibly restricted in functions.php or plugins
add_filter( 'rest_pre_dispatch', function( $result, $server, $request ) {
    $route = $request->get_route();

    // Restrict access to Settings API
    if ( strpos( $route, '/wp/v2/settings' ) === 0 ) {
        return new WP_Error( 'rest_forbidden', 'Settings API access restricted',
                           array( 'status' => 401 ) );
    }

    return $result;
}, 10, 3 );
```

**Solution**:
- Check theme's `functions.php`
- Check for REST API restrictions in plugins

#### 2. Security Plugin Restrictions

**Common restriction plugins**:
- Wordfence Security
- iThemes Security (formerly Better WP Security)
- Sucuri Security
- All In One WP Security & Firewall

**Check Location**:
```
Security Plugin Settings → REST API → Settings Endpoint
or
Firewall → Advanced Rules → REST API Restrictions
```

#### 3. WordPress Version-Specific Restrictions

**WordPress 5.5+ Tightening**:
- Enhanced Settings API restrictions with Application Passwords
- Cases requiring Cookie authentication
- Additional restrictions on WordPress.com hosted sites

#### 4. Application Password Permission Scope Restrictions

**Verification Method**:
```
WordPress Admin → Users → Profile
↓
Application Passwords Section
↓
"Revoke" current password
↓
Generate new Application Password
```

## Security-Conscious Permission Settings

### Based on Principle of Least Privilege

#### Option 1: Administrator Privilege Grant (Easy but broad permissions)

```
Pros: All features immediately available
Cons: Security risk due to excessive privilege grant
Recommendation: Test environments only
```

#### Option 2: Custom Role Creation (Recommended)

```
Pros: Grant only minimum necessary privileges
Cons: Initial setup somewhat complex
Recommendation: Recommended for production
```

#### Option 3: Use Permission Control Plugins

```
Recommended Plugins:
- User Role Editor
- Members
- Capability Manager Enhanced

Pros: Easy permission adjustment via GUI
Cons: Plugin dependency
```

## Step-by-Step Implementation (Administrator Privileges Confirmed Response)

### Phase 1: Immediate Diagnosis and Response

```
1. Check security plugin settings
   - Wordfence → Firewall → Advanced Rules
   - iThemes Security → System Tweaks → REST API

2. Regenerate Application Password
   - WordPress Admin → Users → Profile
   - Revoke current password → Generate new one

3. Check theme functions.php
   - Check for REST API restriction filters

4. Run Settings API test
   cargo run --example comprehensive_test
```

### Phase 2: WordPress Environment Verification/Adjustment

```
1. Verify WordPress version
   - For WordPress 5.5+, check Settings API restriction enhancement

2. Individual plugin disable test
   - Temporarily disable security-related plugins
   - Test Settings API access

3. Verify WordPress REST API settings
   - Admin → Settings → Permalinks → "Save Changes"

4. Check WordPress.com hosted restrictions
   - Verify additional restrictions if hosted on WordPress.com
```

### Phase 3: Alternative Approach Implementation

```
1. Consider Settings API use via Cookie authentication
2. Settings change flow via WordPress admin interface
3. Continue operation with non-Settings API functions
4. Consider custom endpoint creation
```

## Permission Verification Commands

### Permission Testing with MCP-RS

```bash

## Comprehensive permission test

cargo run --example comprehensive_test

## Authentication diagnosis

cargo run --example auth_diagnosis

## Health check (including permission verification)

cargo run --example wordpress_health_check
```

### WordPress-side Permission Verification

```php
// Code for checking current user permissions
$user = wp_get_current_user();
$capabilities = $user->allcaps;

// Check MCP required permissions
$required_caps = [
    'manage_options',
    'edit_posts',
    'upload_files',
    'manage_categories'
];

foreach ($required_caps as $cap) {
    if (user_can($user, $cap)) {
        echo "✅ {$cap}: Allowed\n";
    } else {
        echo "❌ {$cap}: Denied\n";
    }
}
```

## Expected Results

### Test Results After Permission Setup (Administrator Privileges Confirmed)

```
🔍 WordPress API Endpoint Diagnosis Results:
✅ Category API (/wp/v2/categories) - 8 items retrieved successfully
✅ Posts API (/wp/v2/posts) - 1 post, 9 pages retrieved successfully
✅ Media API (/wp/v2/media) - 2 items retrieved successfully
✅ Tags API (/wp/v2/tags) - 6 items retrieved successfully
❌ Settings API (/wp/v2/settings) - 401 Unauthorized ← Individual restriction

📊 Diagnosis Summary:
🔗 Basic Connection: Normal
🔐 Authentication: Fully valid (Administrator privileges confirmed)
⚙️ Settings API Permission: Denied due to individual restriction ← Requires investigation
```

## 🛡️ Security Architecture

## Multi-Layer Protection

1. **Encryption Layer**: AES-GCM-256 + PBKDF2 (100K iterations)
2. **Rate Limiting**: Token bucket algorithm + DDoS protection
3. **TLS Enforcement**: TLS 1.2+ with certificate validation
4. **Input Protection**: SQL injection (11 patterns) + XSS (14 patterns)
5. **Monitoring**: Real-time threat detection and analysis
6. **Audit Logging**: Complete security event recording

## Attack Protection Coverage

- ✅ SQL Injection (11 attack patterns detected)
- ✅ XSS Attacks (14 attack vectors blocked)
- ✅ CSRF Protection
- ✅ DDoS/Rate Limiting Attacks
- ✅ Brute Force Authentication
- ✅ File Upload Attacks
- ✅ Code Injection Attempts

## Available Tools (27 total) - All Security Protected

## 📝 Content Management (10 tools) - 🔒 XSS Protected

| Tool | Purpose | Security Features |
|------|---------|------------------|
| `wordpress_health_check` | Environment diagnostics | Connection validation, auth verification |
| `get_posts` | List posts | SQL injection protection, parameter validation |
| `get_pages` | List pages | WordPress pages retrieval |
| `get_all_content` | All content | Combined posts + pages |
| `get_post` | Single post/page | By ID retrieval |
| `create_post` | Basic post creation | Title + content |
| `create_advanced_post` | Advanced creation | SEO, scheduling, taxonomy |
| `create_post_with_embeds` | Embedded content | YouTube, social media |
| `update_post` | Post modification | Structured updates |
| `delete_post` | Post removal | Trash or permanent |

## 🖼️ Media Management (7 tools)

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `upload_media` | File upload | Base64, accessibility metadata |
| `get_media` | List media | Library browsing with pagination |
| `get_media_item` | Single media | Detailed media information |
| `update_media` | Media metadata | Alt text, captions, descriptions |
| `delete_media` | Media removal | File deletion |
| `create_post_with_featured_image` | Post + image | Combined creation |
| `set_featured_image` | Featured image | Update existing posts |

## 📁 Taxonomy Management (8 tools)

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

## 🔗 Integration Tools (2 tools)

| Tool | Purpose | Key Features |
|------|---------|--------------|
| `create_post_with_categories_tags` | Post + taxonomy | Integrated creation |
| `update_post_categories_tags` | Taxonomy updates | Relationship management |

## Advanced Features

## Post Types & Status

**Supported Post Types:**
- `post` - Blog articles, news posts
- `page` - Static pages (About, Contact, etc.)

**Available Statuses:**
- `publish` - Public, live content
- `draft` - Work in progress
- `private` - Logged-in users only
- `future` - Scheduled publication

## SEO Integration

### Yoast SEO Support

```json
{
  "meta_description": "SEO meta description",
  "focus_keyword": "target keyword",
  "meta_robots_noindex": false,
  "meta_robots_nofollow": false
}
```

### Scheduled Publishing

```json
{
  "status": "future",
  "date": "2024-12-25T10:00:00Z"
}
```

## Embedded Content

### YouTube Integration

- **URL Formats**: `youtube.com/watch?v=`, `youtu.be/`, `youtube.com/embed/`
- **Auto iframe**: Generates proper embed code
- **Fallback**: WordPress oEmbed for unsupported URLs

### Social Media Support

- **Twitter/X**: oEmbed integration
- **Instagram**: Automatic post embedding
- **Facebook**: Post and video embedding
- **TikTok**: Video embedding

## Accessibility Features

### Media Accessibility

```json
{
  "alt_text": "Descriptive text for screen readers",
  "caption": "Visible caption text",
  "description": "Detailed image description"
}
```

### Best Practices

- Always provide alt text for images
- Use descriptive captions
- Include detailed descriptions for complex images
- Follow WCAG guidelines

## Usage Examples

## Complete Content Workflow

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

## Embedded Content Creation

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

## Media Management Workflow

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

## Common Error Types

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

## Retry Strategy

- **Exponential backoff**: Delays increase with each retry
- **Maximum attempts**: 3 retries by default
- **Timeout handling**: 30-second default timeout
- **Error logging**: Structured logging with tracing

## Testing & Examples

## Available Test Examples

```bash

## Basic functionality

cargo run --example wordpress_test

## Complete CRUD operations

cargo run --example wordpress_post_crud_test

## Media management with accessibility

cargo run --example wordpress_media_crud_test

## Embedded content creation

cargo run --example wordpress_embed_test

## Advanced post features (SEO, scheduling)

cargo run --example wordpress_advanced_post_test

## Taxonomy management

cargo run --example wordpress_categories_tags_test

## Integrated workflows

cargo run --example wordpress_posts_with_taxonomy_test
```

## Test Environment Setup

```bash

## Local WordPress with Docker

docker run -d \
  -p 8080:80 \
  -e WORDPRESS_DB_HOST=db \
  -e WORDPRESS_DB_PASSWORD=wordpress \
  wordpress:latest

## Or use existing WordPress site

## Configure in mcp-config.toml

```

## Performance Considerations

## Optimization Features

- **Connection pooling**: Reuse HTTP connections
- **Async operations**: Non-blocking I/O
- **Timeout handling**: Prevent hanging requests
- **Retry logic**: Handle transient failures
- **Structured logging**: Performance monitoring

## Best Practices

- **Batch operations**: Group related API calls
- **Pagination**: Handle large datasets efficiently
- **Caching**: Cache taxonomy and media metadata
- **Error handling**: Graceful degradation
- **Resource cleanup**: Proper connection management

## Security Considerations

## Authentication

- **Application Passwords**: WordPress-recommended method
- **Environment Variables**: Secure credential storage
- **No plaintext logging**: Credentials excluded from logs
- **HTTPS enforcement**: Secure transport required

## Input Validation

- **JSON schema**: Strict parameter validation
- **Type safety**: Rust compile-time checks
- **SQL injection prevention**: Parameterized queries
- **XSS protection**: Content sanitization

## Access Control

- **Principle of least privilege**: Minimal required permissions
- **User role verification**: WordPress role-based access
- **API endpoint restrictions**: Limited to required endpoints
- **Audit logging**: Track all operations

## Troubleshooting

## Common Issues

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

## Debug Mode

```toml
[handlers.wordpress]

## ... other settings

debug = true  

## Enable verbose logging

timeout_seconds = 60  

## Increase timeout

```

## Logging

```bash

## Set log level

RUST_LOG=debug cargo run

## Focus on WordPress handler

RUST_LOG=mcp_rs::handlers::wordpress=debug cargo run
```

## Contributing

## Adding New Tools

1. **Define tool schema** in `list_tools()`
2. **Implement handler** in `call_tool()`
3. **Add tests** in `examples/`
4. **Update documentation**

## WordPress API Extensions

1. **Research WordPress REST API** endpoints
2. **Design MCP tool interface**
3. **Implement with error handling**
4. **Test with real WordPress instance**
5. **Add comprehensive examples**

## Best Practices

- **Follow existing patterns** in codebase
- **Include accessibility features** for media
- **Provide comprehensive examples**
- **Test error conditions** thoroughly
- **Document all parameters** clearly

## 🤖 AI Agent Integration Guide

## Claude Desktop Configuration

To use MCP-RS with Claude Desktop, configure the MCP server connection:

### 1. Locate Claude Desktop Config File

**Windows:**
```
C:\Users\[username]\AppData\Roaming\Claude\claude_desktop_config.json
```

**macOS:**
```
~/Library/Application Support/Claude/claude_desktop_config.json
```

**Linux:**
```
~/.config/claude/claude_desktop_config.json
```

### 2. Add MCP-RS Configuration

Create or edit the config file with the following JSON:

```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:\\path\\to\\mcp-rs.exe",
      "args": [],
      "env": {
        "WORDPRESS_URL": "https://your-wordpress-site.com",
        "WORDPRESS_USERNAME": "your_username",
        "WORDPRESS_PASSWORD": "your_application_password"
      }
    }
  }
}
```

### 3. Important Configuration Notes

**Path Configuration:**
- Use double backslashes (`\\`) in Windows paths
- Enclose paths in double quotes
- Spaces in paths are supported within quotes

**Password Configuration:**
- Use WordPress Application Password (recommended)
- Passwords with spaces are supported
- **Do NOT remove spaces** from your actual password
- Environment variables provide secure credential storage

**Example with Spaces in Password:**
```json
"env": {
  "WORDPRESS_PASSWORD": "AbC1 2DeF 3GhI 4JkL"
}
```

### 4. MCP-RS Server Configuration

Ensure your `mcp-config.toml` is configured for STDIO mode:

```toml
[server]
stdio = true           

## Required for Claude Desktop

log_level = "info"

[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
enabled = true
timeout_seconds = 30
```

### 5. Restart Claude Desktop

After configuration:
1. **Close Claude Desktop completely**
2. **Restart the application**
3. **Verify MCP connection** in a new conversation
4. **Test with a simple command**: "List available WordPress tools"

## Testing AI Agent Connection

### Quick Connection Test

```
Please list all available WordPress tools and their descriptions.
```

### Resource Access Test

```
Can you read the wordpress://categories resource to show me all available categories?
```

### Tool Execution Test

```
Please get the list of WordPress categories using the get_categories tool.
```

## Expected AI Agent Capabilities

With proper configuration, AI agents can:

✅ **Content Management:**
- Create, read, update, delete posts and pages
- Manage categories and tags
- Handle media uploads and organization

✅ **Resource Access:**
- Read `wordpress://posts`, `wordpress://categories`, `wordpress://tags`
- Access structured content data via MCP resources
- Retrieve taxonomy and metadata information

✅ **Advanced Operations:**
- Bulk content operations
- SEO optimization and metadata management
- Comment moderation and user interaction
- Custom post type handling

## Troubleshooting AI Agent Issues

### "Cannot connect to MCP server"

1. **Verify file paths** in claude_desktop_config.json
2. **Check MCP-RS executable** location and permissions
3. **Ensure STDIO mode** is enabled in mcp-config.toml
4. **Restart Claude Desktop** after configuration changes

### "WordPress authentication failed"

1. **Verify Application Password** is correctly set
2. **Check username spelling** and capitalization
3. **Test credentials** manually via WordPress REST API
4. **Ensure user has appropriate permissions**

### "Tools not available"

1. **Confirm MCP connection** is established
2. **Check WordPress handler** is enabled in configuration
3. **Verify WordPress site** is accessible from MCP-RS server
4. **Review server logs** for detailed error information

## AI Agent Best Practices

### Prompt Engineering

```
When working with WordPress content:
1. Always specify target categories for new posts
2. Include SEO-friendly slugs and descriptions
3. Request bulk operations for efficiency
4. Verify content before publishing
```

### Resource Management

```
For large content operations:
1. Use wordpress://posts resource for content overview
2. Filter by categories or tags when possible
3. Batch similar operations together
4. Monitor API rate limits and timeouts
```

### Security Considerations

```
When using MCP-RS with AI agents:
1. Use Application Passwords (never regular passwords)
2. Limit user permissions to necessary functions only
3. Monitor audit logs for unusual activity
4. Regularly rotate Application Passwords
```
