# MCP-RS Development Container

This directory contains the complete Docker-based development environment for MCP-RS, using Docker Compose to orchestrate multiple services including databases and administration tools.

## Architecture

- **Main Container**: Custom Rust/Node.js development environment
- **PostgreSQL**: Primary database with initialization scripts
- **MySQL**: Secondary database for compatibility testing
- **MongoDB**: Document database for NoSQL features
- **Redis**: Cache and session storage
- **Adminer**: Web-based database administration
- **Redis Commander**: Redis management interface

## Features

- **Complete Stack**: All services run in isolated containers
- **Database Persistence**: Data volumes for all databases
- **Network Isolation**: Secure inter-service communication
- **Health Checks**: Automatic service health monitoring
- **Init Scripts**: Database schema and user setup
- **Administration Tools**: Web-based DB and cache management
- **Hot Reload**: Source code changes reflected immediately

## Quick Start

## Option 1: VS Code Dev Containers (Recommended)

1. Install Docker and Docker Compose
2. Open the project in VS Code
3. Install the "Dev Containers" extension
4. Press `Ctrl+Shift+P` → "Dev Containers: Reopen in Container"
5. Wait for all services to start and initialize
6. Start developing!

## Option 2: Standalone Docker Compose

1. Navigate to `.devcontainer` directory
2. Run `./start-dev.sh` to start all services
3. Access development environment: `docker-compose exec mcp-rs-dev zsh`

## Available Ports

- **3000**: MCP HTTP Server
- **8080**: Web UI
- **8081**: API Documentation
- **5432**: PostgreSQL
- **3306**: MySQL
- **27017**: MongoDB
- **6379**: Redis

## Development Aliases

The container comes with useful aliases for common tasks:

## Build & Test

- `mcpbuild` - Build the project with all features
- `mcptest` - Run all tests
- `mcpclippy` - Run clippy with strict warnings
- `mcpbench` - Run benchmarks
- `mcpaudit` - Run security audit

## Run Server

- `mcprun` - Run MCP server with STDIO transport
- `mcprunhttp` - Run MCP server with HTTP transport on port 3000

## Documentation

- `mcpdoc` - Generate and open documentation

## Maintenance

- `mcpupdate` - Update dependencies
- `mcpclean` - Clean build artifacts

## Navigation

- `cdmcp` - Go to project root
- `cdsrc` - Go to source directory
- `cdtest` - Go to tests directory
- `cdbench` - Go to benchmarks directory
- `cddocs` - Go to docs directory
- `cdweb` - Go to web-ui directory

## Database Setup

Run the database setup script to initialize development databases:

```bash
./scripts/setup-dev-db.sh
```

This will set up:
- PostgreSQL database `mcp_rs_dev`
- MySQL database `mcp_rs_dev`
- MongoDB (if available)
- Redis (if available)

## Environment Variables

The following environment variables are pre-configured:

- `RUST_BACKTRACE=1` - Enable full backtraces
- `RUST_LOG=debug` - Enable debug logging
- `DATABASE_URL` - PostgreSQL connection string
- `MYSQL_URL` - MySQL connection string
- `MONGODB_URL` - MongoDB connection string
- `REDIS_URL` - Redis connection string
- `MCP_SERVER_PORT=3000` - Default server port
- `MCP_LOG_LEVEL=debug` - Server log level

## VS Code Extensions

The container automatically installs these extensions:

## Rust Development

- `rust-lang.rust-analyzer` - Rust language server
- `tamasfe.even-better-toml` - Enhanced TOML support
- `serayuzgur.crates` - Cargo.toml dependency management
- `vadimcn.vscode-lldb` - Rust debugging support

## Database Tools

- `ms-vscode.vscode-json` - JSON support
- `ms-vscode.hexeditor` - Binary file editing

## Git & GitHub

- `github.vscode-pull-request-github` - GitHub integration
- `github.copilot` - AI-powered coding assistance
- `github.copilot-chat` - AI chat assistance

## Documentation

- `yzhang.markdown-all-in-one` - Markdown editing
- `davidanson.vscode-markdownlint` - Markdown linting

## Testing

- `hbenl.vscode-test-explorer` - Test runner integration
- `ms-vscode.test-adapter-converter` - Test adapter

## Customization

You can customize the development environment by:

1. **Adding VS Code settings**: Edit the `customizations.vscode.settings` section in `devcontainer.json`
2. **Installing additional tools**: Add commands to `setup.sh`
3. **Adding environment variables**: Add to the `containerEnv` section in `devcontainer.json`
4. **Forwarding additional ports**: Add to the `forwardPorts` array in `devcontainer.json`

## Troubleshooting

## Container Build Issues

If the container fails to build:
1. Try rebuilding without cache: `Dev Containers: Rebuild Without Cache`
2. Check the setup.sh script for any errors
3. Ensure Docker has enough resources allocated

## Database Connection Issues

If databases are not accessible:
1. Run `./scripts/setup-dev-db.sh` to initialize databases
2. Check if services are running: `sudo service postgresql status`
3. Verify environment variables are correctly set

## Performance Issues

If builds are slow:
1. Ensure cargo cache mounting is working
2. Use `CARGO_TARGET_DIR=/tmp/target` to build in RAM
3. Increase Docker memory allocation

## Contributing

When modifying the devcontainer configuration:
1. Test changes thoroughly
2. Update this README if adding new features
3. Consider backward compatibility
4. Document any new environment variables or aliases