# VirusTotal Provider テストスクリプト
#
# VirusTotalプロバイダーの単体テストと統合テストを実行します

Write-Host "=== VirusTotal Provider Test Script ===" -ForegroundColor Cyan
Write-Host ""

# 1. 単体テスト実行
Write-Host "📋 Step 1: Running unit tests..." -ForegroundColor Yellow
Write-Host "─────────────────────────────────────────" -ForegroundColor Gray

cargo test --test threat_intelligence virustotal_provider_tests --lib -- --nocapture

if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Unit tests failed!" -ForegroundColor Red
    exit 1
}

Write-Host "✅ Unit tests passed!" -ForegroundColor Green
Write-Host ""

# 2. 統合テスト実行（APIキーが設定されている場合）
Write-Host "📋 Step 2: Running integration tests..." -ForegroundColor Yellow
Write-Host "─────────────────────────────────────────" -ForegroundColor Gray

if ($env:VIRUSTOTAL_API_KEY) {
    Write-Host "🔑 VIRUSTOTAL_API_KEY detected" -ForegroundColor Green
    Write-Host "   Running integration tests with real API..." -ForegroundColor Cyan

    cargo test --test threat_intelligence virustotal_provider_tests -- --ignored --nocapture

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Integration tests failed!" -ForegroundColor Red
        exit 1
    }

    Write-Host "✅ Integration tests passed!" -ForegroundColor Green
} else {
    Write-Host "⚠️  VIRUSTOTAL_API_KEY not set" -ForegroundColor Yellow
    Write-Host "   Skipping integration tests" -ForegroundColor Yellow
    Write-Host "   Set API key with: `$env:VIRUSTOTAL_API_KEY='your_key'" -ForegroundColor Cyan
}

Write-Host ""

# 3. デモアプリケーション実行
Write-Host "📋 Step 3: Running demo application..." -ForegroundColor Yellow
Write-Host "─────────────────────────────────────────" -ForegroundColor Gray

if ($env:VIRUSTOTAL_API_KEY) {
    Write-Host "🚀 Starting VirusTotal demo..." -ForegroundColor Cyan

    cargo run --example virustotal_demo

    if ($LASTEXITCODE -ne 0) {
        Write-Host "❌ Demo failed!" -ForegroundColor Red
        exit 1
    }

    Write-Host "✅ Demo completed successfully!" -ForegroundColor Green
} else {
    Write-Host "⚠️  Skipping demo (API key not set)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "=== All tests completed ===" -ForegroundColor Cyan
Write-Host ""

# テスト結果サマリー
Write-Host "📊 Test Summary:" -ForegroundColor Cyan
Write-Host "   ✅ Unit tests: PASSED" -ForegroundColor Green

if ($env:VIRUSTOTAL_API_KEY) {
    Write-Host "   ✅ Integration tests: PASSED" -ForegroundColor Green
    Write-Host "   ✅ Demo: COMPLETED" -ForegroundColor Green
} else {
    Write-Host "   ⏭️  Integration tests: SKIPPED (no API key)" -ForegroundColor Yellow
    Write-Host "   ⏭️  Demo: SKIPPED (no API key)" -ForegroundColor Yellow
}

Write-Host ""
