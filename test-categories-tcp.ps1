# MCP-RS カテゴリ取得テスト (TCP直接接続)

try {
    $client = New-Object System.Net.Sockets.TcpClient
    $client.Connect("127.0.0.1", 8080)
    
    $stream = $client.GetStream()
    $writer = New-Object System.IO.StreamWriter($stream)
    $reader = New-Object System.IO.StreamReader($stream)
    
    $jsonRequest = @{
        "jsonrpc" = "2.0"
        "method" = "resources/read"
        "params" = @{
            "uri" = "wordpress://categories"
        }
        "id" = 1
    } | ConvertTo-Json -Depth 3 -Compress
    
    Write-Host "📡 TCP接続でリクエスト送信中..."
    Write-Host "Request: $jsonRequest"
    Write-Host ""
    
    $writer.WriteLine($jsonRequest)
    $writer.Flush()
    
    Write-Host "📦 レスポンス待機中..."
    $response = $reader.ReadLine()
    
    Write-Host "✅ レスポンス受信:"
    Write-Host $response
    
    $responseObj = $response | ConvertFrom-Json
    Write-Host ""
    Write-Host "📋 整形されたレスポンス:"
    $responseObj | ConvertTo-Json -Depth 10
    
} catch {
    Write-Host "❌ エラー: $($_.Exception.Message)"
} finally {
    if ($client) { $client.Close() }
}