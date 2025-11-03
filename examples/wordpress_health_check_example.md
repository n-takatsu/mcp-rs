# WordPress Health Check Example

This example demonstrates how to use the WordPress health check functionality to verify your environment before using other WordPress MCP tools.

## Running the Health Check

```bash
# Make sure your WordPress configuration is set up in mcp-config.toml
cargo run --example wordpress_health_check
```

## Example Configuration

```toml
[wordpress]
url = "https://your-wordpress-site.com"
username = "your_username"
password = "your_application_password"
timeout_seconds = 30
```

## Health Check Results

The health check will verify:

### ✅ **Site Accessibility**
- Can connect to WordPress site
- Site responds to requests
- Basic connectivity test

### ✅ **REST API Availability**
- WordPress REST API v2 is enabled
- API endpoints are accessible
- Proper API namespace detection

### ✅ **Authentication Validity**
- Application password is correct
- User credentials are valid
- Authentication endpoint responds correctly

### ✅ **Permission Adequacy**
- User can read posts
- User can access media library
- Sufficient privileges for content management

### ✅ **Media Upload Capability**
- Media endpoint is accessible
- Upload permissions are available
- File upload functionality is ready

## Example Output

```
✅ WordPress Health Check: HEALTHY

🌐 Site: My WordPress Site (https://example.com)
📝 Description: Just another WordPress site

📊 Health Status:
  • Site Accessible: ✅
  • REST API Available: ✅
  • Authentication Valid: ✅
  • Permissions Adequate: ✅
  • Media Upload Possible: ✅
```

## Troubleshooting

### Common Issues

**❌ Site Accessibility: FAILED**
- Check if WordPress URL is correct
- Verify site is online and accessible
- Check network connectivity

**❌ REST API Available: FAILED**
- WordPress REST API might be disabled
- Check if permalink structure is set (not "Plain")
- Verify .htaccess file is properly configured

**❌ Authentication Valid: FAILED**
- Verify application password is correct
- Check username matches WordPress user
- Ensure user account is active

**❌ Permissions Adequate: FAILED**
- User needs Editor or Administrator role
- Check user has publish_posts capability
- Verify user can upload files

**❌ Media Upload Possible: FAILED**
- Check file upload permissions on server
- Verify upload_max_filesize is adequate
- Check WordPress media upload settings

## MCP Tool Usage

When using MCP, you can run the health check with:

```json
{
  "tool": "wordpress_health_check",
  "arguments": {}
}
```

The AI will automatically interpret the results and provide guidance on any issues found.