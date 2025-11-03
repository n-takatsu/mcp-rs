# WordPress管理者権限設定ガイド

## 🎯 現在の権限状況 (2025-11-03 更新)

### 詳細診断結果
```
✅ 正常動作機能 (wpmaster: 管理者権限確認済み):
- カテゴリーAPI (/wp/v2/categories) - 8件取得成功
- 投稿API (/wp/v2/posts) - 投稿1件、ページ9件取得成功  
- メディアAPI (/wp/v2/media) - 2件取得成功
- タグAPI (/wp/v2/tags) - 6件取得成功

❌ 制限されている機能:
- 設定API (/wp/v2/settings) - 401 Unauthorized

🔍 問題の特定:
管理者権限は正常だが、設定APIのみ個別に制限されている
```

## � 新たに特定された問題: 設定API個別制限

### 管理者権限確認済みでも発生する問題
wpmasterユーザーは**管理者権限を持っている**にも関わらず、`/wp/v2/settings`エンドポイントのみ401エラーが発生。これは以下の原因が考えられます：

### 考えられる原因と解決方法

#### 1. WordPress REST API設定の個別制限
**問題**: 設定APIが特別に制限されている
```php
// functions.php またはプラグインで制限されている可能性
add_filter( 'rest_pre_dispatch', function( $result, $server, $request ) {
    $route = $request->get_route();
    
    // 設定APIへのアクセスを制限
    if ( strpos( $route, '/wp/v2/settings' ) === 0 ) {
        return new WP_Error( 'rest_forbidden', 'Settings API access restricted', 
                           array( 'status' => 401 ) );
    }
    
    return $result;
}, 10, 3 );
```

**解決方法**: 
- テーマの `functions.php` を確認
- プラグインで REST API制限がないかチェック

#### 2. セキュリティプラグインによる制限
**一般的な制限プラグイン**:
- Wordfence Security
- iThemes Security (旧 Better WP Security)  
- Sucuri Security
- All In One WP Security & Firewall

**確認場所**: 
```
セキュリティプラグイン設定 → REST API → Settings Endpoint
または
Firewall → Advanced Rules → REST API Restrictions
```

#### 3. WordPressバージョン固有の制限
**WordPress 5.5+ の厳格化**:
- Application Password での設定API制限強化
- Cookie認証が必要な場合
- WordPress.com hosted での追加制限

#### 4. アプリケーションパスワード権限範囲制限
**確認方法**:
```
WordPress管理画面 → ユーザー → プロフィール
↓
アプリケーションパスワード セクション
↓  
現在のパスワードを「取り消す」
↓
新しいアプリケーションパスワードを生成
```

## 🛡️ セキュリティを考慮した権限設定

### 最小権限の原則に基づく設定

#### オプション1: 管理者権限付与（簡単だが幅広い権限）
```
メリット: 全機能が即座に利用可能
デメリット: 過度な権限付与によるセキュリティリスク
推奨度: テスト環境のみ
```

#### オプション2: カスタムロール作成（推奨）
```
メリット: 必要最小限の権限のみ付与
デメリット: 初期設定がやや複雑
推奨度: 本番環境推奨
```

#### オプション3: 権限制御プラグイン使用
```
推奨プラグイン:
- User Role Editor
- Members
- Capability Manager Enhanced

メリット: GUI で簡単に権限調整可能
デメリット: プラグインへの依存
```

## 📋 段階的実装手順 (管理者権限確認済み対応)

### Phase 1: 即座の診断と対応
```
1. セキュリティプラグインの設定確認
   - Wordfence → Firewall → Advanced Rules
   - iThemes Security → System Tweaks → REST API
   
2. アプリケーションパスワード再生成
   - WordPress管理画面 → ユーザー → プロフィール
   - 現在のパスワードを取り消し → 新規生成
   
3. テーマ functions.php の確認
   - REST API制限フィルターの有無をチェック
   
4. 設定API テスト実行
   cargo run --example comprehensive_test
```

### Phase 2: WordPress環境の確認・調整
```
1. WordPressバージョン確認
   - WordPress 5.5+ の場合、設定API制限強化を確認
   
2. プラグイン個別無効化テスト
   - セキュリティ関連プラグインを一時無効化
   - 設定APIアクセスをテスト
   
3. WordPress REST API 設定確認
   - 管理画面 → 設定 → パーマリンク → 「変更を保存」
   
4. WordPress.com hosted 確認
   - WordPress.com でホストされている場合の追加制限確認
```

### Phase 3: 代替アプローチの実装
```
1. Cookie認証による設定API利用検討
2. WordPress管理画面経由の設定変更フロー
3. 設定API以外の機能での運用継続
4. カスタムエンドポイント作成検討
```

## 🔍 権限確認コマンド

### MCP-RS での権限テスト
```bash
# 包括的権限テスト
cargo run --example comprehensive_test

# 認証診断
cargo run --example auth_diagnosis  

# ヘルスチェック（権限確認含む）
cargo run --example wordpress_health_check
```

### WordPress側での権限確認
```php
// 現在のユーザー権限確認用コード
$user = wp_get_current_user();
$capabilities = $user->allcaps;

// MCP必須権限のチェック
$required_caps = [
    'manage_options',
    'edit_posts', 
    'upload_files',
    'manage_categories'
];

foreach ($required_caps as $cap) {
    if (user_can($user, $cap)) {
        echo "✅ {$cap}: 許可\n";
    } else {
        echo "❌ {$cap}: 拒否\n";
    }
}
```

## 🎯 期待される結果

### 権限設定後のテスト結果（管理者権限確認済み）
```
🔍 WordPress API エンドポイント診断結果:
✅ カテゴリーAPI (/wp/v2/categories) - 8件取得成功
✅ 投稿API (/wp/v2/posts) - 投稿1件、ページ9件取得成功
✅ メディアAPI (/wp/v2/media) - 2件取得成功  
✅ タグAPI (/wp/v2/tags) - 6件取得成功
❌ 設定API (/wp/v2/settings) - 401 Unauthorized ← 個別制限

📊 診断結果まとめ:
🔗 基本接続: 正常
🔐 認証情報: 完全に有効（管理者権限確認済み）
⚙️ 設定API権限: 個別制限により拒否 ← 要調査
```

## 📞 トラブルシューティング

### よくある問題と解決方法

#### 1. 権限変更後も401エラーが継続
```
原因: WordPressキャッシュまたはセッション
解決: 
- WordPressキャッシュクリア
- アプリケーションパスワード再生成
- ブラウザでWordPress管理画面に再ログイン
```

#### 2. 一部の設定APIのみアクセス不可
```
原因: 個別権限の不足またはプラグイン制限
解決:
- User Role Editor で個別権限確認
- セキュリティプラグインの設定確認
- REST API制限プラグインの無効化テスト
```

#### 3. カスタムロール作成後の問題
```
原因: 権限の継承問題
解決:
- wp_roles のリセット
- データベースの wp_options テーブル確認
- プラグイン無効化での動作確認
```

---

**推奨アクション**: まずPhase 1（管理者権限付与）で動作確認を行い、その後セキュリティを考慮したPhase 2（カスタムロール）への移行を検討してください。