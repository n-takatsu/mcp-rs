-- Column Encryption RBAC Integration

-- カラム暗号化権限テーブル
CREATE TABLE IF NOT EXISTS column_encryption_permissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    role_name VARCHAR(255) NOT NULL,
    table_name VARCHAR(255) NOT NULL,
    column_name VARCHAR(255) NOT NULL,
    can_encrypt BOOLEAN DEFAULT FALSE,
    can_decrypt BOOLEAN DEFAULT FALSE,
    can_rotate_key BOOLEAN DEFAULT FALSE,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    UNIQUE(role_name, table_name, column_name)
);

-- 暗号化操作監査ログテーブル
-- request_ip は VARCHAR で保持する（アプリケーション側は sqlx の ipnetwork 拡張を
-- 有効化しておらず、素の文字列としてバインドするため INET 型は使えない）。
CREATE TABLE IF NOT EXISTS encryption_audit_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id VARCHAR(255) NOT NULL,
    operation VARCHAR(50) NOT NULL, -- 'encrypt', 'decrypt', 'rotate_key'
    table_name VARCHAR(255) NOT NULL,
    column_name VARCHAR(255) NOT NULL,
    success BOOLEAN NOT NULL,
    error_message TEXT,
    timestamp TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    request_ip VARCHAR(45),
    user_agent TEXT
);

CREATE INDEX IF NOT EXISTS idx_encryption_audit_user ON encryption_audit_log (user_id, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_encryption_audit_table ON encryption_audit_log (table_name, column_name, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_encryption_audit_operation ON encryption_audit_log (operation, timestamp DESC);

-- デフォルト権限（Admin）
INSERT INTO column_encryption_permissions (role_name, table_name, column_name, can_encrypt, can_decrypt, can_rotate_key)
VALUES
    ('Admin', '*', '*', true, true, true)
ON CONFLICT (role_name, table_name, column_name) DO NOTHING;

-- 更新時刻の自動更新トリガー
CREATE OR REPLACE FUNCTION update_column_encryption_permissions_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_column_encryption_permissions_updated_at
    BEFORE UPDATE ON column_encryption_permissions
    FOR EACH ROW
    EXECUTE FUNCTION update_column_encryption_permissions_updated_at();

-- コメント
COMMENT ON TABLE column_encryption_permissions IS 'カラムレベル暗号化のRBAC権限管理';
COMMENT ON TABLE encryption_audit_log IS '暗号化操作の監査ログ';
COMMENT ON COLUMN column_encryption_permissions.role_name IS 'ロール名（Adminの場合は全権限）';
COMMENT ON COLUMN column_encryption_permissions.table_name IS 'テーブル名（*はワイルドカード）';
COMMENT ON COLUMN column_encryption_permissions.column_name IS 'カラム名（*はワイルドカード）';
COMMENT ON COLUMN encryption_audit_log.operation IS '操作種別: encrypt, decrypt, rotate_key';
COMMENT ON COLUMN encryption_audit_log.success IS '操作成功/失敗';
