-- Migration Rollback: Create users table with JSONB support
-- Created: 2026-02-01

-- Drop trigger
DROP TRIGGER IF EXISTS update_users_updated_at ON users;

-- Drop trigger function
DROP FUNCTION IF EXISTS update_updated_at_column();

-- Drop indexes
DROP INDEX IF EXISTS idx_users_profile_gin;
DROP INDEX IF EXISTS idx_users_settings_gin;
DROP INDEX IF EXISTS idx_users_email;

-- Drop table
DROP TABLE IF EXISTS users;
