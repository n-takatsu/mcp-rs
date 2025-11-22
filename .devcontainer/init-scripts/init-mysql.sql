-- MySQL Initialization Script for MCP-RS Development

-- Create additional database for testing
CREATE DATABASE IF NOT EXISTS mcp_rs_test;

-- Create sample table for testing connection
USE mcp_rs_dev;

CREATE TABLE IF NOT EXISTS health_check (
    id INT AUTO_INCREMENT PRIMARY KEY,
    status VARCHAR(50) NOT NULL,
    checked_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Insert sample data
INSERT INTO health_check (status) VALUES ('healthy');

-- Grant privileges to mcp_user
GRANT ALL PRIVILEGES ON mcp_rs_dev.* TO 'mcp_user'@'%';
GRANT ALL PRIVILEGES ON mcp_rs_test.* TO 'mcp_user'@'%';

-- Apply privileges
FLUSH PRIVILEGES;