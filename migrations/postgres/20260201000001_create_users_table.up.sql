-- Migration: Create users table with JSONB support
-- Created: 2026-02-01

-- Create users table with JSONB profile column
CREATE TABLE IF NOT EXISTS users (
    id SERIAL PRIMARY KEY,
    username VARCHAR(255) NOT NULL UNIQUE,
    email VARCHAR(255) NOT NULL UNIQUE,
    profile JSONB NOT NULL DEFAULT '{}',
    settings JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Create GIN index on profile JSONB column for fast lookups
CREATE INDEX IF NOT EXISTS idx_users_profile_gin ON users USING GIN (profile);

-- Create GIN index on settings JSONB column
CREATE INDEX IF NOT EXISTS idx_users_settings_gin ON users USING GIN (settings);

-- Create index on email for fast lookups
CREATE INDEX IF NOT EXISTS idx_users_email ON users (email);

-- Create updated_at trigger function
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Create trigger to automatically update updated_at
CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Insert sample data
INSERT INTO users (username, email, profile, settings) VALUES
    ('alice', 'alice@example.com', 
     '{"name": "Alice Smith", "age": 30, "city": "Tokyo", "interests": ["coding", "music"]}',
     '{"theme": "dark", "notifications": true, "language": "ja"}'),
    ('bob', 'bob@example.com',
     '{"name": "Bob Johnson", "age": 25, "city": "Osaka", "interests": ["gaming", "sports"]}',
     '{"theme": "light", "notifications": false, "language": "en"}')
ON CONFLICT (username) DO NOTHING;
