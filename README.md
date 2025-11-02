# mcp-rs

**mcp-rs** is a Rust implementation of the [Model Context Protocol (MCP)](https://learn.microsoft.com/en-us/microsoft-copilot-studio/mcp-overview), designed to enable AI agents—such as Copilot Studio—to interact with external systems like WordPress via JSON-RPC.

This project aims to provide a type-safe, extensible, and performant MCP server in Rust, with initial support for WordPress REST API integration.

## License

This project is licensed under the [MIT License](./LICENSE).

## Features

- JSON-RPC 2.0 compliant server
- WordPress post/comment/media/user operations
- Copilot Studio integration-ready
- Modular architecture for future protocol extensions

## Getting Started

```bash
cargo build
cargo run
