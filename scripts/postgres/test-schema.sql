-- PostgreSQL test schema creation script
-- This script creates additional test tables for comprehensive testing

-- Create schema for transaction tests
CREATE SCHEMA IF NOT EXISTS transaction_test;
GRANT ALL ON SCHEMA transaction_test TO testuser;

-- Table for transaction isolation level testing
CREATE TABLE IF NOT EXISTS transaction_test.isolation_test (
    id SERIAL PRIMARY KEY,
    transaction_id VARCHAR(50) NOT NULL,
    value INTEGER NOT NULL,
    locked BOOLEAN DEFAULT false,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Table for savepoint testing
CREATE TABLE IF NOT EXISTS transaction_test.savepoint_test (
    id SERIAL PRIMARY KEY,
    test_number INTEGER NOT NULL,
    data VARCHAR(255),
    checkpoint_name VARCHAR(100),
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Table for constraint testing
CREATE TABLE IF NOT EXISTS transaction_test.constraint_test (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL UNIQUE,
    value INTEGER NOT NULL CHECK (value > 0),
    status VARCHAR(50) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Table for JSON operation testing
CREATE TABLE IF NOT EXISTS transaction_test.json_operations (
    id SERIAL PRIMARY KEY,
    entity_name VARCHAR(255) NOT NULL,
    attributes JSONB NOT NULL DEFAULT '{}',
    tags JSONB NOT NULL DEFAULT '[]',
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create GIN index for JSON operations
CREATE INDEX IF NOT EXISTS idx_json_attributes ON transaction_test.json_operations USING GIN(attributes);
CREATE INDEX IF NOT EXISTS idx_json_tags ON transaction_test.json_operations USING GIN(tags);

-- Table for concurrent operation testing
CREATE TABLE IF NOT EXISTS transaction_test.concurrent_test (
    id SERIAL PRIMARY KEY,
    thread_id VARCHAR(50) NOT NULL,
    operation_count INTEGER DEFAULT 0,
    status VARCHAR(50) NOT NULL DEFAULT 'pending',
    started_at TIMESTAMP,
    completed_at TIMESTAMP,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Table for parameter binding testing
CREATE TABLE IF NOT EXISTS transaction_test.parameter_test (
    id SERIAL PRIMARY KEY,
    param_type VARCHAR(50) NOT NULL,
    int_value INTEGER,
    bigint_value BIGINT,
    float_value FLOAT,
    text_value TEXT,
    boolean_value BOOLEAN,
    date_value DATE,
    timestamp_value TIMESTAMP,
    uuid_value UUID,
    json_value JSONB,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Permissions
GRANT ALL ON transaction_test.* TO testuser;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA transaction_test TO testuser;

-- Create schema for performance testing
CREATE SCHEMA IF NOT EXISTS performance_test;
GRANT ALL ON SCHEMA performance_test TO testuser;

-- Large table for performance testing
CREATE TABLE IF NOT EXISTS performance_test.large_dataset (
    id SERIAL PRIMARY KEY,
    category VARCHAR(100) NOT NULL,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    value NUMERIC(10, 2),
    status VARCHAR(50) DEFAULT 'active',
    data JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Indexes for performance testing
CREATE INDEX IF NOT EXISTS idx_large_category ON performance_test.large_dataset(category);
CREATE INDEX IF NOT EXISTS idx_large_status ON performance_test.large_dataset(status);
CREATE INDEX IF NOT EXISTS idx_large_created ON performance_test.large_dataset(created_at);
CREATE INDEX IF NOT EXISTS idx_large_data ON performance_test.large_dataset USING GIN(data);

-- Query complexity testing table
CREATE TABLE IF NOT EXISTS performance_test.complex_joins (
    id SERIAL PRIMARY KEY,
    rel_type VARCHAR(50) NOT NULL,
    source_id INTEGER NOT NULL,
    target_id INTEGER NOT NULL,
    weight INTEGER DEFAULT 1,
    attributes JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_complex_source ON performance_test.complex_joins(source_id);
CREATE INDEX IF NOT EXISTS idx_complex_target ON performance_test.complex_joins(target_id);
CREATE INDEX IF NOT EXISTS idx_complex_type ON performance_test.complex_joins(rel_type);

GRANT ALL ON performance_test.* TO testuser;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA performance_test TO testuser;

-- Create schema for security testing
CREATE SCHEMA IF NOT EXISTS security_test;
GRANT ALL ON SCHEMA security_test TO testuser;

-- Sensitive data table for security testing
CREATE TABLE IF NOT EXISTS security_test.sensitive_data (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL,
    encrypted_value TEXT NOT NULL,
    access_log JSONB DEFAULT '[]',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    accessed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

GRANT ALL ON security_test.* TO testuser;
GRANT USAGE ON ALL SEQUENCES IN SCHEMA security_test TO testuser;

-- Display schema creation complete message
SELECT 'PostgreSQL test schema creation complete.' as message;
