#!/usr/bin/env bash
# Integration test runner script for Docker Compose environment

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== MCP-RS Container Integration Tests ===${NC}"

# Cleanup function
cleanup() {
    echo -e "${YELLOW}Cleaning up test environment...${NC}"
    docker-compose -f docker-compose.test.yml down -v
}

# Set trap to cleanup on exit
trap cleanup EXIT INT TERM

# Start test environment
echo -e "${YELLOW}Starting test environment...${NC}"
docker-compose -f docker-compose.test.yml up -d postgres-test redis-test

# Wait for services to be ready
echo -e "${YELLOW}Waiting for services to be ready...${NC}"
sleep 10

# Run database migrations if needed
echo -e "${YELLOW}Running database setup...${NC}"
docker-compose -f docker-compose.test.yml exec -T postgres-test psql -U testuser -d mcptest -c "SELECT 1;" || true

# Start MCP servers
echo -e "${YELLOW}Starting MCP servers...${NC}"
docker-compose -f docker-compose.test.yml up -d mcp-server-http mcp-server-websocket

# Wait for servers to be ready
echo -e "${YELLOW}Waiting for MCP servers to be ready...${NC}"
sleep 15

# Check server health
echo -e "${YELLOW}Checking server health...${NC}"
for i in {1..30}; do
    if curl -s http://localhost:3001/health > /dev/null 2>&1; then
        echo -e "${GREEN}HTTP server is healthy${NC}"
        break
    fi
    if [ $i -eq 30 ]; then
        echo -e "${RED}HTTP server failed to start${NC}"
        docker-compose -f docker-compose.test.yml logs mcp-server-http
        exit 1
    fi
    sleep 2
done

# Run integration tests
echo -e "${YELLOW}Running integration tests...${NC}"
export MCP_HTTP_ENDPOINT="http://localhost:3001"
export MCP_WEBSOCKET_ENDPOINT="ws://localhost:3002"
export DATABASE_URL="postgres://testuser:testpass@localhost:5433/mcptest"
export REDIS_URL="redis://localhost:6380"

cargo test --features integration-tests --test '*' -- --test-threads=1

# Check test result
if [ $? -eq 0 ]; then
    echo -e "${GREEN}✓ All integration tests passed!${NC}"
else
    echo -e "${RED}✗ Integration tests failed${NC}"
    echo -e "${YELLOW}Showing server logs:${NC}"
    docker-compose -f docker-compose.test.yml logs
    exit 1
fi

# Optional: Run performance tests
if [ "$RUN_PERFORMANCE_TESTS" = "true" ]; then
    echo -e "${YELLOW}Running performance tests...${NC}"
    cargo test --features integration-tests performance -- --test-threads=1 --nocapture
fi

echo -e "${GREEN}=== Test suite completed successfully ===${NC}"
