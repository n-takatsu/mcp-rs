# WordPress Application Password 設定ガイド

## 📋 Application Password 作成手順

### 1. WordPress 管理画面にログイン

### 2. ユーザー設定で Application Password を作成

```
WordPress管理画面 → ユーザー → プロフィール → Application Passwords
```

### 3. 新しい Application Password を追加

-   **Application Name**: `mcp-rs` (識別用の名前)
-   **Add New Application Password** をクリック

### 4. 生成されたパスワードをコピー

```
例: xxxx xxxx xxxx xxxx xxxx xxxx
```

## 🔒 セキュリティ上の利点

### Application Password の利点:

-   ✅ メインパスワードを使用しない
-   ✅ 個別に無効化可能
-   ✅ 特定のアプリケーション用途に限定
-   ✅ REST API 専用認証
-   ✅ 2FA が有効でも使用可能

### 従来の Basic 認証と比較:

-   ❌ メインパスワードをそのまま使用（危険）
-   ❌ 2FA が有効だと使用不可
-   ❌ パスワード変更時に全て無効化

## ⚙️ 設定例

### 推奨設定:

```json
{
    "auth": {
        "type": "application_password",
        "username": "your-username",
        "password": "xxxx xxxx xxxx xxxx xxxx xxxx"
    }
}
```

### 開発・テスト用（セキュリティ低）:

```json
{
    "auth": {
        "type": "basic",
        "username": "your-username",
        "password": "your-main-password"
    }
}
```

## 🚨 セキュリティ注意事項

1. **設定ファイルの保護**

    - 設定ファイルを `.gitignore` に追加
    - ファイル権限を制限 (600)

2. **Application Password の管理**

    - 定期的なローテーション
    - 不要になったら即座に削除
    - 用途ごとに個別作成

3. **HTTPS の必須使用**
    - WordPress サイトは必ず HTTPS
    - 認証情報の暗号化通信
