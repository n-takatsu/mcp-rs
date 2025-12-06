# PostgreSQL認証データベース マイグレーション

PostgreSQL認証システムのデータベーススキーマを管理します。

## 前提条件

- PostgreSQL 12以上
- `sqlx-cli` インストール済み

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

## セットアップ

### 1. 環境変数設定

```bash
# .env ファイルを作成
export DATABASE_URL="postgresql://username:password@localhost/mcp_rs"
```

### 2. データベース作成

```bash
# PostgreSQLに接続
createdb mcp_rs

# または psql で
psql -U postgres
CREATE DATABASE mcp_rs;
```

### 3. マイグレーション実行

```bash
# プロジェクトルートで実行
sqlx migrate run --source migrations
```

## マイグレーション詳細

### 001_create_users_table.sql

ユーザー情報を格納するテーブルを作成します。

**テーブル構造:**

| カラム名 | 型 | 制約 | 説明 |
|---------|-----|------|------|
| id | TEXT | PRIMARY KEY | ユーザーID（UUID推奨） |
| username | TEXT | NOT NULL | ユーザー名 |
| email | TEXT | UNIQUE | メールアドレス |
| roles | TEXT | NOT NULL | ロール（JSON配列） |
| permissions | TEXT | NOT NULL | パーミッション（JSON配列） |
| provider | TEXT | NOT NULL | 認証プロバイダー |
| metadata | TEXT | NOT NULL | メタデータ（JSON） |
| password_hash | TEXT | NULL | パスワードハッシュ（Argon2） |
| created_at | TIMESTAMP | NOT NULL | 作成日時 |
| updated_at | TIMESTAMP | NOT NULL | 更新日時 |

**インデックス:**
- `idx_users_email`: メールアドレス検索用
- `idx_users_username`: ユーザー名検索用
- `idx_users_created_at`: 作成日時ソート用

**トリガー:**
- `update_users_updated_at`: 更新時に`updated_at`を自動更新

## 使用例

### Rust コードから使用

```rust
use mcp_rs::security::auth::repository::postgres::PostgresUserRepository;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // データベース接続
    let repo = PostgresUserRepository::new(
        "postgresql://user:pass@localhost/mcp_rs",
        10,  // max_connections
        12   // argon2 salt_rounds
    ).await?;

    // マイグレーション実行
    repo.run_migrations().await?;

    // ユーザー作成
    let mut user = AuthUser::new(
        uuid::Uuid::new_v4().to_string(),
        "john_doe".to_string()
    );
    user.email = Some("john@example.com".to_string());
    user.roles.insert(Role::User);

    let password_hash = repo.hash_password("SecurePass123!")?;
    repo.create_user(&user, Some(password_hash)).await?;

    Ok(())
}
```

## マイグレーションの巻き戻し

```bash
# 直前のマイグレーションを取り消し
sqlx migrate revert --source migrations
```

## トラブルシューティング

### エラー: "relation users already exists"

データベースがすでに存在する場合は、`DROP TABLE`で削除してから再実行してください。

```sql
DROP TABLE IF EXISTS users CASCADE;
```

### エラー: "connection refused"

PostgreSQLサーバーが起動していることを確認してください。

```bash
# Linux/Mac
sudo service postgresql start

# macOS (Homebrew)
brew services start postgresql

# Windows
net start postgresql-x64-14
```

## セキュリティ考慮事項

1. **パスワードハッシュ**: Argon2を使用（bcryptより安全）
2. **データベース接続**: SSL/TLS必須（本番環境）
3. **アクセス制御**: データベースユーザーの権限を最小化
4. **バックアップ**: 定期的なデータベースバックアップ

## 本番環境デプロイ

```bash
# 環境変数設定（本番環境）
export DATABASE_URL="postgresql://prod_user:${DB_PASSWORD}@db.example.com/mcp_rs?sslmode=require"

# マイグレーション実行
sqlx migrate run --source migrations

# 接続確認
psql $DATABASE_URL -c "SELECT COUNT(*) FROM users;"
```

## 参考資料

- [sqlx Documentation](https://docs.rs/sqlx/)
- [PostgreSQL Documentation](https://www.postgresql.org/docs/)
- [Argon2 Password Hashing](https://en.wikipedia.org/wiki/Argon2)
