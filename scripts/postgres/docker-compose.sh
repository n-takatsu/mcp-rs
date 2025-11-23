#!/bin/bash
# Docker Compose PostgreSQL environment helper script

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="${SCRIPT_DIR}/docker-compose.postgres.yml"
ENV_FILE="${SCRIPT_DIR}/.env"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Functions
print_usage() {
    cat <<EOF
Usage: $0 <command> [options]

Commands:
  up              Start Docker Compose environment
  down            Stop Docker Compose environment
  restart         Restart containers
  logs            Show container logs
  ps              Show container status
  test-connect    Test database connections
  init-data       Initialize with sample data
  clean           Remove all containers and volumes
  help            Show this help message

Examples:
  $0 up
  $0 logs -f postgres-primary
  $0 test-connect
  $0 down

EOF
}

print_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $1"
}

# Load environment
load_env() {
    if [ -f "$ENV_FILE" ]; then
        set -a
        source "$ENV_FILE"
        set +a
        print_info "Loaded environment from $ENV_FILE"
    else
        print_warning ".env file not found, using defaults"
    fi
}

# Start environment
cmd_up() {
    print_info "Starting Docker Compose environment..."
    docker-compose -f "$COMPOSE_FILE" up -d
    print_info "Environment started"
    print_info "Waiting for services to be healthy..."
    sleep 10
    cmd_ps
}

# Stop environment
cmd_down() {
    print_info "Stopping Docker Compose environment..."
    docker-compose -f "$COMPOSE_FILE" down
    print_info "Environment stopped"
}

# Restart environment
cmd_restart() {
    print_info "Restarting Docker Compose environment..."
    docker-compose -f "$COMPOSE_FILE" restart
    print_info "Environment restarted"
}

# Show logs
cmd_logs() {
    local service="${1:-}"
    if [ -n "$service" ]; then
        docker-compose -f "$COMPOSE_FILE" logs -f "$service"
    else
        docker-compose -f "$COMPOSE_FILE" logs -f
    fi
}

# Show status
cmd_ps() {
    print_info "Container status:"
    docker-compose -f "$COMPOSE_FILE" ps
}

# Test database connections
cmd_test_connect() {
    load_env
    
    local POSTGRES_USER="${POSTGRES_USER:-postgres}"
    local POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-postgres}"
    local POSTGRES_DB="${POSTGRES_DB:-testdb}"
    
    print_info "Testing PostgreSQL Primary connection..."
    if psql -h localhost -p 5432 -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
            -c "SELECT version();" 2>/dev/null; then
        print_info "✓ Primary connection successful"
    else
        print_error "✗ Primary connection failed"
        return 1
    fi
    
    print_info "Testing PostgreSQL Secondary connection..."
    if psql -h localhost -p 5433 -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
            -c "SELECT version();" 2>/dev/null; then
        print_info "✓ Secondary connection successful"
    else
        print_warning "✗ Secondary connection failed (may be normal if replication not ready)"
    fi
    
    print_info "Testing database schema..."
    psql -h localhost -p 5432 -U "$POSTGRES_USER" -d "$POSTGRES_DB" \
         -c "\dt test_schema.*" 2>/dev/null || true
    
    print_info "All connection tests completed"
}

# Initialize with sample data
cmd_init_data() {
    load_env
    
    local POSTGRES_USER="${POSTGRES_USER:-postgres}"
    local POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-postgres}"
    local POSTGRES_DB="${POSTGRES_DB:-testdb}"
    
    print_info "Initializing sample data..."
    
    psql -h localhost -p 5432 -U "$POSTGRES_USER" -d "$POSTGRES_DB" <<EOF
-- Insert sample users
INSERT INTO test_schema.users (name, email, age, active) VALUES
    ('Alice Johnson', 'alice@example.com', 28, true),
    ('Bob Smith', 'bob@example.com', 34, true),
    ('Charlie Brown', 'charlie@example.com', 42, false),
    ('Diana Prince', 'diana@example.com', 31, true);

-- Insert sample posts
INSERT INTO test_schema.posts (user_id, title, content, published, metadata) VALUES
    (1, 'First Post', 'This is my first post', true, '{"author": "Alice", "tags": ["rust", "programming"]}'),
    (1, 'Second Post', 'Another interesting post', true, '{"author": "Alice", "tags": ["database"]}'),
    (2, 'Bob Post', 'Bob writing about systems', false, '{"author": "Bob", "tags": ["systems", "performance"]}'),
    (3, 'Charlie Post', 'Charlie archived post', true, '{"author": "Charlie", "archived": true}');

-- Insert sample comments
INSERT INTO test_schema.comments (post_id, user_id, content) VALUES
    (1, 2, 'Great post Alice!'),
    (1, 3, 'Very interesting'),
    (2, 4, 'I agree!'),
    (3, 1, 'Nice work Bob');

SELECT 'Sample data initialization complete' as message;
EOF
    
    print_info "Sample data initialization completed"
}

# Clean everything
cmd_clean() {
    print_warning "This will remove all containers and volumes!"
    read -p "Are you sure? (yes/no): " -r
    if [[ $REPLY =~ ^yes$ ]]; then
        print_info "Removing all containers and volumes..."
        docker-compose -f "$COMPOSE_FILE" down -v
        print_info "Cleanup completed"
    else
        print_info "Cleanup cancelled"
    fi
}

# Main
main() {
    load_env
    
    local command="${1:-help}"
    shift || true
    
    case "$command" in
        up)
            cmd_up "$@"
            ;;
        down)
            cmd_down "$@"
            ;;
        restart)
            cmd_restart "$@"
            ;;
        logs)
            cmd_logs "$@"
            ;;
        ps)
            cmd_ps "$@"
            ;;
        test-connect)
            cmd_test_connect "$@"
            ;;
        init-data)
            cmd_init_data "$@"
            ;;
        clean)
            cmd_clean "$@"
            ;;
        help)
            print_usage
            ;;
        *)
            print_error "Unknown command: $command"
            print_usage
            exit 1
            ;;
    esac
}

main "$@"
