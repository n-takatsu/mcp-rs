#!/bin/bash
set -e

echo "🚀 Starting MCP-RS Docker Compose Development Environment..."

# Build and start all services
echo "📦 Building and starting containers..."
docker-compose up -d

# Wait for databases to be ready
echo "⏳ Waiting for databases to be ready..."
sleep 10

# Check database health
echo "🔍 Checking database connectivity..."

# PostgreSQL
echo "  🐘 PostgreSQL..."
docker-compose exec postgres pg_isready -U postgres || echo "    ⚠️  PostgreSQL not ready"

# MySQL
echo "  🐬 MySQL..."
docker-compose exec mysql mysqladmin ping -h localhost -u root -ppassword || echo "    ⚠️  MySQL not ready"

# MongoDB
echo "  🍃 MongoDB..."
docker-compose exec mongodb mongosh --eval "db.adminCommand('ping')" || echo "    ⚠️  MongoDB not ready"

# Redis
echo "  🔴 Redis..."
docker-compose exec redis redis-cli -a password ping || echo "    ⚠️  Redis not ready"

echo ""
echo "🎉 MCP-RS Development Environment is ready!"
echo ""
echo "📊 Available Services:"
echo "  • Main Container:      docker-compose exec mcp-rs-dev zsh"
echo "  • MCP Server:          http://localhost:3000"
echo "  • Adminer (DB Admin):  http://localhost:8090"
echo "  • Redis Commander:     http://localhost:8091"
echo ""
echo "🗄️  Database Connections:"
echo "  • PostgreSQL:  postgresql://postgres:password@localhost:5432/mcp_rs_dev"
echo "  • MySQL:       mysql://root:password@localhost:3306/mcp_rs_dev"
echo "  • MongoDB:     mongodb://admin:password@localhost:27017/mcp_rs_dev"
echo "  • Redis:       redis://:password@localhost:6379"
echo ""
echo "🛠️  Development Commands:"
echo "  • Enter container:     docker-compose exec mcp-rs-dev zsh"
echo "  • View logs:           docker-compose logs -f"
echo "  • Stop services:       docker-compose down"
echo "  • Reset data:          docker-compose down -v"