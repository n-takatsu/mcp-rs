# MCP-RS

**mcp-rs** is a high-performance Rust implementation of the [Model Context Protocol (MCP)](https://learn.microsoft.com/en-us/microsoft-copilot-studio/mcp-overview), designed to enable AI agents—such as Copilot Studio—to interact with external systems like WordPress via JSON-RPC.

This project aims to provide a type-safe, extensible, and performant MCP server in Rust, with comprehensive logging, plugin architecture, and production-ready features.

## 📖 Documentation

-   **[📚 Complete Documentation](https://redring2020.github.io/mcp-rs/)** - GitHub Pages site
-   **[🚀 Getting Started Guide](https://redring2020.github.io/mcp-rs/website/docs/guides/getting-started.html)** - Quick setup
-   **[📝 API Reference](https://redring2020.github.io/mcp-rs/website/docs/api/)** - Complete API docs
-   **[🏗️ Architecture Overview](https://redring2020.github.io/mcp-rs/website/docs/architecture/overview.html)** - System design
-   **[📊 Logging Strategy](https://redring2020.github.io/mcp-rs/website/docs/architecture/logging.html)** - Operational monitoring

## Features

-   🚀 **JSON-RPC 2.0 compliant server** - Full MCP protocol implementation
-   📝 **WordPress integration** - Post/comment/media/user operations
-   🤖 **Copilot Studio ready** - Seamless AI agent integration
-   🔧 **Modular architecture** - Easy to extend with new protocols
-   ⚡ **High performance** - Built with Rust and Tokio
-   🔒 **Type-safe** - Comprehensive error handling and validation

## Quick Start

### Prerequisites

-   Rust 1.70+
-   WordPress site with REST API enabled (optional for testing)

### Installation

```bash
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs
cargo build --release
```

### Configuration

1. Copy the example environment file:

```bash
cp .env.example .env
```

2. Edit `.env` with your WordPress configuration:

```bash
WORDPRESS_URL=https://your-wordpress-site.com
WORDPRESS_USERNAME=your_username
WORDPRESS_PASSWORD=your_password
```

### Running the Server

#### For MCP clients (stdio mode):

```bash
MCP_STDIO=1 cargo run
```

#### For development (TCP mode):

```bash
cargo run
```

The server will listen on `127.0.0.1:8080` by default.

## Usage Examples

### Testing WordPress Integration

```bash
cargo run --example wordpress_test
```

### Available Tools

The WordPress handler provides the following tools:

-   **get_posts** - Retrieve WordPress posts
-   **create_post** - Create new WordPress posts
-   **get_comments** - Retrieve WordPress comments

### Available Resources

-   **wordpress://posts** - Access to all WordPress posts
-   **wordpress://comments** - Access to all WordPress comments

## Development

### Project Structure

```
src/
├── mcp/           # Core MCP protocol implementation
│   ├── types.rs   # MCP type definitions
│   ├── server.rs  # MCP server implementation
│   └── error.rs   # Error handling
├── handlers/      # Protocol handlers
│   └── wordpress.rs # WordPress REST API handler
└── main.rs        # Application entry point
```

### Adding New Handlers

1. Create a new handler in `src/handlers/`:

```rust
use async_trait::async_trait;
use crate::mcp::{McpHandler, McpError, Tool, Resource};

pub struct MyHandler {
    // Handler fields
}

#[async_trait]
impl McpHandler for MyHandler {
    // Implement required methods
}
```

2. Register the handler in `main.rs`:

```rust
let my_handler = MyHandler::new();
server.add_handler("my_handler".to_string(), Box::new(my_handler));
```

### Testing

Run the test suite:

```bash
cargo test
```

Run with logging:

```bash
RUST_LOG=debug cargo test
```

## Integration with Copilot Studio

To use this MCP server with Copilot Studio:

1. Configure the server to run in stdio mode
2. Set up the MCP client configuration in Copilot Studio
3. Point to the compiled binary with `MCP_STDIO=1`

Example configuration:

```json
{
    "mcpServers": {
        "wordpress": {
            "command": "path/to/mcp-rs",
            "env": {
                "MCP_STDIO": "1",
                "WORDPRESS_URL": "https://your-site.com",
                "WORDPRESS_USERNAME": "username",
                "WORDPRESS_PASSWORD": "password"
            }
        }
    }
}
```

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

### Development Setup

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Add tests if applicable
5. Submit a pull request

## License

This project is licensed under the [MIT License](./LICENSE).

## Roadmap

-   [ ] Additional WordPress operations (media, users, taxonomies)
-   [ ] Support for other CMS platforms
-   [ ] WebSocket transport support
-   [ ] Advanced authentication methods
-   [ ] Caching layer for improved performance
-   [ ] Comprehensive test suite
-   [ ] Documentation improvements
