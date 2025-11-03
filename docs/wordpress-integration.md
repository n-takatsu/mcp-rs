# WordPress Integration Guide

Complete guide for using MCP-RS with WordPress through the REST API.

## 🔗 Quick Reference (Tested Examples)

**All examples below link to executable, automatically-tested code:**

### Configuration Examples
- **[Basic WordPress Setup](../src/config.rs#L21-L43)** - WordPress configuration with MCP-RS
- **[Environment Variables](../src/config.rs#L132-L178)** - Secure environment variable handling

### WordPress Handler Examples  
- **[Handler Initialization](../tests/integration_tests.rs)** - WordPress handler setup and testing

## WordPress REST API Integration

MCP-RS provides 27 WordPress tools covering the complete WordPress ecosystem:

### 📝 Content Management
- **Posts**: Create, read, update, delete with full metadata support
- **Pages**: Static content management with hierarchical relationships  
- **Drafts & Publishing**: Complete status control (draft → review → publish)
- **Scheduling**: Future publication with ISO8601 timestamps

### 🖼️ Media Management
- **File Uploads**: Base64 and multipart form support
- **Media Library**: Full CRUD operations with metadata
- **Featured Images**: Automatic association with posts/pages
- **Accessibility**: Alt text, captions, and descriptions

### 🏷️ Taxonomy Management
- **Categories**: Hierarchical organization with parent-child relationships
- **Tags**: Flat taxonomy for flexible content organization
- **Custom Taxonomies**: Support for theme and plugin taxonomies

### 🎬 Rich Content
- **YouTube Embeds**: Automatic video embedding with metadata
- **Social Media**: oEmbed integration for Twitter, Instagram, etc.
- **Custom Embeds**: Support for various content providers

## Configuration

### Basic Setup

```toml
[handlers.wordpress]
url = "https://your-site.com"
username = "your-username"  
password = "${WP_APP_PASSWORD}"  # Application password
enabled = true
timeout_seconds = 30
```

### Environment Variables

```bash
export WP_APP_PASSWORD="your-application-password"
export WP_USERNAME="your-username"
export WP_SITE_URL="https://your-site.com"
```

### Advanced Configuration

For production environments, see our **[tested configuration examples](../src/config.rs)** which demonstrate:

- Environment variable expansion
- Security best practices  
- Error handling patterns
- Timeout configuration

## WordPress Application Passwords

MCP-RS uses WordPress Application Passwords for secure authentication:

### Creating Application Passwords

1. **WordPress Admin** → **Users** → **Your Profile**
2. Scroll to **Application Passwords** section
3. Enter application name: "MCP-RS Integration"
4. Click **Add New Application Password**
5. Copy the generated password immediately

### Security Best Practices

- ✅ Store passwords in environment variables
- ✅ Use descriptive application names
- ✅ Regularly rotate passwords
- ✅ Monitor authentication logs
- ❌ Never commit passwords to version control

## Available WordPress Tools

### Content Tools (12 tools)
- `wordpress_get_posts` - Retrieve posts with filtering
- `wordpress_create_post` - Create new posts
- `wordpress_update_post` - Update existing posts  
- `wordpress_delete_post` - Delete posts
- `wordpress_get_pages` - Retrieve pages
- `wordpress_create_page` - Create new pages
- `wordpress_update_page` - Update existing pages
- `wordpress_delete_page` - Delete pages
- `wordpress_get_post_revisions` - Post history
- `wordpress_restore_post_revision` - Restore previous versions
- `wordpress_get_post_status` - Check publication status
- `wordpress_set_post_status` - Change publication status

### Media Tools (7 tools)
- `wordpress_upload_media` - Upload files
- `wordpress_get_media` - Retrieve media metadata
- `wordpress_update_media` - Update media information
- `wordpress_delete_media` - Remove media files
- `wordpress_set_featured_image` - Associate images with posts
- `wordpress_get_media_sizes` - Available image sizes
- `wordpress_regenerate_thumbnails` - Recreate image variations

### Taxonomy Tools (8 tools)
- `wordpress_get_categories` - List categories
- `wordpress_create_category` - Create new categories
- `wordpress_update_category` - Modify categories
- `wordpress_delete_category` - Remove categories
- `wordpress_get_tags` - List tags
- `wordpress_create_tag` - Create new tags
- `wordpress_update_tag` - Modify tags
- `wordpress_delete_tag` - Remove tags

## Error Handling

WordPress integration includes comprehensive error handling:

### Authentication Errors
- Invalid credentials → Clear error messages
- Expired passwords → Renewal instructions
- Permission denied → Capability requirements

### Network Errors  
- Connection timeouts → Configurable retry logic
- Rate limiting → Exponential backoff
- Server errors → Detailed error context

### Content Errors
- Validation failures → Field-specific feedback
- Duplicate content → Conflict resolution options
- Missing dependencies → Requirement guidance

## Testing Your Integration

Run the WordPress integration tests:

```bash
# Test WordPress handler functionality
cargo test wordpress

# Test with actual WordPress site (requires configuration)
WP_TEST_URL="https://test-site.com" \
WP_TEST_USER="test-user" \
WP_TEST_PASSWORD="test-password" \
cargo test integration_tests

# Test all documentation examples
cargo test --doc
```

## Troubleshooting

### Common Issues

**Authentication Failed**
- Verify application password is correct
- Check username matches WordPress user
- Ensure user has required permissions

**Connection Timeout**
- Increase timeout in configuration
- Check network connectivity
- Verify WordPress site is accessible

**Permission Denied**
- Verify user role has required capabilities
- Check WordPress REST API is enabled
- Confirm endpoint permissions

### Debug Mode

Enable debug logging:

```bash
RUST_LOG=debug cargo run
```

## Advanced Features

### Custom Post Types
WordPress integration supports custom post types created by themes and plugins.

### Multisite Support  
Basic multisite support with network-level operations.

### Plugin Integration
Compatible with major WordPress plugins:
- **Yoast SEO**: Meta field support
- **WooCommerce**: Product management
- **Advanced Custom Fields**: Custom field handling

---

**📝 Note**: All code examples in this guide link to executable tests in the repository. Run `cargo test --doc` to verify examples work correctly.