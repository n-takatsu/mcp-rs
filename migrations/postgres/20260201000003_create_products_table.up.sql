-- Migration: Create products table with JSONB specifications
-- Created: 2026-02-01

-- Create products table with JSONB for flexible attributes
CREATE TABLE IF NOT EXISTS products (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    sku VARCHAR(100) NOT NULL UNIQUE,
    price DECIMAL(10, 2) NOT NULL,
    specifications JSONB NOT NULL DEFAULT '{}',
    tags JSONB NOT NULL DEFAULT '[]',
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- Create GIN index on specifications for product search
CREATE INDEX IF NOT EXISTS idx_products_specifications_gin ON products USING GIN (specifications);

-- Create GIN index on tags
CREATE INDEX IF NOT EXISTS idx_products_tags_gin ON products USING GIN (tags);

-- Create index on name for text search
CREATE INDEX IF NOT EXISTS idx_products_name ON products (name);

-- Create trigger for updated_at
CREATE TRIGGER update_products_updated_at
    BEFORE UPDATE ON products
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- Insert sample products
INSERT INTO products (name, sku, price, specifications, tags) VALUES
    ('Gaming Laptop', 'LAPTOP-001', 1299.99,
     '{"cpu": "Intel i7", "ram": "16GB", "storage": "512GB SSD", "gpu": "RTX 3060"}',
     '["gaming", "laptop", "high-performance"]'),
    ('Wireless Mouse', 'MOUSE-001', 29.99,
     '{"connectivity": "Bluetooth", "dpi": 1600, "battery": "AAA x2"}',
     '["peripheral", "wireless", "mouse"]'),
    ('Mechanical Keyboard', 'KEYBOARD-001', 149.99,
     '{"switches": "Cherry MX Blue", "layout": "TKL", "backlight": "RGB"}',
     '["peripheral", "mechanical", "keyboard", "rgb"]')
ON CONFLICT (sku) DO NOTHING;
