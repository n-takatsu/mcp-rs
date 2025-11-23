# Contributing to MCP-RS

Thank you for your interest in contributing to MCP-RS! We welcome contributions from the community.

## Current Projects

### ✅ MySQL Phase 1 (Complete - Nov 2025)

Parameterized queries, transaction management, and SQL injection prevention for MySQL.

**Key Components**:

- Prepared statements for safe parameterized queries
- ACID-compliant transaction management
- 4 isolation levels support
- Savepoint functionality
- 45 comprehensive tests

**Related Files**:

- `src/handlers/database/engines/mysql/prepared.rs`
- `src/handlers/database/engines/mysql/transaction.rs`
- `tests/mysql_phase1_*.rs`

**For more info**: See [MySQL Phase 1 Guide](./docs/mysql-phase1-guide.md)

### 🚧 Future Projects

- **Phase 2**: PostgreSQL backend (Q1 2026)
- **Phase 3**: Redis & SQLite support (Q2-Q3 2026)
- **WebSocket**: Real-time communication (Q2 2026)
- **AI Integration**: LLM direct integration (Q3 2026)

## Prerequisites

- Rust 1.70+
- Cargo
- Git

## Development Setup

1. Fork and clone the repository:
   ```bash
   git clone https://github.com/your-username/mcp-rs.git
   cd mcp-rs
   ```

2. Create a new branch for your feature:
   ```bash
   git checkout -b feature/your-feature-name
   ```

3. Run tests to ensure everything works:
   ```bash
   cargo test
   ```

## Development Guidelines

## Code Style

- Follow Rust standard formatting with `rustfmt`
- Use `clippy` to catch common mistakes
- Write comprehensive tests for new functionality
- Add documentation for public APIs

## Testing

- Run the full test suite: `cargo test`
- Run specific tests: `cargo test test_name`
- Check code coverage and ensure new code is tested

## Security

- Follow secure coding practices
- Test security-related changes thoroughly
- Report security vulnerabilities privately to the maintainers

## Submitting Changes

## Pull Request Process

1. Ensure your code follows the project's coding standards
2. Add tests for new functionality
3. Update documentation as needed
4. Ensure all tests pass
5. Submit a pull request with a clear description

## Commit Messages

Use clear, descriptive commit messages:
- `feat: add new WordPress media upload functionality`
- `fix: resolve timeout issue in HTTP client`
- `docs: update API documentation`
- `test: add integration tests for security features`

## Reporting Issues

## Bug Reports

When reporting bugs, please include:
- OS and Rust version
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs or error messages

## Feature Requests

For feature requests, please describe:
- The use case and problem being solved
- Proposed solution or approach
- Any alternatives considered

## Code of Conduct

This project follows the Rust Code of Conduct. Please be respectful and inclusive in all interactions.

## Questions?

Feel free to open an issue for questions or join the discussion in existing issues.

## License

By contributing to MCP-RS, you agree that your contributions will be licensed under the same license as the project (MIT OR Apache-2.0).
