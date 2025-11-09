# HTTP JSON-RPC サーバーテスト

$jsonRequest = @{
    "jsonrpc" = "2.0"
    "method" = "resources/read"
    "params" = @{
        "uri" = "wordpress://categories"
    }
    "id" = 1
} | ConvertTo-Json -Depth 3

Write-Host "📡 HTTP JSON-RPC サーバーをテスト中..."
Write-Host "URL: http://127.0.0.1:8081/mcp"
Write-Host "Request: $jsonRequest"
Write-Host ""

try {
    $response = Invoke-RestMethod -Uri "http://127.0.0.1:8081/mcp" -Method Post -Body $jsonRequest -ContentType "application/json"
    
    Write-Host "✅ レスポンス受信:"
    $response | ConvertTo-Json -Depth 10
    
    Write-Host ""
    Write-Host "📋 カテゴリ一覧:"
    if ($response.result -and $response.result.contents) {
        $categories = $response.result.contents[0].text | ConvertFrom-Json
        foreach ($category in $categories) {
            Write-Host "  - [$($category.id)] $($category.name) ($($category.slug))"
        }
    }
} catch {
    Write-Host "❌ エラー: $($_.Exception.Message)"
    Write-Host "詳細: $($_.ErrorDetails.Message)"
}