-- PostgreSQL ユーザーテーブル作成マイグレーション
-- 認証システムのユーザー情報を永続化

-- ユーザーテーブル
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    username TEXT NOT NULL,
    email TEXT UNIQUE,
    roles TEXT NOT NULL,
    permissions TEXT NOT NULL,
    provider TEXT NOT NULL DEFAULT 'Local',
    metadata TEXT NOT NULL DEFAULT '{}',
    password_hash TEXT,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMP NOT NULL DEFAULT NOW()
);

-- インデックス作成（検索パフォーマンス向上）
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_created_at ON users(created_at);

-- 更新日時の自動更新トリガー
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

CREATE TRIGGER update_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_at_column();

-- コメント追加
COMMENT ON TABLE users IS '認証システムのユーザー情報';
COMMENT ON COLUMN users.id IS 'ユーザーID（UUID推奨）';
COMMENT ON COLUMN users.username IS 'ユーザー名';
COMMENT ON COLUMN users.email IS 'メールアドレス（OAuth2ユーザーはNULL可）';
COMMENT ON COLUMN users.roles IS 'ロール（JSON配列）';
COMMENT ON COLUMN users.permissions IS 'パーミッション（JSON配列）';
COMMENT ON COLUMN users.provider IS '認証プロバイダー（Local/Google/GitHub/Microsoft）';
COMMENT ON COLUMN users.metadata IS 'メタデータ（JSON）';
COMMENT ON COLUMN users.password_hash IS 'パスワードハッシュ（Argon2）';
