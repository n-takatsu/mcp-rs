# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha] - 2025-11-04

### Added
- **WordPress Integration**: Complete WordPress REST API integration with 27 tools
  - Advanced post/page management with SEO integration
  - Complete media management with base64 upload support
  - Category and tag management with hierarchical support
  - YouTube and social media embed support
  - Comment management and retrieval
- **Enterprise Security**: 6-layer security architecture (100% implemented)
  - AES-GCM-256 encryption with PBKDF2 key derivation
  - SQL injection protection (11 attack patterns)
  - XSS attack prevention (14 attack patterns)
  - Token bucket rate limiting with DDoS protection
  - TLS 1.2+ enforcement
  - Comprehensive audit logging
- **Core Infrastructure**: 
  - JSON-RPC 2.0 server implementation using axum
  - Type-safe TOML configuration with environment variable override
  - Comprehensive error handling with thiserror
  - Async/await support with tokio runtime
  - Production-ready logging with tracing
- **Documentation**: 
  - Complete README with usage examples
  - Technical documentation in project-docs/
  - GitHub Pages website preparation
  - Contributing guidelines and code of conduct
- **Testing**: 205+ comprehensive tests with 100% pass rate
- **Security Features**:
  - Zero-panic operations with Result-based error handling
  - Safe environment variable expansion with infinite loop prevention
  - Application password lifecycle management
  - Production monitoring and health checks

### Security
- Implemented military-grade AES-GCM-256 encryption
- Added comprehensive input validation and sanitization
- Enabled zero-trust data validation architecture
- Implemented real-time security monitoring

### Technical
- Built with Rust 2021 edition for memory safety
- Async-first architecture using tokio
- Clean layered architecture with separation of concerns
- Production-optimized build profiles

### Documentation
- Comprehensive API documentation for all 27 WordPress tools
- Security implementation guide with examples
- Architecture documentation with design decisions
- Complete setup and deployment guides

## [0.0.0] - 2025-10-01

### Added
- Initial project setup
- Basic project structure
- License files (MIT/Apache-2.0)

[Unreleased]: https://github.com/n-takatsu/mcp-rs/compare/v0.1.0-alpha...HEAD
[0.1.0-alpha]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.1.0-alpha
[0.0.0]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.0.0