-- tests/database/postgres_database_integration_tests.rs が使うスキーマと初期データ（#513）。
-- scripts/postgres/init.sql（docker-composeのコンテナの初期化）から、ユーザーの作成と
-- 権限の付与を除いたもの。テストを実行するユーザー（TEST_DATABASE_URL）で流すので、
-- 作ったテーブルの所有者はそのユーザーになる。テストごとに、スキーマを消してから流す。

CREATE SCHEMA IF NOT EXISTS test_schema;

-- Create users table
CREATE TABLE IF NOT EXISTS test_schema.users (
  id SERIAL PRIMARY KEY,
  name VARCHAR(255) NOT NULL,
  email VARCHAR(255) UNIQUE NOT NULL,
  age INTEGER,
  active BOOLEAN DEFAULT TRUE,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create posts table
CREATE TABLE IF NOT EXISTS test_schema.posts (
  id SERIAL PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES test_schema.users(id) ON DELETE CASCADE,
  title VARCHAR(255) NOT NULL,
  content TEXT,
  published BOOLEAN DEFAULT FALSE,
  metadata JSONB DEFAULT '{}'::JSONB,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create comments table
CREATE TABLE IF NOT EXISTS test_schema.comments (
  id SERIAL PRIMARY KEY,
  post_id INTEGER NOT NULL REFERENCES test_schema.posts(id) ON DELETE CASCADE,
  user_id INTEGER NOT NULL REFERENCES test_schema.users(id) ON DELETE CASCADE,
  content TEXT NOT NULL,
  created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Create indexes
CREATE INDEX IF NOT EXISTS idx_users_email ON test_schema.users(email);
CREATE INDEX IF NOT EXISTS idx_posts_user_id ON test_schema.posts(user_id);
CREATE INDEX IF NOT EXISTS idx_posts_published ON test_schema.posts(published);
CREATE INDEX IF NOT EXISTS idx_comments_post_id ON test_schema.comments(post_id);
CREATE INDEX IF NOT EXISTS idx_comments_user_id ON test_schema.comments(user_id);
CREATE INDEX IF NOT EXISTS idx_posts_metadata ON test_schema.posts USING GIN(metadata);

-- Insert sample data for testing
INSERT INTO test_schema.users (name, email, age, active) 
VALUES 
  ('Alice Johnson', 'alice@example.com', 28, true),
  ('Bob Smith', 'bob@example.com', 35, true),
  ('Charlie Brown', 'charlie@example.com', 32, false)
ON CONFLICT (email) DO NOTHING;

INSERT INTO test_schema.posts (user_id, title, content, published, metadata)
VALUES 
  (1, 'PostgreSQL Testing', 'A comprehensive guide to testing PostgreSQL', true, '{"tags": ["postgresql", "testing"], "author": "Alice"}'),
  (2, 'Database Design', 'Best practices for database design', true, '{"tags": ["database", "design"], "author": "Bob"}'),
  (1, 'Advanced Transactions', 'Understanding transaction isolation levels', false, '{"tags": ["transactions", "acid"], "author": "Alice"}')
ON CONFLICT DO NOTHING;

INSERT INTO test_schema.comments (post_id, user_id, content)
VALUES 
  (1, 2, 'Great article!'),
  (1, 3, 'Very informative'),
  (2, 1, 'Excellent points')
ON CONFLICT DO NOTHING;
