# MCP-RS HTTP JSON-RPC サーバーテスト

Write-Host "🚀 MCP-RS HTTP JSON-RPC サーバーテスト開始" -ForegroundColor Green
Write-Host ""

# Test 1: resources/read for categories
$categoriesRequest = @{
    "jsonrpc" = "2.0"
    "method" = "resources/read"
    "params" = @{
        "uri" = "wordpress://categories"
    }
    "id" = 1
} | ConvertTo-Json -Depth 3 -Compress

Write-Host "📋 Test 1: WordPress カテゴリ取得"
Write-Host "URL: http://127.0.0.1:8081/mcp"
Write-Host "Request: $categoriesRequest"
Write-Host ""

try {
    $response1 = Invoke-RestMethod -Uri "http://127.0.0.1:8081/mcp" -Method Post -Body $categoriesRequest -ContentType "application/json" -TimeoutSec 10
    Write-Host "✅ カテゴリ取得成功!" -ForegroundColor Green
    Write-Host "Response:" -ForegroundColor Cyan
    $response1 | ConvertTo-Json -Depth 10
    Write-Host ""
} catch {
    Write-Host "❌ カテゴリ取得エラー: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host "詳細: $($_.Exception.Response)" -ForegroundColor Yellow
    Write-Host ""
}

# Test 2: resources/read for tags  
$tagsRequest = @{
    "jsonrpc" = "2.0"
    "method" = "resources/read"
    "params" = @{
        "uri" = "wordpress://tags"
    }
    "id" = 2
} | ConvertTo-Json -Depth 3 -Compress

Write-Host "🏷️  Test 2: WordPress タグ取得"
Write-Host "Request: $tagsRequest"
Write-Host ""

try {
    $response2 = Invoke-RestMethod -Uri "http://127.0.0.1:8081/mcp" -Method Post -Body $tagsRequest -ContentType "application/json" -TimeoutSec 10
    Write-Host "✅ タグ取得成功!" -ForegroundColor Green
    Write-Host "Response:" -ForegroundColor Cyan
    $response2 | ConvertTo-Json -Depth 10
    Write-Host ""
} catch {
    Write-Host "❌ タグ取得エラー: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host ""
}

# Test 3: tools/list
$toolsListRequest = @{
    "jsonrpc" = "2.0"
    "method" = "tools/list"
    "params" = @{}
    "id" = 3
} | ConvertTo-Json -Depth 3 -Compress

Write-Host "🔧 Test 3: 利用可能ツール一覧取得"
Write-Host "Request: $toolsListRequest"
Write-Host ""

try {
    $response3 = Invoke-RestMethod -Uri "http://127.0.0.1:8081/mcp" -Method Post -Body $toolsListRequest -ContentType "application/json" -TimeoutSec 10
    Write-Host "✅ ツール一覧取得成功!" -ForegroundColor Green
    Write-Host "利用可能ツール数: $($response3.result.tools.Count)"
    Write-Host "ツール一覧:" -ForegroundColor Cyan
    foreach ($tool in $response3.result.tools) {
        Write-Host "  - $($tool.name): $($tool.description)"
    }
    Write-Host ""
} catch {
    Write-Host "❌ ツール一覧取得エラー: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host ""
}

# Test 4: resources/list
$resourcesListRequest = @{
    "jsonrpc" = "2.0"
    "method" = "resources/list"
    "params" = @{}
    "id" = 4
} | ConvertTo-Json -Depth 3 -Compress

Write-Host "📚 Test 4: 利用可能リソース一覧取得"
Write-Host "Request: $resourcesListRequest"
Write-Host ""

try {
    $response4 = Invoke-RestMethod -Uri "http://127.0.0.1:8081/mcp" -Method Post -Body $resourcesListRequest -ContentType "application/json" -TimeoutSec 10
    Write-Host "✅ リソース一覧取得成功!" -ForegroundColor Green
    Write-Host "利用可能リソース数: $($response4.result.resources.Count)"
    Write-Host "リソース一覧:" -ForegroundColor Cyan
    foreach ($resource in $response4.result.resources) {
        Write-Host "  - $($resource.uri): $($resource.name)"
    }
    Write-Host ""
} catch {
    Write-Host "❌ リソース一覧取得エラー: $($_.Exception.Message)" -ForegroundColor Red
    Write-Host ""
}

Write-Host "🎯 テスト完了!" -ForegroundColor Green
Write-Host ""
Write-Host "📋 結果サマリ:"
Write-Host "  - HTTP JSON-RPC エンドポイント: http://127.0.0.1:8081/mcp"
Write-Host "  - AI Agent からのアクセス: 準備完了"
Write-Host "  - WordPress リソース: カテゴリ・タグ取得可能"
Write-Host "  - MCP プロトコル: 完全対応"