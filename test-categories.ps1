# MCP-RS カテゴリ取得テスト

$jsonRequest = @{
    "jsonrpc" = "2.0"
    "method" = "resources/read"
    "params" = @{
        "uri" = "wordpress://categories"
    }
    "id" = 1
} | ConvertTo-Json -Depth 3

Write-Host "📡 MCP-RS にリクエスト送信中..."
Write-Host "URI: http://127.0.0.1:8080"
Write-Host "Request: $jsonRequest"
Write-Host ""

try {
    $response = Invoke-RestMethod -Uri "http://127.0.0.1:8080" -Method Post -Body $jsonRequest -ContentType "application/json"
    Write-Host "✅ レスポンス受信:"
    $response | ConvertTo-Json -Depth 10
} catch {
    Write-Host "❌ エラー: $($_.Exception.Message)"
    Write-Host "詳細: $($_.Exception.Response)"
}