-- PostgreSQL Initialization Script for MCP-RS Development

-- Create additional databases for testing
CREATE DATABASE mcp_rs_test;

-- Create user for application
CREATE USER mcp_user WITH PASSWORD 'password';

-- Grant privileges
GRANT ALL PRIVILEGES ON DATABASE mcp_rs_dev TO mcp_user;
GRANT ALL PRIVILEGES ON DATABASE mcp_rs_test TO mcp_user;

-- Switch to mcp_rs_dev database
\c mcp_rs_dev;

-- Create extensions that might be useful
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- Create a sample table for testing connection
CREATE TABLE IF NOT EXISTS health_check (
    id SERIAL PRIMARY KEY,
    status VARCHAR(50) NOT NULL,
    checked_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert sample data
INSERT INTO health_check (status) VALUES ('healthy');

-- Grant table permissions
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO mcp_user;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO mcp_user;