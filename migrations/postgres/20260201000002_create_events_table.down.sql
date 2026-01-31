-- Migration Rollback: Create events table with JSONB data
-- Created: 2026-02-01

-- Drop indexes
DROP INDEX IF EXISTS idx_events_metadata_gin;
DROP INDEX IF EXISTS idx_events_data_gin;
DROP INDEX IF EXISTS idx_events_type;
DROP INDEX IF EXISTS idx_events_created_at;
DROP INDEX IF EXISTS idx_events_type_created;

-- Drop table
DROP TABLE IF EXISTS events;
