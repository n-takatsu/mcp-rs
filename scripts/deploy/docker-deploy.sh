#!/bin/bash
# Docker Compose deployment script for MCP-RS

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

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

check_dependencies() {
    log_info "Checking dependencies..."
    
    if ! command -v docker &> /dev/null; then
        log_error "Docker is not installed"
        exit 1
    fi
    
    if ! command -v docker-compose &> /dev/null; then
        log_error "Docker Compose is not installed"
        exit 1
    fi
    
    log_info "All dependencies are installed"
}

check_env_file() {
    if [ ! -f "$PROJECT_ROOT/.env" ]; then
        log_warn ".env file not found, creating from example..."
        cat > "$PROJECT_ROOT/.env" <<EOF
# Database
POSTGRES_PASSWORD=changeme
MYSQL_ROOT_PASSWORD=changeme

# Application
RUST_LOG=info
MCP_SERVER_PORT=3000
EOF
        log_info "Created .env file. Please update the passwords!"
    fi
}

build_image() {
    log_info "Building Docker image..."
    cd "$PROJECT_ROOT"
    docker build -t mcp-rs:latest .
    log_info "Docker image built successfully"
}

start_services() {
    log_info "Starting services..."
    cd "$PROJECT_ROOT"
    
    if [ "$WITH_NGINX" = "true" ]; then
        docker-compose --profile with-nginx up -d
    else
        docker-compose up -d
    fi
    
    log_info "Services started"
}

stop_services() {
    log_info "Stopping services..."
    cd "$PROJECT_ROOT"
    docker-compose down
    log_info "Services stopped"
}

restart_services() {
    log_info "Restarting services..."
    stop_services
    start_services
    log_info "Services restarted"
}

show_logs() {
    cd "$PROJECT_ROOT"
    docker-compose logs -f
}

health_check() {
    log_info "Performing health check..."
    
    max_attempts=30
    attempt=0
    
    while [ $attempt -lt $max_attempts ]; do
        if curl -f http://localhost:3000/health &> /dev/null; then
            log_info "Health check passed!"
            return 0
        fi
        
        attempt=$((attempt + 1))
        log_info "Waiting for service to be ready... ($attempt/$max_attempts)"
        sleep 2
    done
    
    log_error "Health check failed after $max_attempts attempts"
    return 1
}

show_status() {
    log_info "Service status:"
    cd "$PROJECT_ROOT"
    docker-compose ps
}

# Parse arguments
COMMAND=${1:-help}
WITH_NGINX=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --with-nginx)
            WITH_NGINX=true
            shift
            ;;
        *)
            shift
            ;;
    esac
done

# Main
case $COMMAND in
    deploy)
        check_dependencies
        check_env_file
        build_image
        start_services
        health_check
        show_status
        log_info "Deployment completed successfully!"
        ;;
    start)
        check_dependencies
        start_services
        health_check
        show_status
        ;;
    stop)
        stop_services
        ;;
    restart)
        restart_services
        health_check
        show_status
        ;;
    build)
        build_image
        ;;
    logs)
        show_logs
        ;;
    status)
        show_status
        ;;
    health)
        health_check
        ;;
    help|*)
        echo "MCP-RS Docker Deployment Script"
        echo ""
        echo "Usage: $0 [command] [options]"
        echo ""
        echo "Commands:"
        echo "  deploy      - Build and deploy all services"
        echo "  start       - Start all services"
        echo "  stop        - Stop all services"
        echo "  restart     - Restart all services"
        echo "  build       - Build Docker image"
        echo "  logs        - Show service logs"
        echo "  status      - Show service status"
        echo "  health      - Perform health check"
        echo "  help        - Show this help message"
        echo ""
        echo "Options:"
        echo "  --with-nginx    Include Nginx reverse proxy"
        echo ""
        echo "Examples:"
        echo "  $0 deploy"
        echo "  $0 deploy --with-nginx"
        echo "  $0 start"
        echo "  $0 logs"
        ;;
esac
