# API Reference

Complete API documentation for MCP-RS.

## Core Types

### JSON-RPC Messages

All MCP-RS communication follows the JSON-RPC 2.0 specification.

#### Request Format
```json
{
  "jsonrpc": "2.0",
  "method": "method_name",
  "params": { /* method parameters */ },
  "id": 1
}
```

#### Response Format
```json
{
  "jsonrpc": "2.0",
  "result": { /* response data */ },
  "id": 1
}
```

#### Error Format
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32000,
    "message": "Error description",
    "data": { /* additional error info */ }
  },
  "id": 1
}
```

## MCP Protocol Methods

### Initialization

#### `initialize`
Establishes connection and negotiates capabilities.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "roots": {
        "listChanged": true
      },
      "sampling": {}
    },
    "clientInfo": {
      "name": "client-name",
      "version": "1.0.0"
    }
  },
  "id": 1
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "protocolVersion": "2024-11-05",
    "capabilities": {
      "logging": {},
      "tools": {
        "listChanged": true
      },
      "resources": {
        "subscribe": true,
        "listChanged": true
      },
      "prompts": {
        "listChanged": true
      }
    },
    "serverInfo": {
      "name": "mcp-rs",
      "version": "0.1.0"
    }
  },
  "id": 1
}
```

### Tools

#### `tools/list`
Lists available tools.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tools/list",
  "id": 2
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "tools": [
      {
        "name": "wordpress_get_posts",
        "description": "Retrieve WordPress posts",
        "inputSchema": {
          "type": "object",
          "properties": {
            "per_page": {
              "type": "number",
              "description": "Number of posts to retrieve",
              "default": 10
            },
            "status": {
              "type": "string",
              "description": "Post status",
              "enum": ["publish", "draft", "private"],
              "default": "publish"
            }
          }
        }
      }
    ]
  },
  "id": 2
}
```

#### `tools/call`
Executes a tool.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "tools/call",
  "params": {
    "name": "wordpress_get_posts",
    "arguments": {
      "per_page": 5,
      "status": "publish"
    }
  },
  "id": 3
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "content": [
      {
        "type": "text",
        "text": "Retrieved 5 posts:\n1. Sample Post Title\n2. Another Post\n..."
      }
    ],
    "isError": false
  },
  "id": 3
}
```

### Resources

#### `resources/list`
Lists available resources.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "resources/list",
  "id": 4
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "resources": [
      {
        "uri": "wordpress://posts",
        "name": "WordPress Posts",
        "description": "All published WordPress posts",
        "mimeType": "application/json"
      },
      {
        "uri": "wordpress://pages",
        "name": "WordPress Pages",
        "description": "All published WordPress pages",
        "mimeType": "application/json"
      }
    ]
  },
  "id": 4
}
```

#### `resources/read`
Reads a resource.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "resources/read",
  "params": {
    "uri": "wordpress://posts"
  },
  "id": 5
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "contents": [
      {
        "uri": "wordpress://posts",
        "mimeType": "application/json",
        "text": "[{\"id\":1,\"title\":\"Sample Post\",\"content\":\"...\"}]"
      }
    ]
  },
  "id": 5
}
```

### Prompts

#### `prompts/list`
Lists available prompts.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "prompts/list",
  "id": 6
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "prompts": [
      {
        "name": "wordpress_content_analysis",
        "description": "Analyze WordPress content for SEO and readability",
        "arguments": [
          {
            "name": "post_id",
            "description": "WordPress post ID to analyze",
            "required": true
          }
        ]
      }
    ]
  },
  "id": 6
}
```

#### `prompts/get`
Retrieves a prompt.

**Request:**
```json
{
  "jsonrpc": "2.0",
  "method": "prompts/get",
  "params": {
    "name": "wordpress_content_analysis",
    "arguments": {
      "post_id": "123"
    }
  },
  "id": 7
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "description": "WordPress content analysis prompt",
    "messages": [
      {
        "role": "user",
        "content": {
          "type": "text",
          "text": "Please analyze the following WordPress post for SEO optimization and readability improvements..."
        }
      }
    ]
  },
  "id": 7
}
```

## Error Codes

MCP-RS uses standard JSON-RPC error codes plus custom MCP-specific codes:

| Code | Message | Description |
|------|---------|-------------|
| -32700 | Parse error | Invalid JSON |
| -32600 | Invalid Request | Invalid JSON-RPC request |
| -32601 | Method not found | Method doesn't exist |
| -32602 | Invalid params | Invalid method parameters |
| -32603 | Internal error | Internal JSON-RPC error |
| -32000 | Server error | Generic server error |
| -32001 | Invalid tool | Tool not found or invalid |
| -32002 | Tool execution failed | Tool execution error |
| -32003 | Resource not found | Resource URI not found |
| -32004 | Resource read failed | Resource read error |
| -32005 | Configuration error | Configuration validation error |

## WordPress Handler API

### Available Tools

#### `wordpress_get_posts`
Retrieves WordPress posts with filtering options.

**Parameters:**
- `per_page` (number, optional): Number of posts (default: 10, max: 100)
- `page` (number, optional): Page number for pagination (default: 1)
- `status` (string, optional): Post status filter (default: "publish")
- `author` (number, optional): Author ID filter
- `search` (string, optional): Search term
- `categories` (array, optional): Category ID filter
- `tags` (array, optional): Tag ID filter

#### `wordpress_create_post`
Creates a new WordPress post.

**Parameters:**
- `title` (string, required): Post title
- `content` (string, required): Post content (HTML)
- `status` (string, optional): Post status (default: "draft")
- `categories` (array, optional): Category IDs
- `tags` (array, optional): Tag IDs
- `featured_media` (number, optional): Featured image ID

#### `wordpress_update_post`
Updates an existing WordPress post.

**Parameters:**
- `id` (number, required): Post ID
- `title` (string, optional): Post title
- `content` (string, optional): Post content (HTML)
- `status` (string, optional): Post status
- `categories` (array, optional): Category IDs
- `tags` (array, optional): Tag IDs

#### `wordpress_delete_post`
Deletes a WordPress post.

**Parameters:**
- `id` (number, required): Post ID
- `force` (boolean, optional): Bypass trash (default: false)

#### `wordpress_get_media`
Retrieves WordPress media items.

**Parameters:**
- `per_page` (number, optional): Number of items (default: 10)
- `media_type` (string, optional): Media type filter
- `search` (string, optional): Search term

#### `wordpress_upload_media`
Uploads media to WordPress.

**Parameters:**
- `file_data` (string, required): Base64 encoded file data
- `filename` (string, required): Original filename
- `title` (string, optional): Media title
- `alt_text` (string, optional): Alternative text

### Resources

- `wordpress://posts` - All published posts
- `wordpress://pages` - All published pages  
- `wordpress://media` - Media library items
- `wordpress://categories` - Post categories
- `wordpress://tags` - Post tags
- `wordpress://users` - Site users

---

For implementation examples, see the [Guides section](../guides/).