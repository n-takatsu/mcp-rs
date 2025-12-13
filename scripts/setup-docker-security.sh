# ========================================
# Docker Security Configuration Script
# ========================================
# This script generates secure configurations for Docker deployment

# Environment file template
ENV_FILE=".env.production"

cat > "$ENV_FILE" << 'EOF'
# ========================================
# Production Environment Configuration
# ========================================

# Server Configuration
RUST_LOG=info
MCP_SERVER_PORT=3000
MCP_TRANSPORT=http

# Database Configuration (PostgreSQL)
POSTGRES_DB=mcp_db
POSTGRES_USER=mcp_user
POSTGRES_PASSWORD=CHANGE_THIS_PASSWORD_IN_PRODUCTION

# Redis Configuration
REDIS_MAX_MEMORY=100mb
REDIS_EVICTION_POLICY=allkeys-lru

# Security Settings
# Generate secure passwords using: openssl rand -base64 32
# POSTGRES_PASSWORD=$(openssl rand -base64 32)

EOF

echo "✅ Created $ENV_FILE"
echo "⚠️  IMPORTANT: Change POSTGRES_PASSWORD before deploying!"

# .env template for development
DEV_ENV_FILE=".env.development"

cat > "$DEV_ENV_FILE" << 'EOF'
# ========================================
# Development Environment Configuration
# ========================================

# Server Configuration
RUST_LOG=debug
RUST_BACKTRACE=1
MCP_SERVER_PORT=3000
MCP_TRANSPORT=http

# Database Configuration (PostgreSQL)
POSTGRES_DB=mcp_dev
POSTGRES_USER=dev_user
POSTGRES_PASSWORD=dev_password

# Redis Configuration
REDIS_MAX_MEMORY=50mb

EOF

echo "✅ Created $DEV_ENV_FILE"

# Docker security best practices document
SECURITY_DOC="docs/docker-security-guide.md"

mkdir -p docs

cat > "$SECURITY_DOC" << 'EOF'
# Docker Security Guide

## Overview

This guide outlines security best practices implemented in mcp-rs Docker configuration.

## Security Features

### 1. Non-Root User Execution

- **Distroless Base Image**: Uses `gcr.io/distroless/cc-debian12:nonroot`
- **UID 65532**: Default non-root user (nonroot)
- **No Shell Access**: Distroless images contain only runtime dependencies

```dockerfile
USER nonroot:nonroot
```

### 2. Minimal Attack Surface

- **Distroless Image**: ~20MB smaller than alpine, no package manager
- **Read-Only Root Filesystem**: Prevents runtime modifications
- **Dropped Capabilities**: Removes all unnecessary Linux capabilities

```yaml
security_opt:
  - no-new-privileges:true
read_only: true
cap_drop:
  - ALL
cap_add:
  - NET_BIND_SERVICE  # Only if binding to port < 1024
```

### 3. Resource Limits

Prevents DoS attacks and resource exhaustion:

```yaml
deploy:
  resources:
    limits:
      cpus: '2'
      memory: 512M
    reservations:
      cpus: '0.5'
      memory: 128M
```

### 4. Network Isolation

- **Bridge Network**: Isolated container network
- **Subnet Configuration**: 172.20.0.0/16
- **No Host Network**: Prevents direct host access

### 5. Secrets Management

**DO NOT** commit secrets to version control. Use:

#### Docker Secrets (Swarm)

```bash
echo "secure_password" | docker secret create postgres_password -
```

#### Environment Variables (Compose)

```bash
# .env file (gitignored)
POSTGRES_PASSWORD=$(openssl rand -base64 32)
```

#### External Secret Managers

- AWS Secrets Manager
- HashiCorp Vault
- Azure Key Vault

## Security Scanning

### Container Image Scanning

#### Trivy (Recommended)

```bash
# Install Trivy
brew install trivy  # macOS
# or
apt-get install trivy  # Debian/Ubuntu

# Scan Docker image
docker build -t mcp-rs:latest .
trivy image mcp-rs:latest

# Fail on HIGH/CRITICAL vulnerabilities
trivy image --exit-code 1 --severity HIGH,CRITICAL mcp-rs:latest
```

#### Docker Scout

```bash
# Enable Docker Scout
docker scout quickview mcp-rs:latest

# Detailed CVE report
docker scout cves mcp-rs:latest

# Compare with base image
docker scout compare --to gcr.io/distroless/cc-debian12:nonroot mcp-rs:latest
```

#### Grype

```bash
# Install Grype
curl -sSfL https://raw.githubusercontent.com/anchore/grype/main/install.sh | sh

# Scan image
grype mcp-rs:latest
```

### Runtime Security

#### Falco (Runtime Security)

```bash
# Install Falco
helm repo add falcosecurity https://falcosecurity.github.io/charts
helm install falco falcosecurity/falco

# Monitor container behavior
kubectl logs -f -n falco -l app=falco
```

## Vulnerability Mitigation

### High-Priority Actions

1. **Regular Updates**: Rebuild images weekly
2. **Base Image Pinning**: Use specific image digests
3. **Multi-Stage Builds**: Separate build and runtime dependencies
4. **Dependency Auditing**: `cargo audit` for Rust dependencies

### Example: Pinned Base Image

```dockerfile
FROM rust:1.83-slim-bookworm@sha256:abc123... AS builder
FROM gcr.io/distroless/cc-debian12:nonroot@sha256:def456...
```

### Dependency Scanning

```bash
# Install cargo-audit
cargo install cargo-audit

# Check for known vulnerabilities
cargo audit

# Generate advisory database
cargo audit --deny warnings
```

## Production Deployment Checklist

- [ ] Change all default passwords
- [ ] Use Docker secrets or external secret manager
- [ ] Enable read-only root filesystem
- [ ] Drop all unnecessary capabilities
- [ ] Set resource limits (CPU, memory)
- [ ] Use specific image tags (avoid `latest`)
- [ ] Enable health checks
- [ ] Configure log rotation
- [ ] Implement network policies
- [ ] Run security scans (Trivy, Docker Scout)
- [ ] Enable runtime security monitoring (Falco)
- [ ] Review firewall rules
- [ ] Configure TLS/SSL certificates
- [ ] Enable audit logging

## Monitoring and Alerting

### Health Checks

```yaml
healthcheck:
  test: ["CMD-SHELL", "/app/mcp-rs --health-check || exit 1"]
  interval: 30s
  timeout: 3s
  retries: 3
  start_period: 10s
```

### Log Aggregation

Use structured logging with log forwarding:

- **ELK Stack**: Elasticsearch, Logstash, Kibana
- **Loki**: Grafana Loki for log aggregation
- **Fluentd**: Log collection and forwarding

### Metrics

Expose Prometheus metrics:

```rust
// In your application
use prometheus::{Encoder, TextEncoder, Registry};

let registry = Registry::new();
// Register metrics...
```

## Incident Response

1. **Isolate**: Stop affected containers
2. **Investigate**: Review logs and metrics
3. **Remediate**: Apply patches, update images
4. **Verify**: Scan updated images
5. **Redeploy**: Use canary or blue-green deployment

## References

- [OWASP Container Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/Container_Security_Cheat_Sheet.html)
- [CIS Docker Benchmark](https://www.cisecurity.org/benchmark/docker)
- [Docker Security Best Practices](https://docs.docker.com/develop/security-best-practices/)
- [Distroless Containers](https://github.com/GoogleContainerTools/distroless)

EOF

echo "✅ Created $SECURITY_DOC"

echo ""
echo "=========================================="
echo "Security configuration complete!"
echo "=========================================="
echo ""
echo "Next steps:"
echo "1. Review and update passwords in .env.production"
echo "2. Run security scan: docker build -t mcp-rs:latest . && trivy image mcp-rs:latest"
echo "3. Read security guide: docs/docker-security-guide.md"
echo ""
