# Plugin System Documentation

MCP-RS provides a powerful dynamic plugin system that allows you to extend functionality at runtime.

## Quick Start

The best way to understand the plugin system is through our **executable documentation examples**. These examples are automatically tested to ensure they work correctly:

### 🔗 Core Plugin APIs

- **[Plugin Discovery](../src/plugins/mod.rs#L10-L23)** - Basic plugin discovery from directories
- **[Plugin Registry Usage](../src/plugins/mod.rs#L30-L54)** - Complete plugin registry lifecycle
- **[Plugin Loader](../src/plugins/loader.rs#L9-L25)** - Dynamic plugin loading fundamentals
- **[Search Path Management](../src/plugins/loader.rs#L33-L52)** - Managing plugin search paths

### 🔗 Configuration Examples

- **[Basic Configuration](../src/config.rs#L10-L16)** - Default MCP configuration setup
- **[Plugin Configuration](../src/config.rs#L21-L43)** - Advanced plugin configuration

## Architecture Overview

```
┌─────────────────────┐    ┌─────────────────────┐
│   Plugin Registry   │────│   Plugin Loader     │
│                     │    │                     │
│ - Discovery         │    │ - Dynamic Loading   │
│ - Lifecycle Mgmt    │    │ - Search Paths      │
│ - Dependency Res.   │    │ - Safety Checks     │
└─────────────────────┘    └─────────────────────┘
           │                           │
           └───────────┬───────────────┘
                       │
          ┌─────────────────────┐
          │   Handler Registry  │
          │                     │
          │ - Handler Management│
          │ - Integration       │
          └─────────────────────┘
```

## Running the Examples

All code examples in this documentation are executable and tested:

```bash
# Test all documentation examples
cargo test --doc

# Test specific module examples
cargo test --doc plugins
cargo test --doc config

# Generate and view documentation locally
cargo doc --open
```

## Plugin Development Guide

### Creating a Plugin

1. **Implement the DynamicPlugin trait** (see [plugin traits documentation](../src/plugins/types.rs))
2. **Create a plugin manifest** (see manifest examples in tests)
3. **Build as dynamic library** with proper exports
4. **Test integration** using the examples above

### Plugin Manifest Format

```toml
[metadata]
name = "my_plugin"
version = "0.1.0"
description = "My custom MCP plugin"
author = "Your Name"

[entry_points]
wordpress = "create_wordpress_handler"

[dependencies]
mcp_rs = "0.1"

[build]
abi_version = "1.0"
```

## API Reference

For detailed API documentation with live examples, run:

```bash
cargo doc --open
```

This will generate comprehensive documentation with all the executable examples integrated inline.

## Testing Your Plugin

Use our plugin integration test patterns:

```bash
# Run plugin-specific tests
cargo test plugin_integration_tests
cargo test simple_tests

# Run all tests including examples
cargo test
```

---

**📝 Note**: All code examples in this documentation are automatically verified through `cargo test --doc`. If an example doesn't work, the CI will catch it immediately, ensuring documentation accuracy.