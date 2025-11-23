-- PostgreSQL initialization script for MCP-RS Phase 2 testing
-- This script runs automatically on container startup

-- Create test user if not exists
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT FROM pg_user WHERE usename = 'testuser'
  ) THEN
    CREATE USER testuser WITH PASSWORD 'testpass';
  END IF;
END
$$;

-- Grant privileges to test user
GRANT CONNECT ON DATABASE testdb TO testuser;
GRANT CREATE ON DATABASE testdb TO testuser;

-- Create testing schema
CREATE SCHEMA IF NOT EXISTS test_schema;
GRANT ALL ON SCHEMA test_schema TO testuser;
GRANT ALL ON SCHEMA public TO testuser;

-- Create initial test tables
CREATE TABLE IF NOT EXISTS test_schema.users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    email VARCHAR(255) UNIQUE NOT NULL,
    age INTEGER CHECK (age >= 0),
    active BOOLEAN DEFAULT true,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS test_schema.posts (
    id SERIAL PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES test_schema.users(id) ON DELETE CASCADE,
    title VARCHAR(500) NOT NULL,
    content TEXT,
    published BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS test_schema.comments (
    id SERIAL PRIMARY KEY,
    post_id INTEGER NOT NULL REFERENCES test_schema.posts(id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL REFERENCES test_schema.users(id) ON DELETE CASCADE,
    content TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes for performance testing
CREATE INDEX IF NOT EXISTS idx_posts_user_id ON test_schema.posts(user_id);
CREATE INDEX IF NOT EXISTS idx_posts_published ON test_schema.posts(published);
CREATE INDEX IF NOT EXISTS idx_comments_post_id ON test_schema.comments(post_id);
CREATE INDEX IF NOT EXISTS idx_comments_user_id ON test_schema.comments(user_id);
CREATE INDEX IF NOT EXISTS idx_posts_metadata ON test_schema.posts USING GIN(metadata);

-- Set appropriate permissions
GRANT ALL ON test_schema.* TO testuser;
GRANT USAGE ON SEQUENCE test_schema.users_id_seq TO testuser;
GRANT USAGE ON SEQUENCE test_schema.posts_id_seq TO testuser;
GRANT USAGE ON SEQUENCE test_schema.comments_id_seq TO testuser;

-- Enable UUID extension for advanced testing
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Create table with UUID for testing
CREATE TABLE IF NOT EXISTS test_schema.uuid_entities (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    name VARCHAR(255) NOT NULL,
    data JSONB DEFAULT '{}',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

GRANT ALL ON test_schema.uuid_entities TO testuser;

-- Create logging function for transaction testing
CREATE OR REPLACE FUNCTION log_update()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = CURRENT_TIMESTAMP;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create triggers for updated_at fields
CREATE TRIGGER users_update_trigger
BEFORE UPDATE ON test_schema.users
FOR EACH ROW
EXECUTE FUNCTION log_update();

CREATE TRIGGER posts_update_trigger
BEFORE UPDATE ON test_schema.posts
FOR EACH ROW
EXECUTE FUNCTION log_update();

-- Grant execute permission on functions
GRANT EXECUTE ON FUNCTION log_update TO testuser;

-- Display initialization complete message
SELECT 'PostgreSQL initialization complete. Test schema ready.' as message;
