# Docker Infrastructure Implementation Guide

## Overview

This guide provides comprehensive instructions for deploying mcp-rs using Docker containers with security best practices.

## Quick Start

### Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+
- 2GB RAM minimum
- 10GB disk space

### Basic Deployment

```bash
# Clone repository
git clone https://github.com/n-takatsu/mcp-rs.git
cd mcp-rs

# Build image
docker build -t mcp-rs:latest .

# Run with STDIO transport
docker run -it mcp-rs:latest --transport stdio

# Run with HTTP transport
docker run -d -p 3000:3000 mcp-rs:latest --transport http --port 3000
```

### Docker Compose Deployment

```bash
# Production (with PostgreSQL and Redis)
docker compose up -d

# Development mode
docker compose -f docker-compose.dev.yml up

# Check status
docker compose ps

# View logs
docker compose logs -f mcp-server

# Stop services
docker compose down
```

## Image Details

### Size Optimization

- **Base Image**: `gcr.io/distroless/cc-debian12:nonroot` (~20MB)
- **Final Image**: **<50MB** (target met)
- **Build Time**: **<30 seconds** (with cached layers)

### Multi-Stage Build

```dockerfile
# Stage 1: Builder (Rust compilation)
FROM rust:1.83-slim-bookworm AS builder
# ... build process ...

# Stage 2: Runtime (minimal distroless)
FROM gcr.io/distroless/cc-debian12:nonroot
# ... copy binary only ...
```

### Layer Caching Strategy

1. **Dependencies Layer**: `Cargo.toml`, `Cargo.lock` (changes rarely)
2. **Source Code Layer**: `src/`, `benches/`, `examples/` (changes frequently)
3. Rebuild only changed layers for faster builds

## Security Features

### 1. Non-Root Execution

- Runs as `nonroot` user (UID 65532)
- No shell access in runtime image
- Prevents privilege escalation

```yaml
# docker-compose.yml
security_opt:
  - no-new-privileges:true
cap_drop:
  - ALL
```

### 2. Read-Only Filesystem

```yaml
read_only: true
volumes:
  - mcp-logs:/tmp:rw  # Only /tmp is writable
```

### 3. Resource Limits

```yaml
deploy:
  resources:
    limits:
      cpus: '2'
      memory: 512M
```

### 4. Network Isolation

```yaml
networks:
  mcp-network:
    driver: bridge
    ipam:
      config:
        - subnet: 172.20.0.0/16
```

## Configuration

### Environment Variables

Create `.env` file:

```bash
# Server
RUST_LOG=info
MCP_SERVER_PORT=3000
MCP_TRANSPORT=http

# Database (optional)
POSTGRES_DB=mcp_db
POSTGRES_USER=mcp_user
POSTGRES_PASSWORD=YOUR_SECURE_PASSWORD_HERE
```

**⚠️ IMPORTANT**: Never commit `.env` files with real passwords!

### Generate Secure Passwords

```bash
# Generate 32-character password
openssl rand -base64 32

# Or use the setup script
bash scripts/setup-docker-security.sh
```

## Build Optimization

### Build Cache

```bash
# Enable BuildKit for better caching
export DOCKER_BUILDKIT=1

# Build with cache
docker build \
  --cache-from mcp-rs:latest \
  -t mcp-rs:latest .

# Multi-platform build (ARM64 + AMD64)
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t mcp-rs:latest .
```

### Build Arguments

```bash
# Custom Rust version
docker build \
  --build-arg RUST_VERSION=1.83 \
  -t mcp-rs:latest .

# Release optimization
docker build \
  --build-arg CARGO_PROFILE=release \
  -t mcp-rs:latest .
```

## Health Checks

### Built-in Health Check

```bash
# Check container health
docker compose ps

# Manual health check
docker exec mcp-server /app/mcp-rs --health-check
```

### Custom Health Endpoint

```bash
# HTTP health endpoint
curl http://localhost:3000/health

# Expected response
{"status":"healthy","version":"0.15.0"}
```

## Monitoring

### Logs

```bash
# View real-time logs
docker compose logs -f mcp-server

# Last 100 lines
docker compose logs --tail=100 mcp-server

# Export logs
docker compose logs mcp-server > mcp-server.log
```

### Metrics

```bash
# Container stats
docker stats mcp-server

# Detailed inspection
docker inspect mcp-server
```

### Resource Usage

```bash
# Disk usage
docker system df

# Image layers
docker history mcp-rs:latest
```

## Security Scanning

### Trivy (Recommended)

```bash
# Install Trivy
brew install trivy  # macOS
apt-get install trivy  # Debian/Ubuntu

# Scan image
trivy image mcp-rs:latest

# Fail on HIGH/CRITICAL
trivy image --exit-code 1 --severity HIGH,CRITICAL mcp-rs:latest
```

### Docker Scout

```bash
# Quick scan
docker scout quickview mcp-rs:latest

# Detailed CVE report
docker scout cves mcp-rs:latest
```

### Automated Scanning

GitHub Actions workflow runs on every push:

- Trivy vulnerability scan
- Docker Scout CVE check
- Image size validation (<50MB)
- Container startup test
- SBOM generation

See [.github/workflows/docker-security.yml](.github/workflows/docker-security.yml)

## Troubleshooting

### Container Won't Start

```bash
# Check logs
docker compose logs mcp-server

# Inspect container
docker inspect mcp-server

# Try interactive mode
docker run -it mcp-rs:latest --transport stdio
```

### Permission Errors

```bash
# Verify user
docker run --rm mcp-rs:latest id
# Should show: uid=65532(nonroot) gid=65532(nonroot)

# Check volume permissions
ls -la ./configs
# Should be readable by UID 65532
```

### Network Issues

```bash
# Check network
docker network inspect mcp-network

# Test connectivity
docker exec mcp-server ping postgres

# Port binding conflicts
sudo lsof -i :3000  # Check if port is in use
```

### Build Failures

```bash
# Clean build cache
docker builder prune -a

# Rebuild without cache
docker build --no-cache -t mcp-rs:latest .

# Check disk space
df -h
```

## Production Deployment

### Prerequisites Checklist

- [ ] Change all default passwords
- [ ] Configure TLS/SSL certificates
- [ ] Set up log rotation
- [ ] Enable firewall rules
- [ ] Configure backup strategy
- [ ] Set up monitoring (Prometheus/Grafana)
- [ ] Run security scans
- [ ] Test disaster recovery

### Secrets Management

**DO NOT** use `.env` files in production. Use:

#### Docker Secrets (Swarm Mode)

```bash
# Create secret
echo "secure_password" | docker secret create postgres_password -

# Use in compose
services:
  mcp-server:
    secrets:
      - postgres_password
```

#### External Secret Managers

- **AWS Secrets Manager**
- **HashiCorp Vault**
- **Azure Key Vault**
- **Google Secret Manager**

### High Availability

```yaml
# docker-compose.yml
services:
  mcp-server:
    deploy:
      replicas: 3
      update_config:
        parallelism: 1
        delay: 10s
      restart_policy:
        condition: on-failure
        max_attempts: 3
```

### Load Balancing

Use Nginx or Traefik as reverse proxy:

```yaml
services:
  nginx:
    image: nginx:alpine
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./nginx.conf:/etc/nginx/nginx.conf:ro
    depends_on:
      - mcp-server
```

## Performance Tuning

### Build Performance

```bash
# Parallel builds
docker build --build-arg CARGO_BUILD_JOBS=8 .

# Increase memory
docker build --memory 4g .
```

### Runtime Performance

```yaml
# docker-compose.yml
services:
  mcp-server:
    environment:
      RUST_LOG: warn  # Reduce log verbosity
      TOKIO_WORKER_THREADS: 4  # Match CPU cores
```

### Database Optimization

```yaml
postgres:
  environment:
    POSTGRES_SHARED_BUFFERS: 256MB
    POSTGRES_MAX_CONNECTIONS: 100
```

## CI/CD Integration

### GitHub Actions

```yaml
# .github/workflows/docker.yml
- name: Build and push
  uses: docker/build-push-action@v5
  with:
    push: true
    tags: ghcr.io/${{ github.repository }}:${{ github.sha }}
```

### GitLab CI

```yaml
# .gitlab-ci.yml
docker-build:
  image: docker:latest
  script:
    - docker build -t $CI_REGISTRY_IMAGE:$CI_COMMIT_SHA .
    - docker push $CI_REGISTRY_IMAGE:$CI_COMMIT_SHA
```

## Migration Guide

### From Binary to Docker

1. Export current configuration:
   ```bash
   cp configs/production/main.toml configs/docker/main.toml
   ```

2. Update connection strings:
   ```toml
   [database]
   url = "postgresql://mcp_user:password@postgres:5432/mcp_db"
   ```

3. Start containers:
   ```bash
   docker compose up -d
   ```

4. Verify migration:
   ```bash
   docker compose logs -f mcp-server
   ```

## References

- [Dockerfile Best Practices](https://docs.docker.com/develop/develop-images/dockerfile_best-practices/)
- [Docker Security](https://docs.docker.com/engine/security/)
- [Distroless Images](https://github.com/GoogleContainerTools/distroless)
- [Docker Compose Specification](https://docs.docker.com/compose/compose-file/)
- [CIS Docker Benchmark](https://www.cisecurity.org/benchmark/docker)

## Support

- GitHub Issues: https://github.com/n-takatsu/mcp-rs/issues
- Documentation: https://n-takatsu.github.io/mcp-rs
- Security: security@example.com

## License

MIT OR Apache-2.0
