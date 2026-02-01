# PostgreSQL Migrations

このディレクトリにはPostgreSQL用のデータベースマイグレーションファイルが含まれています。

## マイグレーションファイル

### 命名規則

マイグレーションファイルは以下の形式で命名されます：

```
{version}_{description}.{up|down}.sql
```

例: `20260201000001_create_users_table.up.sql`

- **version**: タイムスタンプベースのバージョン番号 (YYYYMMDDHHmmss形式)
- **description**: マイグレーションの説明（スネークケース）
- **up**: データベースに変更を適用するSQL
- **down**: 変更をロールバックするSQL

## 既存のマイグレーション

### 1. Create Users Table (20260201000001)

**目的**: JSONB列を持つusersテーブルの作成

**テーブル構造**:
- `id`: 主キー
- `username`: ユーザー名（一意）
- `email`: メールアドレス（一意）
- `profile`: ユーザープロフィール（JSONB）
- `settings`: ユーザー設定（JSONB）
- `created_at`: 作成日時
- `updated_at`: 更新日時

**インデックス**:
- GIN index on `profile` (JSONB検索の高速化)
- GIN index on `settings`
- B-tree index on `email`

**トリガー**:
- `update_updated_at`: 更新時に自動的にupdated_atを更新

### 2. Create Events Table (20260201000002)

**目的**: イベントログ用テーブルの作成

**テーブル構造**:
- `id`: 主キー
- `event_type`: イベントタイプ
- `user_id`: ユーザーID（外部キー）
- `metadata`: イベントメタデータ（JSONB）
- `data`: イベントデータ（JSONB）
- `created_at`: 作成日時

**インデックス**:
- GIN indexes on `metadata` and `data`
- B-tree index on `event_type`
- B-tree index on `created_at` (降順)
- Composite index on `(event_type, created_at)`

### 3. Create Products Table (20260201000003)

**目的**: 製品カタログ用テーブルの作成

**テーブル構造**:
- `id`: 主キー
- `name`: 製品名
- `sku`: 製品コード（一意）
- `price`: 価格
- `specifications`: 製品仕様（JSONB）
- `tags`: タグ（JSONB配列）
- `created_at`: 作成日時
- `updated_at`: 更新日時

**インデックス**:
- GIN indexes on `specifications` and `tags`
- B-tree index on `name`

## マイグレーションの実行

### Rustコードから実行

```rust
use mcp_rs::handlers::database::engines::postgres::{PostgresEngine, PostgresConfig};

let config = PostgresConfig::builder()
    .host("localhost")
    .database("mcp_rs")
    .build()?;

let engine = PostgresEngine::new(config).await?;
let migration_manager = engine.migration_manager("./migrations/postgres");

// マイグレーション実行
migration_manager.run_migrations().await?;

// マイグレーション履歴確認
let history = migration_manager.get_migration_history().await?;
for migration in history {
    println!("Version {}: {}", migration.version, migration.description);
}
```

### sqlx CLIから実行

```bash
# マイグレーション実行
sqlx migrate run --database-url postgres://user:pass@localhost/mcp_rs --source ./migrations/postgres

# マイグレーション情報
sqlx migrate info --database-url postgres://user:pass@localhost/mcp_rs --source ./migrations/postgres

# 最後のマイグレーションをロールバック
sqlx migrate revert --database-url postgres://user:pass@localhost/mcp_rs --source ./migrations/postgres
```

## JSONB使用例

### ユーザープロフィール検索

```sql
-- 特定の都市に住むユーザーを検索
SELECT * FROM users WHERE profile->>'city' = 'Tokyo';

-- 30歳以上のユーザー
SELECT * FROM users WHERE (profile->>'age')::int >= 30;

-- 特定の興味を持つユーザー
SELECT * FROM users WHERE profile->'interests' @> '["coding"]';
```

### イベントクエリ

```sql
-- 特定期間のログインイベント
SELECT * FROM events 
WHERE event_type = 'login' 
AND created_at > NOW() - INTERVAL '1 day';

-- 成功したログインのみ
SELECT * FROM events 
WHERE event_type = 'login' 
AND data->>'success' = 'true';
```

### 製品検索

```sql
-- 特定のCPUを搭載した製品
SELECT * FROM products 
WHERE specifications->>'cpu' LIKE '%i7%';

-- 特定タグを持つ製品
SELECT * FROM products 
WHERE tags @> '["gaming"]';
```

## パフォーマンス最適化

### GINインデックスの効果

JSONB列にGINインデックスを作成することで、以下のクエリが高速化されます：

- `@>` (contains): JSONB値に特定のキー/値が含まれるか
- `<@` (contained by): JSONB値が特定の値に含まれるか
- `?` (exists): キーが存在するか
- `?|` (exists any): いずれかのキーが存在するか
- `?&` (exists all): すべてのキーが存在するか

### インデックス使用統計

```sql
-- インデックスの使用状況確認
SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE tablename IN ('users', 'events', 'products')
ORDER BY idx_scan DESC;
```

## 新しいマイグレーション作成

```rust
let manager = engine.migration_manager("./migrations/postgres");
let (up_file, down_file) = manager.create_migration("add_user_roles").await?;
println!("Created: {} and {}", up_file, down_file);
```

その後、生成されたファイルにSQL文を記述してください。

## 注意事項

1. **Down migration**: すべてのup migrationに対応するdown migrationを作成してください
2. **本番環境**: 本番環境でのマイグレーション実行前に必ずバックアップを取得してください
3. **テスト**: 新しいマイグレーションは開発環境で十分にテストしてください
4. **ロールバック**: Down migrationが正しく動作することを確認してください
