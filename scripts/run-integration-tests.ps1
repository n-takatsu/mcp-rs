# PowerShell script for running integration tests on Windows

$ErrorActionPreference = "Stop"

Write-Host "=== MCP-RS Container Integration Tests ===" -ForegroundColor Green

# Cleanup function
function Cleanup {
    Write-Host "Cleaning up test environment..." -ForegroundColor Yellow
    docker-compose -f docker-compose.test.yml down -v
}

# Register cleanup
try {
    # Start test environment
    Write-Host "Starting test environment..." -ForegroundColor Yellow
    docker-compose -f docker-compose.test.yml up -d postgres-test redis-test

    # Wait for services
    Write-Host "Waiting for services to be ready..." -ForegroundColor Yellow
    Start-Sleep -Seconds 10

    # Start MCP servers
    Write-Host "Starting MCP servers..." -ForegroundColor Yellow
    docker-compose -f docker-compose.test.yml up -d mcp-server-http mcp-server-websocket

    # Wait for servers
    Write-Host "Waiting for MCP servers to be ready..." -ForegroundColor Yellow
    Start-Sleep -Seconds 15

    # Check server health
    Write-Host "Checking server health..." -ForegroundColor Yellow
    $maxAttempts = 30
    $attempt = 0
    $healthy = $false

    while ($attempt -lt $maxAttempts) {
        try {
            $response = Invoke-WebRequest -Uri "http://localhost:3001/health" -TimeoutSec 5 -UseBasicParsing
            if ($response.StatusCode -eq 200) {
                Write-Host "HTTP server is healthy" -ForegroundColor Green
                $healthy = $true
                break
            }
        }
        catch {
            # Ignore errors, just retry
        }
        $attempt++
        Start-Sleep -Seconds 2
    }

    if (-not $healthy) {
        Write-Host "HTTP server failed to start" -ForegroundColor Red
        docker-compose -f docker-compose.test.yml logs mcp-server-http
        exit 1
    }

    # Set environment variables
    $env:MCP_HTTP_ENDPOINT = "http://localhost:3001"
    $env:MCP_WEBSOCKET_ENDPOINT = "ws://localhost:3002"
    $env:DATABASE_URL = "postgres://testuser:testpass@localhost:5433/mcptest"
    $env:REDIS_URL = "redis://localhost:6380"

    # Run integration tests
    Write-Host "Running integration tests..." -ForegroundColor Yellow
    cargo test --features integration-tests --test '*' -- --test-threads=1

    if ($LASTEXITCODE -eq 0) {
        Write-Host "✓ All integration tests passed!" -ForegroundColor Green
    }
    else {
        Write-Host "✗ Integration tests failed" -ForegroundColor Red
        Write-Host "Showing server logs:" -ForegroundColor Yellow
        docker-compose -f docker-compose.test.yml logs
        exit 1
    }

    # Optional: Run performance tests
    if ($env:RUN_PERFORMANCE_TESTS -eq "true") {
        Write-Host "Running performance tests..." -ForegroundColor Yellow
        cargo test --features integration-tests performance -- --test-threads=1 --nocapture
    }

    Write-Host "=== Test suite completed successfully ===" -ForegroundColor Green
}
finally {
    Cleanup
}
