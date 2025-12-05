#!/bin/bash
# Kubernetes deployment script for MCP-RS

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
K8S_DIR="$PROJECT_ROOT/k8s"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
NAMESPACE="mcp-rs"
IMAGE_TAG="${IMAGE_TAG:-latest}"
REGISTRY="${REGISTRY:-ghcr.io/n-takatsu/mcp-rs}"

# Functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

check_dependencies() {
    log_info "Checking dependencies..."
    
    if ! command -v kubectl &> /dev/null; then
        log_error "kubectl is not installed"
        exit 1
    fi
    
    if ! kubectl cluster-info &> /dev/null; then
        log_error "Cannot connect to Kubernetes cluster"
        exit 1
    fi
    
    log_info "All dependencies are available"
}

create_namespace() {
    log_step "Creating namespace..."
    
    if kubectl get namespace $NAMESPACE &> /dev/null; then
        log_info "Namespace $NAMESPACE already exists"
    else
        kubectl apply -f "$K8S_DIR/namespace.yaml"
        log_info "Namespace created"
    fi
}

create_secrets() {
    log_step "Creating secrets..."
    
    # Check if secrets exist
    if kubectl get secret mcp-rs-secrets -n $NAMESPACE &> /dev/null; then
        log_warn "Secrets already exist. Skipping..."
        return
    fi
    
    # Prompt for passwords
    read -sp "Enter PostgreSQL password: " POSTGRES_PASSWORD
    echo
    read -sp "Enter MySQL password: " MYSQL_PASSWORD
    echo
    
    # Create secrets
    kubectl create secret generic mcp-rs-secrets \
        --from-literal=POSTGRES_PASSWORD="$POSTGRES_PASSWORD" \
        --from-literal=MYSQL_ROOT_PASSWORD="$MYSQL_PASSWORD" \
        --from-literal=DATABASE_URL="postgresql://postgres:$POSTGRES_PASSWORD@postgres:5432/mcp_rs" \
        --from-literal=MYSQL_URL="mysql://root:$MYSQL_PASSWORD@mysql:3306/mcp_rs" \
        --from-literal=REDIS_URL="redis://redis:6379" \
        -n $NAMESPACE
    
    log_info "Secrets created"
}

create_tls_certs() {
    log_step "Creating TLS certificates..."
    
    if kubectl get secret mcp-rs-tls-certs -n $NAMESPACE &> /dev/null; then
        log_warn "TLS certificates already exist. Skipping..."
        return
    fi
    
    CERTS_DIR="$PROJECT_ROOT/certs"
    
    if [ ! -f "$CERTS_DIR/server.crt" ] || [ ! -f "$CERTS_DIR/server.key" ]; then
        log_error "TLS certificates not found in $CERTS_DIR"
        log_info "Please create certificates or use cert-manager"
        return 1
    fi
    
    kubectl create secret tls mcp-rs-tls-certs \
        --cert="$CERTS_DIR/server.crt" \
        --key="$CERTS_DIR/server.key" \
        -n $NAMESPACE
    
    log_info "TLS certificates created"
}

deploy_application() {
    log_step "Deploying application..."
    
    # Update image tag in deployment
    kubectl set image deployment/mcp-rs \
        mcp-rs="$REGISTRY:$IMAGE_TAG" \
        -n $NAMESPACE &> /dev/null || true
    
    # Apply manifests
    kubectl apply -f "$K8S_DIR/configmap.yaml"
    kubectl apply -f "$K8S_DIR/deployment.yaml"
    kubectl apply -f "$K8S_DIR/ingress.yaml"
    kubectl apply -f "$K8S_DIR/hpa.yaml"
    
    log_info "Application deployed"
}

wait_for_rollout() {
    log_step "Waiting for rollout to complete..."
    
    kubectl rollout status deployment/mcp-rs -n $NAMESPACE --timeout=5m
    
    log_info "Rollout completed"
}

health_check() {
    log_step "Performing health check..."
    
    # Get pod name
    POD_NAME=$(kubectl get pods -n $NAMESPACE -l app=mcp-rs -o jsonpath='{.items[0].metadata.name}')
    
    if [ -z "$POD_NAME" ]; then
        log_error "No pods found"
        return 1
    fi
    
    # Check health endpoint
    if kubectl exec -n $NAMESPACE "$POD_NAME" -- curl -f http://localhost:3000/health &> /dev/null; then
        log_info "Health check passed!"
        return 0
    else
        log_error "Health check failed"
        return 1
    fi
}

show_status() {
    log_step "Service status:"
    
    echo ""
    log_info "Pods:"
    kubectl get pods -n $NAMESPACE -l app=mcp-rs
    
    echo ""
    log_info "Services:"
    kubectl get svc -n $NAMESPACE
    
    echo ""
    log_info "Ingress:"
    kubectl get ingress -n $NAMESPACE
    
    echo ""
    log_info "HPA:"
    kubectl get hpa -n $NAMESPACE
}

show_logs() {
    kubectl logs -f deployment/mcp-rs -n $NAMESPACE
}

scale_deployment() {
    REPLICAS=$1
    
    if [ -z "$REPLICAS" ]; then
        log_error "Please specify number of replicas"
        exit 1
    fi
    
    log_step "Scaling deployment to $REPLICAS replicas..."
    kubectl scale deployment/mcp-rs --replicas=$REPLICAS -n $NAMESPACE
    log_info "Deployment scaled"
}

rollback() {
    log_step "Rolling back deployment..."
    kubectl rollout undo deployment/mcp-rs -n $NAMESPACE
    wait_for_rollout
    log_info "Rollback completed"
}

delete_deployment() {
    log_warn "This will delete all MCP-RS resources in namespace $NAMESPACE"
    read -p "Are you sure? (yes/no): " confirmation
    
    if [ "$confirmation" != "yes" ]; then
        log_info "Deletion cancelled"
        return
    fi
    
    log_step "Deleting deployment..."
    kubectl delete -f "$K8S_DIR/hpa.yaml" || true
    kubectl delete -f "$K8S_DIR/ingress.yaml" || true
    kubectl delete -f "$K8S_DIR/deployment.yaml" || true
    kubectl delete -f "$K8S_DIR/configmap.yaml" || true
    
    log_info "Deployment deleted"
}

# Parse command
COMMAND=${1:-help}

# Main
case $COMMAND in
    deploy)
        check_dependencies
        create_namespace
        create_secrets
        create_tls_certs
        deploy_application
        wait_for_rollout
        health_check
        show_status
        log_info "Deployment completed successfully!"
        ;;
    update)
        check_dependencies
        deploy_application
        wait_for_rollout
        health_check
        show_status
        ;;
    status)
        show_status
        ;;
    logs)
        show_logs
        ;;
    scale)
        scale_deployment $2
        ;;
    rollback)
        rollback
        ;;
    delete)
        delete_deployment
        ;;
    health)
        health_check
        ;;
    help|*)
        echo "MCP-RS Kubernetes Deployment Script"
        echo ""
        echo "Usage: $0 [command] [options]"
        echo ""
        echo "Commands:"
        echo "  deploy      - Full deployment (namespace, secrets, app)"
        echo "  update      - Update existing deployment"
        echo "  status      - Show deployment status"
        echo "  logs        - Show application logs"
        echo "  scale N     - Scale deployment to N replicas"
        echo "  rollback    - Rollback to previous version"
        echo "  delete      - Delete deployment"
        echo "  health      - Perform health check"
        echo "  help        - Show this help message"
        echo ""
        echo "Environment Variables:"
        echo "  IMAGE_TAG   - Docker image tag (default: latest)"
        echo "  REGISTRY    - Container registry (default: ghcr.io/n-takatsu/mcp-rs)"
        echo ""
        echo "Examples:"
        echo "  $0 deploy"
        echo "  IMAGE_TAG=v1.0.0 $0 update"
        echo "  $0 scale 5"
        echo "  $0 logs"
        ;;
esac
