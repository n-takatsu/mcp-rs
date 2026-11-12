---
layout: page
title: Documentation
permalink: /docs/
---

# MCP-RS Documentation

MCP-RS is a Model Context Protocol (MCP) server written in Rust. MCP clients such as Claude Desktop and Claude Code connect to it over STDIO, and other clients over HTTP.

## What it offers

- **[WordPress]({{ site.baseurl }}/docs/wordpress/)**: 32 tools for posts, pages, media, categories, tags, comments, and site settings, through the WordPress REST API
- **[Database]({{ site.baseurl }}/docs/database/)**: Read-only SQL tools for one PostgreSQL database (writes only with `allow_write = true`)
- **[Plugins]({{ site.baseurl }}/docs/plugins/)**: The tools of other MCP servers, started as a child process, in a Docker container, or as a Kubernetes Job
- **[Security]({{ site.baseurl }}/docs/security/)**: Encrypted credentials; for HTTP mode, API key or OAuth 2.1 authentication, per-caller tool limits, and TLS/mTLS; rate limiting; an audit log of every tool call
- **`mcp_rs_status`**: A built-in tool that shows the version, the config file, the log files, and what failed to start

## Documentation sections

- **[Getting Started]({{ site.baseurl }}/docs/guides/)** - Build, configure, and connect a client
- **[WordPress]({{ site.baseurl }}/docs/wordpress/)** - WordPress tools and settings
- **[Database]({{ site.baseurl }}/docs/database/)** - PostgreSQL tools and settings
- **[Plugins]({{ site.baseurl }}/docs/plugins/)** - Add tools from other MCP servers
- **[Security]({{ site.baseurl }}/docs/security/)** - Credentials, HTTP authentication, tool limits, TLS, audit log
- **[API Reference]({{ site.baseurl }}/docs/api/)** - MCP methods and `mcp_rs_status`
- **[Architecture]({{ site.baseurl }}/docs/architecture/)** - How a request travels through mcp-rs

## Contributing

See the [contributing guidelines](https://github.com/n-takatsu/mcp-rs/blob/main/CONTRIBUTING.md).

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](https://github.com/n-takatsu/mcp-rs/blob/main/LICENSE-APACHE))
- MIT license ([LICENSE-MIT](https://github.com/n-takatsu/mcp-rs/blob/main/LICENSE-MIT))

at your option.
