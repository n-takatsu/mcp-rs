---
layout: page
title: Architecture Documentation
permalink: /docs/architecture/
---

# MCP-RS Architecture Documentation

This section contains detailed architectural documentation for the MCP-RS project.

## 📋 Main Architecture Document

For the complete and authoritative architecture documentation, please see:
**[Main Architecture Document](https://github.com/n-takatsu/mcp-rs/blob/main/project-docs/architecture.md)**

The document below provides a web-friendly overview of the current architecture status.

---

## Architecture Overview

MCP-RS follows a layered architecture pattern designed for extensibility, maintainability, and performance.

## Current Architecture Status

**Last Updated**: November 3, 2025
**Architecture Version**: v0.1.0-alpha
**Implementation Status**: Development Phase

## Layer Structure

## 1. Application Layer

- **Entry Point**: `main.rs` - Application initialization and CLI handling
- **Configuration**: Command-line argument parsing and environment setup
- **Lifecycle Management**: Application startup, shutdown, and signal handling

## 2. API Layer

- **MCP Protocol**: `mcp/` module - Core Model Context Protocol implementation
- **JSON-RPC**: Request/response handling and protocol compliance
- **Interface Definition**: Standardized API endpoints and message formats

## 3. Service Layer

- **Handlers**: `handlers/` module - Concrete feature implementations
  - ✅ WordPress Handler - Complete REST API integration
  - 🔄 Future handlers planned (GitHub, File System, Database)
- **Plugin System**: plugins run as separate MCP server processes, and mcp-rs relays their tools (`plugin_proxy`). See [Plugins](../plugins/)

## 4. Request Dispatch

- `McpServer` (`src/mcp/server.rs`) holds the registered handlers (WordPress and plugins) in registration order. It merges their tool lists and routes each `tools/call` by tool name
- STDIO mode reads requests from standard input. HTTP mode passes each HTTP or WebSocket request to the same logic through `RequestDispatcher`
- The earlier `core` module (`Runtime`, `HandlerRegistry`) was never connected to request handling and has been removed ([#409](https://github.com/n-takatsu/mcp-rs/issues/409))

## 5. Infrastructure Layer

- **Transport**: `transport/` module - Communication abstraction
  - 🔄 stdio transport (planned)
  - 🔄 HTTP server (in development)
  - 🔄 WebSocket support (planned)
- **Configuration**: `config/` module - TOML and environment-based config
- **Error Handling**: `error.rs` - Comprehensive error types and handling

## Implementation Progress

## ✅ Completed Components

### WordPress Handler (`handlers/wordpress.rs`)

- Full WordPress REST API integration
- Authentication with Application Passwords
- Timeout and retry mechanisms
- Comprehensive error handling
- Support for posts, pages, media, and user management

### Configuration System (`config/`)

- TOML-based configuration files
- Environment variable overrides
- Hierarchical configuration merging
- Type-safe configuration validation

### MCP Protocol Implementation (`mcp/`)

- JSON-RPC 2.0 compliance
- Core MCP message types
- Tool execution framework
- Resource management interface

### Error Handling (`error.rs`)

- Comprehensive error types using `thiserror`
- JSON-RPC error code mapping
- Structured error responses
- Debug and display implementations

## 🔄 In Development

### HTTP Transport

- Axum-based HTTP server
- Request routing and middleware
- Connection management
- Security headers and CORS

### Plugin Architecture

- Third-party plugins run as separate MCP server processes, and mcp-rs relays their tools as `<plugin>__<tool>` (`src/plugin_proxy.rs`, [#394](https://github.com/n-takatsu/mcp-rs/issues/394))
- Rate limiting, audit logging, timeouts, and response size limits are applied by mcp-rs, not by the plugin
- Stage 1 runs plugins as child processes without the server's environment variables. Stage 2 runs a plugin in a Docker container (`docker run -i`) with no network, no capabilities and a read-only root filesystem by default ([#411](https://github.com/n-takatsu/mcp-rs/issues/411)). Stage 3 runs a plugin as a Kubernetes Job through `kubectl`, with a non-root, read-only pod and a deny-all NetworkPolicy by default ([#413](https://github.com/n-takatsu/mcp-rs/issues/413))

## 📋 Planned Components

### Additional Transports

- **stdio**: For CLI and pipe-based communication
- **WebSocket**: For real-time bidirectional communication
- **Unix Sockets**: For local inter-process communication

### Advanced Features

- **Distributed Operation**: Multi-node coordination
- **Security Framework**: Authentication and authorization
- **Caching Layer**: Response caching and optimization
- **Monitoring**: Metrics collection and alerting

## Design Principles

## 1. Plugin-Oriented Architecture

- Unified interface through `McpHandler` trait
- Configuration-driven feature enablement
- Isolated execution contexts

## 2. Transport Abstraction

- Protocol-agnostic communication layer
- Pluggable transport implementations
- Unified message routing
- Connection lifecycle management

## 3. Configuration-Driven Design

- File-based configuration with environment overrides
- Hot-reload capability for configuration changes
- Type-safe configuration validation
- Hierarchical configuration merging

## 4. Async-First Approach

- tokio-based async runtime
- Non-blocking I/O operations
- Efficient resource utilization
- Timeout and cancellation support

## Future Roadmap

## Version 0.2.0 (Planned: December 2025)

- Complete core runtime implementation
- stdio transport support
- Basic plugin dynamic loading
- Enhanced error handling and recovery

## Version 0.3.0 (Planned: January 2026)

- WebSocket transport implementation
- Advanced plugin ecosystem
- Performance monitoring and metrics
- Comprehensive testing suite

## Version 1.0.0 (Planned: March 2026)

- Production-ready stability
- Complete documentation
- Security audit and hardening
- Ecosystem and community tools

## Technical Decisions

## ADR-001: Layered Architecture

- **Decision**: Adopt 4-layer architecture pattern
- **Rationale**: Clear separation of concerns and testability
- **Impact**: Improved maintainability and extensibility

## ADR-002: Plugin System Design

- **Decision**: Run each plugin as a separate MCP server process, and relay its tools through an `McpHandler` in mcp-rs ([#392](https://github.com/n-takatsu/mcp-rs/issues/392)). This replaces the earlier plan of loading native plugin libraries in-process
- **Rationale**: Third-party plugins must not be able to affect the core server or read its credentials. Native libraries run inside the server process with no isolation, and Rust has no stable ABI
- **Impact**: Plugins can be written in any language as MCP servers. Isolation can be strengthened in stages (child process, container, Kubernetes)

## ADR-003: Async Runtime Choice

- **Decision**: tokio as the primary async runtime
- **Rationale**: Mature ecosystem and performance characteristics
- **Impact**: High concurrency and efficient I/O handling

## ADR-004: Configuration Strategy

- **Decision**: TOML files with environment variable overrides
- **Rationale**: Human-readable config with deployment flexibility
- **Impact**: Easier configuration management and deployment

---

For more detailed technical specifications, see the [main architecture document](https://github.com/n-takatsu/mcp-rs/blob/main/project-docs/architecture.md).
