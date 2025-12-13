# MCP-Server Helm Chart

Production-ready Helm Chart for deploying MCP-RS Server with High Availability, auto-scaling, and comprehensive monitoring.

## Features

- ✅ **High Availability**: Multi-replica deployment with pod anti-affinity
- ✅ **Auto-scaling**: HPA based on CPU/memory metrics
- ✅ **Zero-downtime Updates**: Rolling updates with PodDisruptionBudget
- ✅ **Security**: securityContext, NetworkPolicy, read-only filesystem
- ✅ **Monitoring**: Prometheus ServiceMonitor and alerting rules
- ✅ **Backup**: Automated CronJob for data backup
- ✅ **Canary Deployment**: Traffic splitting for gradual rollout
- ✅ **Environment-specific**: Separate values for dev/staging/prod

## Quick Start

### Installation

```bash
# Add repository (if published)
helm repo add mcp https://n-takatsu.github.io/mcp-rs
helm repo update

# Install with default values
helm install mcp-server mcp/mcp-server -n mcp-production --create-namespace

# Or install from local chart
helm install mcp-server ./charts/mcp-server -n mcp-production --create-namespace
```

### Environment-specific Deployment

```bash
# Development
helm install mcp-dev charts/mcp-server \
  -f charts/mcp-server/values-dev.yaml \
  -n mcp-dev --create-namespace

# Staging
helm install mcp-staging charts/mcp-server \
  -f charts/mcp-server/values-staging.yaml \
  -n mcp-staging --create-namespace

# Production
helm install mcp-prod charts/mcp-server \
  -f charts/mcp-server/values-production.yaml \
  -n mcp-production --create-namespace
```

## Configuration

### Key Parameters

| Parameter | Description | Default |
|-----------|-------------|---------|
| `replicaCount` | Number of replicas | `3` |
| `image.repository` | Image repository | `ghcr.io/n-takatsu/mcp-rs` |
| `image.tag` | Image tag | `0.15.0` |
| `resources.limits.cpu` | CPU limit | `2000m` |
| `resources.limits.memory` | Memory limit | `4Gi` |
| `autoscaling.enabled` | Enable HPA | `true` |
| `autoscaling.minReplicas` | Min replicas | `3` |
| `autoscaling.maxReplicas` | Max replicas | `10` |
| `ingress.enabled` | Enable Ingress | `false` |
| `monitoring.enabled` | Enable monitoring | `true` |

### Full Configuration

See [values.yaml](./values.yaml) for all available options.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                       Ingress                           │
│              (nginx with TLS & rate limit)              │
└───────────────────────┬─────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────────┐
│                      Service                            │
│                  (ClusterIP/LoadBalancer)               │
└───────────────────────┬─────────────────────────────────┘
                        │
        ┌───────────────┼───────────────┐
        │               │               │
        ▼               ▼               ▼
    ┌────────┐      ┌────────┐      ┌────────┐
    │  Pod 1 │      │  Pod 2 │      │  Pod N │
    │        │      │        │      │        │
    │ mcp-rs │      │ mcp-rs │      │ mcp-rs │
    └────┬───┘      └────┬───┘      └────┬───┘
         │               │               │
         └───────────────┴───────────────┘
                        │
                        ▼
            ┌───────────────────────┐
            │  PersistentVolume     │
            │  (data & backups)     │
            └───────────────────────┘
```

## Values Files

- **values.yaml**: Default production-ready configuration
- **values-dev.yaml**: Development environment (1 replica, debug mode)
- **values-staging.yaml**: Staging environment (2-5 replicas, moderate HA)
- **values-production.yaml**: Full production (5-20 replicas, strict security)
- **values-canary.yaml**: Canary deployment (10% traffic split)

## Advanced Usage

### Custom Values

Create your own `my-values.yaml`:

```yaml
replicaCount: 5

image:
  tag: "0.16.0"

resources:
  limits:
    cpu: 4000m
    memory: 8Gi
  requests:
    cpu: 1000m
    memory: 2Gi

config:
  logLevel: warn
  rateLimit:
    requestsPerSecond: 5000
    burstSize: 10000
```

Deploy with custom values:

```bash
helm install mcp-server charts/mcp-server \
  -f my-values.yaml \
  -n mcp-production
```

### Secrets Management

#### Manual Secrets

```bash
kubectl create secret generic mcp-server-secrets \
  --from-literal=API_KEY=your-key \
  --from-literal=DB_PASSWORD=your-password \
  -n mcp-production

helm install mcp-server charts/mcp-server \
  --set envFrom[0].secretRef.name=mcp-server-secrets \
  -n mcp-production
```

#### External Secrets Operator

```yaml
secrets:
  enabled: true
  externalSecrets:
    enabled: true
    backendType: secretsManager
```

### Monitoring Setup

#### Prerequisites

```bash
# Install Prometheus Operator
helm install prometheus-operator prometheus-community/kube-prometheus-stack \
  -n monitoring --create-namespace
```

#### Enable Monitoring

```yaml
monitoring:
  enabled: true
  serviceMonitor:
    enabled: true
    interval: 30s
  prometheusRules:
    enabled: true
    rules:
      - alert: HighErrorRate
        expr: rate(http_errors_total[5m]) > 0.05
        for: 5m
```

### Canary Deployment

```bash
# 1. Deploy canary with 10% traffic
helm install mcp-canary charts/mcp-server \
  -f charts/mcp-server/values-canary.yaml \
  --set image.tag=0.16.0 \
  -n mcp-production

# 2. Monitor metrics
kubectl top pods -n mcp-production -l deployment-type=canary

# 3. Increase traffic to 50%
helm upgrade mcp-canary charts/mcp-server \
  --set ingress.annotations."nginx\.ingress\.kubernetes\.io/canary-weight"=50 \
  --reuse-values \
  -n mcp-production

# 4. Full rollout
helm upgrade mcp-prod charts/mcp-server \
  --set image.tag=0.16.0 \
  -n mcp-production

# 5. Remove canary
helm uninstall mcp-canary -n mcp-production
```

## Testing

### Chart Validation

```bash
# Lint chart
helm lint charts/mcp-server

# Template rendering
helm template mcp-server charts/mcp-server

# Dry-run
helm install mcp-server charts/mcp-server --dry-run --debug
```

### Helm Test

```bash
# Run built-in tests
helm test mcp-server -n mcp-production

# Expected output:
# NAME: mcp-server
# LAST DEPLOYED: ...
# NAMESPACE: mcp-production
# STATUS: deployed
# TEST SUITE:     mcp-server-test-connection
# Last Started:   ...
# Last Completed: ...
# Phase:          Succeeded
```

## Upgrade

```bash
# Check upgrade diff
helm diff upgrade mcp-server charts/mcp-server \
  -f values-production.yaml \
  -n mcp-production

# Upgrade release
helm upgrade mcp-server charts/mcp-server \
  -f values-production.yaml \
  --set image.tag=0.16.0 \
  -n mcp-production \
  --wait --timeout 10m
```

## Rollback

```bash
# View history
helm history mcp-server -n mcp-production

# Rollback to previous version
helm rollback mcp-server -n mcp-production

# Rollback to specific revision
helm rollback mcp-server 5 -n mcp-production
```

## Uninstall

```bash
# Uninstall release
helm uninstall mcp-server -n mcp-production

# Delete PVCs (if needed)
kubectl delete pvc -n mcp-production -l app.kubernetes.io/instance=mcp-server
```

## Troubleshooting

### Common Issues

#### Pod not starting

```bash
kubectl describe pod -n mcp-production <pod-name>
kubectl logs -n mcp-production <pod-name>
```

#### HPA not scaling

```bash
# Check metrics server
kubectl top nodes
kubectl get apiservice v1beta1.metrics.k8s.io

# HPA status
kubectl describe hpa mcp-server -n mcp-production
```

#### Ingress not working

```bash
# Check ingress
kubectl describe ingress mcp-server -n mcp-production

# Ingress controller logs
kubectl logs -n ingress-nginx -l app.kubernetes.io/component=controller
```

### Debug Mode

```bash
# Enable debug logging
helm upgrade mcp-server charts/mcp-server \
  --set config.logLevel=debug \
  --reuse-values \
  -n mcp-production
```

## Best Practices

### Production Checklist

- ✅ Use resource limits and requests
- ✅ Enable autoscaling (HPA)
- ✅ Configure Pod Disruption Budget
- ✅ Enable monitoring and alerting
- ✅ Use read-only root filesystem
- ✅ Enable network policies
- ✅ Configure backup CronJob
- ✅ Use TLS for Ingress
- ✅ Store secrets securely (External Secrets)
- ✅ Test in staging before production

### Security Hardening

```yaml
securityContext:
  runAsNonRoot: true
  runAsUser: 1000
  fsGroup: 1000
  seccompProfile:
    type: RuntimeDefault

containerSecurityContext:
  allowPrivilegeEscalation: false
  readOnlyRootFilesystem: true
  capabilities:
    drop:
      - ALL

networkPolicy:
  enabled: true
```

## Documentation

- [Production Deployment Guide](../../docs/PRODUCTION_DEPLOYMENT_GUIDE.md)
- [Kubernetes Guide](../../docs/KUBERNETES_GUIDE.md)
- [Main README](../../README.md)

## Support

- GitHub Issues: https://github.com/n-takatsu/mcp-rs/issues
- Discussions: https://github.com/n-takatsu/mcp-rs/discussions

## License

Apache License 2.0 / MIT License
