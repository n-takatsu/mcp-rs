-- Migration Rollback: Create products table with JSONB specifications
-- Created: 2026-02-01

-- Drop trigger
DROP TRIGGER IF EXISTS update_products_updated_at ON products;

-- Drop indexes
DROP INDEX IF EXISTS idx_products_specifications_gin;
DROP INDEX IF EXISTS idx_products_tags_gin;
DROP INDEX IF EXISTS idx_products_name;

-- Drop table
DROP TABLE IF EXISTS products;
