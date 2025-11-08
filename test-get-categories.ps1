# MCP-RS get_categories ツールテスト

$jsonRequest = @{
    "jsonrpc" = "2.0"
    "method" = "tools/call"
    "params" = @{
        "name" = "get_categories"
        "arguments" = @{}
    }
    "id" = 1
} | ConvertTo-Json -Depth 3 -Compress

Write-Host "📡 get_categories ツールを実行..."
Write-Host "Request: $jsonRequest"

try {
    $client = New-Object System.Net.Sockets.TcpClient
    $client.Connect("127.0.0.1", 8080)
    
    $stream = $client.GetStream()
    $writer = New-Object System.IO.StreamWriter($stream)
    $reader = New-Object System.IO.StreamReader($stream)
    
    $writer.WriteLine($jsonRequest)
    $writer.Flush()
    
    $response = $reader.ReadLine()
    Write-Host "✅ レスポンス:"
    Write-Host $response
    
} catch {
    Write-Host "❌ エラー: $($_.Exception.Message)"
} finally {
    if ($client) { $client.Close() }
}