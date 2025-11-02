# API Reference

Complete API documentation for MCP-RS components.

## Core Components

### Protocol Implementation
- [MCP Protocol Types](protocol.html) - Core MCP protocol implementation
- [Error Types](errors.html) - Comprehensive error handling
- [Transport Layer](transport.html) - STDIO and TCP transport implementations

### Server Components
- [MCP Server](server.html) - Main server implementation
- [Request Handling](request-handling.html) - Request routing and processing
- [Response Management](response-management.html) - Response formatting and delivery

### Plugin System
- [Plugin Registry](plugins.html) - Plugin management and lifecycle
- [Tool Providers](tool-providers.html) - Tool implementation interface
- [Resource Providers](resource-providers.html) - Resource access interface
- [Prompt Providers](prompt-providers.html) - Prompt template system

### Configuration
- [Configuration Types](config.html) - Configuration structure and options
- [Environment Integration](environment.html) - Environment variable handling
- [Validation](validation.html) - Configuration validation

### Logging
- [Logger Interface](logging.html) - Structured logging implementation
- [Request Context](request-context.html) - Request correlation and tracing
- [Error Context](error-context.html) - Error tracking and debugging

## SDK Components

### Development Helpers
- [Plugin Macros](sdk/macros.html) - Code generation utilities
- [Testing Utilities](sdk/testing.html) - Test helpers and fixtures
- [Helper Functions](sdk/helpers.html) - Common utility functions

## Type Definitions

All types are documented with their full structure, validation rules, and usage examples.

### Error Handling

The API uses a comprehensive error system with structured error types for different failure scenarios:

- `McpError::ParseError` - JSON parsing failures
- `McpError::ValidationError` - Input validation failures
- `McpError::PluginError` - Plugin execution errors
- `McpError::TransportError` - Communication errors
- `McpError::ConfigurationError` - Configuration issues

### Async Patterns

All I/O operations are async and return `Result` types for proper error handling.