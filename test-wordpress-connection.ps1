#!/usr/bin/env pwsh

# WordPress接続テストスクリプト
# 新しいアプリケーションパスワードで接続テストを実行します

Write-Host "=== WordPress接続テスト実行スクリプト ===" -ForegroundColor Green

# パスワードの入力を促す
Write-Host "`n新しいWordPressアプリケーションパスワードを入力してください:" -ForegroundColor Yellow
$password = Read-Host -AsSecureString
$passwordPlain = [Runtime.InteropServices.Marshal]::PtrToStringAuto([Runtime.InteropServices.Marshal]::SecureStringToBSTR($password))

# 環境変数を設定
$env:WORDPRESS_URL = "https://redring.jp"
$env:WORDPRESS_USERNAME = "wpmaster"
$env:WORDPRESS_PASSWORD = $passwordPlain

Write-Host "`n接続テストを開始します..." -ForegroundColor Cyan

# テストを実行
try {
    cargo run --example wordpress_connection_test
    if ($LASTEXITCODE -eq 0) {
        Write-Host "`n✅ 接続テスト成功！" -ForegroundColor Green
        Write-Host "新しいアプリケーションパスワードが正常に動作しています。" -ForegroundColor Green
        
        # 設定ファイルの更新を提案
        Write-Host "`n📝 設定ファイル (mcp-config.toml) の更新を忘れずに行ってください。" -ForegroundColor Yellow
        Write-Host "パスワード部分を新しいものに変更してください。" -ForegroundColor Yellow
    } else {
        Write-Host "`n❌ 接続テスト失敗" -ForegroundColor Red
        Write-Host "アプリケーションパスワードを確認してください。" -ForegroundColor Red
    }
} catch {
    Write-Host "`n❌ テスト実行エラー: $($_.Exception.Message)" -ForegroundColor Red
} finally {
    # セキュリティのため環境変数をクリア
    Remove-Item Env:WORDPRESS_PASSWORD -ErrorAction SilentlyContinue
}

Write-Host "`n=== テスト完了 ===" -ForegroundColor Green