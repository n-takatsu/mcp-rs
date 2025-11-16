# MCP-RS HTTP Transport テスト

param(
    [string]$ServerUrl = "http://127.0.0.1:8081/mcp",
    [string]$ConfigPath = "configs/production/web-ui.toml",  # 統合後のデフォルト
    [string]$TestCase = "all",
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"

function Write-TestHeader($title) {
    Write-Host ""
    Write-Host "=" * 60 -ForegroundColor Cyan
    Write-Host "[TEST] $title" -ForegroundColor Yellow
    Write-Host "=" * 60 -ForegroundColor Cyan
    Write-Host ""
}

function Write-TestResult($test, $success, $message = "", $response = $null) {
    if ($success) {
        Write-Host "[PASS] $test" -ForegroundColor Green
    } else {
        Write-Host "[FAIL] $test" -ForegroundColor Red
    }

    if ($message) {
        Write-Host "   -> $message" -ForegroundColor Yellow
    }

    if ($response -and $PSBoundParameters.ContainsKey('Verbose')) {
        Write-Host "   Response: $($response | ConvertTo-Json -Compress)" -ForegroundColor Gray
    }
}

function Test-ServerConnection {
    Write-TestHeader "Server Connection Test"

    try {
        Invoke-RestMethod -Uri $ServerUrl -Method Post -Body '{"jsonrpc":"2.0","method":"ping","id":0}' -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop | Out-Null
        Write-TestResult "Server Connection" $true "Server responding at $ServerUrl"
        return $true
    } catch {
        Write-TestResult "Server Connection" $false "Failed to connect to $ServerUrl - $($_.Exception.Message)"
        return $false
    }
}

function Test-HttpInitialize {
    Write-TestHeader "HTTP Initialize Test"

    $request = @{
        "jsonrpc" = "2.0"
        "method" = "initialize"
        "params" = @{
            "protocolVersion" = "2024-11-05"
            "capabilities" = @{
                "roots" = @{ "listChanged" = $true }
                "sampling" = @{}
            }
        }
        "id" = 1
    } | ConvertTo-Json -Depth 4 -Compress

    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop

        if ($response.result -and $response.result.protocolVersion) {
            Write-TestResult "HTTP Initialize" $true "Protocol version: $($response.result.protocolVersion)" $response
            return $true
        } else {
            Write-TestResult "HTTP Initialize" $false "Invalid initialize response format" $response
            return $false
        }
    } catch {
        Write-TestResult "HTTP Initialize" $false $_.Exception.Message
        return $false
    }
}

function Test-HttpResourcesList {
    Write-TestHeader "HTTP Resources List Test"

    $request = @{
        "jsonrpc" = "2.0"
        "method" = "resources/list"
        "params" = @{}
        "id" = 2
    } | ConvertTo-Json -Depth 2 -Compress

    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop

        if ($response.result -and $response.result.resources) {
            $resourceCount = $response.result.resources.Count
            Write-TestResult "HTTP Resources List" $true "Found $resourceCount resources" $response
            return $true
        } else {
            Write-TestResult "HTTP Resources List" $false "No resources in response" $response
            return $false
        }
    } catch {
        Write-TestResult "HTTP Resources List" $false $_.Exception.Message
        return $false
    }
}

function Test-HttpResourceRead {
    Write-TestHeader "HTTP Resource Read Test"

    $request = @{
        "jsonrpc" = "2.0"
        "method" = "resources/read"
        "params" = @{
            "uri" = "wordpress://categories"
        }
        "id" = 3
    } | ConvertTo-Json -Depth 3 -Compress

    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop

        if ($response.result -and $response.result.contents) {
            Write-TestResult "HTTP Resource Read (Categories)" $true "Successfully read WordPress categories" $response
            return $true
        } else {
            Write-TestResult "HTTP Resource Read (Categories)" $false "No content in categories response" $response
            return $false
        }
    } catch {
        Write-TestResult "HTTP Resource Read (Categories)" $false $_.Exception.Message
        return $false
    }
}

function Test-HttpToolsList {
    Write-TestHeader "HTTP Tools List Test"

    $request = @{
        "jsonrpc" = "2.0"
        "method" = "tools/list"
        "params" = @{}
        "id" = 4
    } | ConvertTo-Json -Depth 2 -Compress

    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop

        if ($response.result -and $response.result.tools) {
            $toolCount = $response.result.tools.Count
            Write-TestResult "HTTP Tools List" $true "Found $toolCount tools" $response
            return $true
        } else {
            Write-TestResult "HTTP Tools List" $false "No tools in response" $response
            return $false
        }
    } catch {
        Write-TestResult "HTTP Tools List" $false $_.Exception.Message
        return $false
    }
}

function Test-HttpToolCall {
    Write-TestHeader "HTTP Tool Call Test"

    $request = @{
        "jsonrpc" = "2.0"
        "method" = "tools/call"
        "params" = @{
            "name" = "wordpress_get_categories"
            "arguments" = @{}
        }
        "id" = 5
    } | ConvertTo-Json -Depth 3 -Compress

    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop

        if ($response.result) {
            Write-TestResult "HTTP Tool Call (WordPress Categories)" $true "Tool executed successfully" $response
            return $true
        } else {
            Write-TestResult "HTTP Tool Call (WordPress Categories)" $false "No result from tool call" $response
            return $false
        }
    } catch {
        Write-TestResult "HTTP Tool Call (WordPress Categories)" $false $_.Exception.Message
        return $false
    }
}

function Test-HttpResourceReadPosts {
    Write-TestHeader "HTTP Resource Read Posts Test"

    $request = @{
        "jsonrpc" = "2.0"
        "method" = "resources/read"
        "params" = @{
            "uri" = "wordpress://posts?limit=5"
        }
        "id" = 6
    } | ConvertTo-Json -Depth 3 -Compress

    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop

        if ($response.result -and $response.result.contents) {
            Write-TestResult "HTTP Resource Read (Posts)" $true "Successfully read WordPress posts" $response
            return $true
        } else {
            Write-TestResult "HTTP Resource Read (Posts)" $false "No content in posts response" $response
            return $false
        }
    } catch {
        Write-TestResult "HTTP Resource Read (Posts)" $false $_.Exception.Message
        return $false
    }
}

# メインテスト実行
Write-Host "🚀 MCP-RS HTTP Transport Tests" -ForegroundColor Magenta
Write-Host "🌐 Server: $ServerUrl" -ForegroundColor Gray
Write-Host "📝 Config: $ConfigPath" -ForegroundColor Gray
Write-Host "🎯 Test Case: $TestCase" -ForegroundColor Gray
Write-Host "⏱️  Timeout: $TimeoutSeconds seconds" -ForegroundColor Gray

$results = @{}

# 基本接続テスト
$results["Connection"] = Test-ServerConnection

if (-not $results["Connection"]) {
    Write-Host ""
    Write-Host "❌ Server connection failed. Make sure MCP-RS is running with HTTP transport." -ForegroundColor Red
    Write-Host "💡 Try: .\target\debug\mcp-rs.exe --config $ConfigPath" -ForegroundColor Yellow
    exit 1
}

# 個別テスト実行
if ($TestCase -eq "all" -or $TestCase -eq "initialize") {
    $results["Initialize"] = Test-HttpInitialize
}

if ($TestCase -eq "all" -or $TestCase -eq "resources") {
    $results["ResourcesList"] = Test-HttpResourcesList
    $results["ResourceRead"] = Test-HttpResourceRead
    $results["ResourceReadPosts"] = Test-HttpResourceReadPosts
}

if ($TestCase -eq "all" -or $TestCase -eq "tools") {
    $results["ToolsList"] = Test-HttpToolsList
    $results["ToolCall"] = Test-HttpToolCall
}

# 結果サマリー
Write-TestHeader "Test Results Summary"

$passed = ($results.Values | Where-Object { $_ -eq $true }).Count
$total = $results.Count

foreach ($test in $results.GetEnumerator()) {
    $status = if ($test.Value) { "[PASS]" } else { "[FAIL]" }
    Write-Host "$status $($test.Key)" -ForegroundColor $(if ($test.Value) { "Green" } else { "Red" })
}

Write-Host ""
Write-Host "📊 Total Tests: $total" -ForegroundColor Gray
Write-Host "✅ Passed: $passed" -ForegroundColor Green
Write-Host "❌ Failed: $($total - $passed)" -ForegroundColor Red

if ($passed -eq $total) {
    Write-Host ""
    Write-Host "🎉 All HTTP transport tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host ""
    Write-Host "⚠️  Some HTTP transport tests failed." -ForegroundColor Yellow
    exit 1
}
