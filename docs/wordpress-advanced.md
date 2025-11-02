# WordPress Advanced Features Documentation

## 📝 投稿タイプと詳細オプション

### 🎯 投稿タイプ

WordPressでは以下の投稿タイプがサポートされています：

- **投稿 (post)**: ブログ記事やニュース記事
- **固定ページ (page)**: 会社概要、お問い合わせページなど

### 📊 投稿ステータス

以下のステータスが利用可能です：

| ステータス | 説明 | 用途 |
|-----------|------|------|
| `publish` | 公開 | 一般公開されている記事 |
| `draft` | 下書き | 作成中の記事 |
| `private` | 非公開 | ログインユーザーのみ閲覧可能 |
| `future` | 予約投稿 | 指定日時に自動公開 |

### 🔍 SEOメタデータ

#### Yoast SEO プラグイン対応メタフィールド

```json
{
  "_yoast_wpseo_title": "カスタムSEOタイトル",
  "_yoast_wpseo_metadesc": "カスタムメタディスクリプション",
  "_yoast_wpseo_meta-robots-noindex": "1",
  "_yoast_wpseo_meta-robots-nofollow": "1",
  "_yoast_wpseo_canonical": "https://example.com/canonical-url",
  "_yoast_wpseo_focuskw": "フォーカスキーワード"
}
```

#### その他のSEOプラグイン

```json
{
  "_aioseop_title": "All in One SEO タイトル",
  "_aioseop_description": "All in One SEO ディスクリプション",
  "_genesis_title": "Genesis SEO タイトル",
  "_genesis_description": "Genesis SEO ディスクリプション"
}
```

## 🛠️ 高度な投稿作成API

### `PostCreateParams` 構造体

投稿作成時のパラメータを構造体で整理し、より明確で保守しやすいAPIを提供します。

```rust
#[derive(Debug, Clone)]
pub struct PostCreateParams {
    pub title: String,                           // タイトル
    pub content: String,                         // コンテンツ
    pub post_type: String,                       // "post" or "page"
    pub status: String,                          // "publish", "draft", "private", "future"
    pub date: Option<String>,                    // 予約投稿日 (ISO8601形式)
    pub categories: Option<Vec<u64>>,            // カテゴリーID（投稿のみ）
    pub tags: Option<Vec<u64>>,                  // タグID（投稿のみ）
    pub featured_media_id: Option<u64>,          // アイキャッチ画像ID
    pub meta: Option<HashMap<String, String>>,   // SEOメタデータ等
}
```

### `PostUpdateParams` 構造体

投稿更新時のパラメータ構造体（すべてのフィールドがOptional）

```rust
#[derive(Debug, Clone, Default)]
pub struct PostUpdateParams {
    pub title: Option<String>,
    pub content: Option<String>,
    pub status: Option<String>,
    pub categories: Option<Vec<u64>>,
    pub tags: Option<Vec<u64>>,
    pub featured_media_id: Option<u64>,
    pub meta: Option<HashMap<String, String>>,
}
```

### `create_advanced_post` 関数

```rust
pub async fn create_advanced_post(
    &self,
    params: PostCreateParams,
) -> Result<WordPressPost, McpError>
```

**使用例:**
```rust
use mcp_rs::handlers::wordpress::{PostCreateParams, WordPressHandler};
use std::collections::HashMap;

// 基本的な投稿作成
let basic_params = PostCreateParams {
    title: "新しい記事".to_string(),
    content: "記事の内容".to_string(),
    post_type: "post".to_string(),
    status: "publish".to_string(),
    ..Default::default()
};

// SEOメタデータ付きの投稿作成
let mut seo_meta = HashMap::new();
seo_meta.insert("_yoast_wpseo_metadesc".to_string(), "SEO説明".to_string());
seo_meta.insert("_yoast_wpseo_meta-robots-noindex".to_string(), "1".to_string());

let seo_params = PostCreateParams {
    title: "SEO記事".to_string(),
    content: "内容...".to_string(),
    post_type: "post".to_string(),
    status: "draft".to_string(),
    categories: Some(vec![1, 2, 3]),
    tags: Some(vec![10, 11]),
    meta: Some(seo_meta),
    ..Default::default()
};

let post = handler.create_advanced_post(seo_params).await?;
```

### `update_post` 関数

```rust
pub async fn update_post(
    &self,
    post_id: u64,
    params: PostUpdateParams,
) -> Result<WordPressPost, McpError>
```

**使用例:**
```rust
use mcp_rs::handlers::wordpress::{PostUpdateParams, WordPressHandler};

// タイトルのみ更新
let update_params = PostUpdateParams {
    title: Some("新しいタイトル".to_string()),
    ..Default::default()
};

// 複数フィールド更新
let comprehensive_update = PostUpdateParams {
    title: Some("更新されたタイトル".to_string()),
    content: Some("更新された内容".to_string()),
    status: Some("publish".to_string()),
    categories: Some(vec![1, 5]),
    tags: Some(vec![20, 21, 22]),
    featured_media_id: Some(456),
    meta: None,
};

let updated_post = handler.update_post(123, comprehensive_update).await?;
```

### MCP Tool: `create_advanced_post`

MCPツールでは引数をJSONで指定し、内部的に構造体に変換されます。

```json
{
  "name": "create_advanced_post",
  "arguments": {
    "title": "記事タイトル",
    "content": "記事内容",
    "post_type": "post",
    "status": "draft",
    "categories": [1, 5],
    "tags": [10, 15, 20],
    "featured_media_id": 123,
    "meta": {
      "_yoast_wpseo_metadesc": "SEO用メタディスクリプション",
      "_yoast_wpseo_meta-robots-noindex": "1",
      "_yoast_wpseo_meta-robots-nofollow": "1"
    }
  }
}
```

### MCP Tool: `update_post`

```json
{
  "name": "update_post",
  "arguments": {
    "post_id": 123,
    "title": "更新されたタイトル",
    "status": "publish",
    "categories": [1, 2, 3],
    "meta": {
      "_yoast_wpseo_metadesc": "新しいメタディスクリプション"
    }
  }
}
```

## 📅 予約投稿

### 日時形式

ISO8601形式を使用してください：

```
2025-12-25T10:00:00  # 2025年12月25日 10:00
2025-01-01T00:00:00  # 2025年1月1日 00:00
```

### WordPressタイムゾーン

WordPressの設定タイムゾーンに基づいて解釈されます。

## 🎯 AI エージェント使用例

新しい構造体ベースのAPIにより、より明確で保守しやすいコードが書けます。

### 基本的な投稿作成

**User:** "ブログ記事を下書きで作成して"

**AI automatically:**
```json
{
  "tool": "create_advanced_post",
  "arguments": {
    "title": "新しいブログ記事",
    "content": "記事内容...",
    "post_type": "post",
    "status": "draft"
  }
}
```

### SEO最適化投稿

**User:** "SEO設定込みで記事を公開して、noindexにして"

**AI automatically:**
```json
{
  "tool": "create_advanced_post",
  "arguments": {
    "title": "SEO記事",
    "content": "内容...",
    "post_type": "post",
    "status": "publish",
    "meta": {
      "_yoast_wpseo_meta-robots-noindex": "1",
      "_yoast_wpseo_metadesc": "カスタムメタディスクリプション"
    }
  }
}
```

### 予約投稿

**User:** "クリスマス記事を12月25日10時に公開予約して"

**AI automatically:**
```json
{
  "tool": "create_advanced_post",
  "arguments": {
    "title": "クリスマス記事",
    "content": "メリークリスマス！",
    "post_type": "post",
    "status": "future",
    "date": "2025-12-25T10:00:00"
  }
}
```

### 非公開固定ページ

**User:** "会社の内部情報用の非公開ページを作成して"

**AI automatically:**
```json
{
  "tool": "create_advanced_post",
  "arguments": {
    "title": "内部情報",
    "content": "機密情報...",
    "post_type": "page",
    "status": "private"
  }
}
```

### 投稿の部分更新

**User:** "投稿123のタイトルだけ変更して"

**AI automatically:**
```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 123,
    "title": "新しいタイトル"
  }
}
```

### 複合的な更新

**User:** "投稿456を公開状態にして、カテゴリーも追加して"

**AI automatically:**
```json
{
  "tool": "update_post",
  "arguments": {
    "post_id": 456,
    "status": "publish",
    "categories": [1, 3, 5]
  }
}
```

## 📊 コンテンツ管理

### すべてのコンテンツ取得

```json
{
  "tool": "get_all_content"
}
```

### 投稿のみ取得

```json
{
  "tool": "get_posts"
}
```

### 固定ページのみ取得

```json
{
  "tool": "get_pages"
}
```

## ⚠️ 注意事項

1. **権限**: 投稿作成には適切なWordPress権限が必要
2. **プラグイン**: SEOメタデータは対応プラグインが必要
3. **タイムゾーン**: 予約投稿はWordPress設定に依存
4. **バリデーション**: 無効な日時や存在しないIDはエラーになります