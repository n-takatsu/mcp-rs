# AbuseIPDB API Integration Test Script
#
# このスクリプトは実際のAbuseIPDB APIを使用してテストします
# APIキーが必要です: https://www.abuseipdb.com/account/api

param(
    [string]$ApiKey = $env:ABUSEIPDB_API_KEY
)

if (-not $ApiKey) {
    Write-Host "❌ Error: ABUSEIPDB_API_KEY is not set" -ForegroundColor Red
    Write-Host ""
    Write-Host "Please set your API key:" -ForegroundColor Yellow
    Write-Host '  $env:ABUSEIPDB_API_KEY="your_api_key_here"' -ForegroundColor Cyan
    Write-Host ""
    Write-Host "または引数で指定:" -ForegroundColor Yellow
    Write-Host '  .\test-abuseipdb.ps1 -ApiKey "your_api_key_here"' -ForegroundColor Cyan
    exit 1
}

Write-Host "=== AbuseIPDB API Integration Test ===" -ForegroundColor Green
Write-Host ""

# 環境変数を設定
$env:ABUSEIPDB_API_KEY = $ApiKey

Write-Host "📋 Step 1: Running unit tests..." -ForegroundColor Cyan
cargo test --quiet --test threat_intelligence abuseipdb_provider_tests::abuseipdb_tests
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Unit tests failed" -ForegroundColor Red
    exit 1
}
Write-Host "✅ Unit tests passed" -ForegroundColor Green
Write-Host ""

Write-Host "📋 Step 2: Running integration tests (with real API)..." -ForegroundColor Cyan
Write-Host "   This will make actual API calls to AbuseIPDB" -ForegroundColor Yellow
Write-Host ""

cargo test --test threat_intelligence abuseipdb_provider_tests::integration_tests --ignored --nocapture -- --test-threads=1
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Integration tests failed" -ForegroundColor Red
    Write-Host "   Check your API key and network connection" -ForegroundColor Yellow
    exit 1
}
Write-Host "✅ Integration tests passed" -ForegroundColor Green
Write-Host ""

Write-Host "📋 Step 3: Running demo application..." -ForegroundColor Cyan
cargo run --quiet --example abuseipdb_demo
if ($LASTEXITCODE -ne 0) {
    Write-Host "❌ Demo failed" -ForegroundColor Red
    exit 1
}
Write-Host ""
Write-Host "✅ All tests completed successfully!" -ForegroundColor Green
Write-Host ""
Write-Host "🎉 AbuseIPDB API integration is working correctly" -ForegroundColor Green
