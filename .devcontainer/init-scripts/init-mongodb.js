// MongoDB Initialization Script for MCP-RS Development

// Switch to mcp_rs_dev database
db = db.getSiblingDB('mcp_rs_dev');

// Create a sample collection for testing connection
db.health_check.insertOne({
    status: 'healthy',
    checked_at: new Date()
});

// Create user for application
db.createUser({
    user: 'mcp_user',
    pwd: 'password',
    roles: [
        {
            role: 'readWrite',
            db: 'mcp_rs_dev'
        }
    ]
});

// Switch to test database
db = db.getSiblingDB('mcp_rs_test');

// Create test collection
db.health_check.insertOne({
    status: 'test_ready',
    checked_at: new Date()
});

// Grant access to test database
db.createUser({
    user: 'mcp_user',
    pwd: 'password',
    roles: [
        {
            role: 'readWrite',
            db: 'mcp_rs_test'
        }
    ]
});