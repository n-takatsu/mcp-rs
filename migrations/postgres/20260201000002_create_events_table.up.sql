-- Migration: Create events table with JSONB data
-- Created: 2026-02-01

-- Create events table for storing application events
CREATE TABLE IF NOT EXISTS events (
    id SERIAL PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    user_id INTEGER REFERENCES users(id) ON DELETE CASCADE,
    metadata JSONB NOT NULL DEFAULT '{}',
    data JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Create GIN index on metadata for fast JSON queries
CREATE INDEX IF NOT EXISTS idx_events_metadata_gin ON events USING GIN (metadata);

-- Create GIN index on data
CREATE INDEX IF NOT EXISTS idx_events_data_gin ON events USING GIN (data);

-- Create index on event_type for filtering
CREATE INDEX IF NOT EXISTS idx_events_type ON events (event_type);

-- Create index on created_at for time-based queries
CREATE INDEX IF NOT EXISTS idx_events_created_at ON events (created_at DESC);

-- Create composite index for common query pattern
CREATE INDEX IF NOT EXISTS idx_events_type_created ON events (event_type, created_at DESC);

-- Insert sample events
INSERT INTO events (event_type, user_id, metadata, data) VALUES
    ('login', 1, 
     '{"ip": "192.168.1.1", "user_agent": "Mozilla/5.0"}',
     '{"timestamp": "2026-02-01T10:00:00Z", "success": true}'),
    ('page_view', 1,
     '{"referrer": "https://example.com", "session_id": "abc123"}',
     '{"page": "/dashboard", "duration_ms": 1234}'),
    ('api_call', 2,
     '{"endpoint": "/api/users", "method": "GET"}',
     '{"response_time_ms": 45, "status_code": 200}')
ON CONFLICT DO NOTHING;
