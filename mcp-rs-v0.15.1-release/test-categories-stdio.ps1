# MCP-RS カテゴリ取得テスト (STDIO)

$jsonRequest = @{
    "jsonrpc" = "2.0"
    "method" = "resources/read"
    "params" = @{
        "uri" = "wordpress://categories"
    }
    "id" = 1
} | ConvertTo-Json -Depth 3 -Compress

Write-Host "📡 STDIO モードでテスト..."
Write-Host "Request: $jsonRequest"
Write-Host ""

$requestWithHeader = "Content-Length: $($jsonRequest.Length)`r`n`r`n$jsonRequest"

Write-Host "Full request with header:"
Write-Host $requestWithHeader