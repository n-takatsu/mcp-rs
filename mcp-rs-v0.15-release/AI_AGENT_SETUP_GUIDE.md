# MCP-RS AI Agent Setup Guide

## 🤖 Claude Desktop Integration

### 1. Quick Configuration

**Step 1: Locate Claude Config File**
```
Windows: C:\Users\[username]\AppData\Roaming\Claude\claude_desktop_config.json
macOS: ~/Library/Application Support/Claude/claude_desktop_config.json
Linux: ~/.config/claude/claude_desktop_config.json
```

**Step 2: Add MCP-RS Configuration**
```json
{
  "mcpServers": {
    "mcp-rs": {
      "command": "C:\\path\\to\\mcp-rs.exe",
      "args": [],
      "env": {
        "WORDPRESS_URL": "https://your-site.com",
        "WORDPRESS_USERNAME": "your_username",
        "WORDPRESS_PASSWORD": "your_app_password"
      }
    }
  }
}
```

**Step 3: Update MCP-RS Config**
Ensure `mcp-config.toml` has:
```toml
[server]
stdio = true

[handlers.wordpress]
url = "${WORDPRESS_URL}"
username = "${WORDPRESS_USERNAME}"
password = "${WORDPRESS_PASSWORD}"
enabled = true
```

### 2. Important Notes

✅ **Password Configuration:**
- Use WordPress Application Password (Admin → Users → Application Passwords)
- Spaces in passwords are supported - **do NOT remove them**
- Example: `"AbC1 2DeF 3GhI 4JkL"` (keep spaces as-is)

✅ **Path Configuration:**
- Windows: Use double backslashes (`\\`) in JSON
- Paths with spaces: Keep within quotes
- Example: `"C:\\Program Files\\mcp-rs\\mcp-rs.exe"`

### 3. Test Connection

**Restart Claude Desktop** after configuration, then try:

```
"List available WordPress tools"
```

```
"Show me WordPress categories using wordpress://categories resource"
```

### 4. Quick Troubleshooting

❌ **"Cannot connect"** → Check file paths and restart Claude Desktop  
❌ **"Auth failed"** → Verify Application Password and username  
❌ **"No tools"** → Ensure `stdio = true` in mcp-config.toml  

### 5. Advanced Features

Once connected, AI agents can:
- 📝 Create and manage WordPress posts/pages
- 🏷️ Access categories and tags via MCP resources
- 📊 Perform bulk content operations
- 🔍 Search and filter content efficiently

For detailed documentation, see: `project-docs/wordpress-guide.md`