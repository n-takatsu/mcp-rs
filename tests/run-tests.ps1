# MCP-RS 統合テストランナー

param(
    [string]$TestSuite = "all",          # all, transport, api, integration
    [string]$Transport = "http",         # http, stdio, both
    [string]$ServerUrl = "http://127.0.0.1:8081/mcp",
    [string]$ConfigPath = "configs/development/http-transport.toml",  # 統合後のデフォルト
    [switch]$StartServer = $false,       # 自動でサーバー起動
    [switch]$Verbose = $false,           # 詳細出力
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"

# テスト用の色設定
$Colors = @{
    Header = "Magenta"
    Success = "Green"
    Warning = "Yellow"
    Error = "Red"
    Info = "Cyan"
    Debug = "Gray"
}

function Write-Header {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Text
    )

    Write-Host ""
    Write-Host "=" * 80 -ForegroundColor $Colors.Header
    Write-Host "[TEST] $Text" -ForegroundColor $Colors.Header
    Write-Host "=" * 80 -ForegroundColor $Colors.Header
    Write-Host ""
}

function Write-SubHeader {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Text
    )

    Write-Host ""
    Write-Host "--- $Text ---" -ForegroundColor $Colors.Header
}

function Write-TestResult {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]
        [string]$Suite,

        [Parameter(Mandatory)]
        [bool]$Success,

        [hashtable]$Details = @{}
    )

    $status = if ($Success) { "[PASS]" } else { "[FAIL]" }
    $color = if ($Success) { $Colors.Success } else { $Colors.Error }

    Write-Host "$status $Suite" -ForegroundColor $color

    if ($Verbose -and $Details.Count -gt 0) {
        foreach ($detail in $Details.GetEnumerator()) {
            Write-Host "   $($detail.Key): $($detail.Value)" -ForegroundColor $Colors.Debug
        }
    }
}

function Start-MCPServer {
    Write-SubHeader -Text "Starting MCP-RS Server"

    # 既存プロセスを確認・停止
    $existingProcess = Get-Process -Name "mcp-rs" -ErrorAction SilentlyContinue
    if ($existingProcess) {
        Write-Host "[INFO] Stopping existing MCP-RS process..." -ForegroundColor $Colors.Warning
        Stop-Process -Name "mcp-rs" -Force -ErrorAction SilentlyContinue
        Start-Sleep -Seconds 2
    }

    # サーバー起動
    if (Test-Path ".\target\debug\mcp-rs.exe") {
        $serverPath = ".\target\debug\mcp-rs.exe"
    } elseif (Test-Path ".\target\release\mcp-rs.exe") {
        $serverPath = ".\target\release\mcp-rs.exe"
    } else {
        Write-Host "[ERROR] MCP-RS executable not found. Run 'cargo build' first." -ForegroundColor $Colors.Error
        return $false
    }

    try {
        Write-Host "[INFO] Starting server: $serverPath --config $ConfigPath" -ForegroundColor $Colors.Info
        Start-Process -FilePath $serverPath -ArgumentList "--config", $ConfigPath -WindowStyle Hidden

        # サーバー起動待機
        Write-Host "[INFO] Waiting for server to start..." -ForegroundColor $Colors.Info
        Start-Sleep -Seconds 3

        # 接続テスト
        $testRequest = @{
            "jsonrpc" = "2.0"
            "method" = "initialize"
            "params" = @{
                "protocolVersion" = "2024-11-05"
                "capabilities" = @{}
            }
            "id" = 0
        } | ConvertTo-Json -Compress

        for ($i = 1; $i -le 10; $i++) {
            try {
                Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $testRequest -ContentType "application/json" -TimeoutSec 5 -ErrorAction Stop | Out-Null
                Write-Host "[SUCCESS] Server is ready!" -ForegroundColor $Colors.Success
                return $true
            } catch {
                if ($i -eq 10) {
                    Write-Host "[ERROR] Server failed to start within timeout" -ForegroundColor $Colors.Error
                    return $false
                }
                Start-Sleep -Seconds 2
            }
        }
    } catch {
        Write-Host "[ERROR] Failed to start server: $($_.Exception.Message)" -ForegroundColor $Colors.Error
        return $false
    }
}

function Stop-MCPServer {
    Write-SubHeader -Text "Stopping MCP-RS Server"

    $process = Get-Process -Name "mcp-rs" -ErrorAction SilentlyContinue
    if ($process) {
        Stop-Process -Name "mcp-rs" -Force -ErrorAction SilentlyContinue
        Write-Host "[INFO] Server stopped" -ForegroundColor $Colors.Info
    }
}

function Invoke-TransportTests {
    Write-SubHeader -Text "Transport Tests"

    $results = @{}

    if ($Transport -eq "stdio" -or $Transport -eq "both") {
        try {
            Write-Host "[INFO] Running STDIO transport tests..." -ForegroundColor $Colors.Info
            & ".\transport\test-stdio.ps1" -ConfigPath "..\$ConfigPath" | Out-Null
            $results["STDIO"] = $LASTEXITCODE -eq 0
        } catch {
            $results["STDIO"] = $false
        }
    }

    if ($Transport -eq "http" -or $Transport -eq "both") {
        try {
            Write-Host "[INFO] Running HTTP transport tests..." -ForegroundColor $Colors.Info
            & ".\transport\test-http.ps1" -ServerUrl $ServerUrl -ConfigPath "..\$ConfigPath" -TimeoutSeconds $TimeoutSeconds | Out-Null
            $results["HTTP"] = $LASTEXITCODE -eq 0
        } catch {
            $results["HTTP"] = $false
        }
    }

    return $results
}

function Invoke-APITests {
    Write-SubHeader -Text "API Tests"

    $results = @{}

    try {
        Write-Host "[INFO] Running WordPress API tests..." -ForegroundColor $Colors.Info
        & ".\api\test-wordpress.ps1" -ServerUrl $ServerUrl -TimeoutSeconds $TimeoutSeconds | Out-Null
        $results["WordPress"] = $LASTEXITCODE -eq 0
    } catch {
        $results["WordPress"] = $false
    }

    return $results
}

function Invoke-IntegrationTests {
    Write-SubHeader -Text "Integration Tests"

    # Rust統合テストを実行
    try {
        Write-Host "[INFO] Running Rust integration tests..." -ForegroundColor $Colors.Info
        cargo test --test integration_tests
        $results = @{ "Rust Integration" = $LASTEXITCODE -eq 0 }
    } catch {
        $results = @{ "Rust Integration" = $false }
    }

    return $results
}

# メイン実行
Write-Header -Text "MCP-RS Test Runner v0.15.1"

Write-Host "[INFO] Test Suite: $TestSuite" -ForegroundColor $Colors.Info
Write-Host "[INFO] Transport: $Transport" -ForegroundColor $Colors.Info
Write-Host "[INFO] Server URL: $ServerUrl" -ForegroundColor $Colors.Info
Write-Host "[INFO] Config: $ConfigPath" -ForegroundColor $Colors.Info

$allResults = @{}

# サーバー起動が必要な場合
if ($StartServer -and ($TestSuite -ne "integration")) {
    if (-not (Start-MCPServer)) {
        Write-Host "[ERROR] Failed to start server. Cannot run tests." -ForegroundColor $Colors.Error
        exit 1
    }
}

try {
    # テストスイート実行
    if ($TestSuite -eq "all" -or $TestSuite -eq "transport") {
        $transportResults = Invoke-TransportTests
        foreach ($key in $transportResults.Keys) {
            $allResults[$key] = $transportResults[$key]
        }
    }

    if ($TestSuite -eq "all" -or $TestSuite -eq "integration") {
        $integrationResults = Invoke-IntegrationTests
        foreach ($key in $integrationResults.Keys) {
            $allResults[$key] = $integrationResults[$key]
        }
    }

    if ($TestSuite -eq "all" -or $TestSuite -eq "api") {
        $apiResults = Invoke-APITests
        foreach ($key in $apiResults.Keys) {
            $allResults[$key] = $apiResults[$key]
        }
    }

} finally {
    # サーバー停止
    if ($StartServer) {
        Stop-MCPServer
    }
}

# 結果サマリー
Write-Header -Text "Test Results Summary"

$passed = 0
$total = 0

foreach ($result in $allResults.GetEnumerator()) {
    Write-TestResult -Suite $result.Key -Success $result.Value
    if ($result.Value) { $passed++ }
    $total++
}

Write-Host ""
Write-Host "[INFO] Total Test Suites: $total" -ForegroundColor $Colors.Info
Write-Host "[SUCCESS] Passed: $passed" -ForegroundColor $Colors.Success
Write-Host "[ERROR] Failed: $($total - $passed)" -ForegroundColor $Colors.Error

if ($total -gt 0) {
    $successRate = [math]::Round(($passed / $total) * 100, 1)
    Write-Host "[INFO] Success Rate: $successRate%" -ForegroundColor $(if ($successRate -eq 100) { $Colors.Success } else { $Colors.Warning })
}

if ($passed -eq $total) {
    Write-Host ""
    Write-Host "[SUCCESS] All tests passed successfully!" -ForegroundColor $Colors.Success
    exit 0
} else {
    Write-Host ""
    Write-Host "[WARNING] Some tests failed. Check the output above for details." -ForegroundColor $Colors.Warning
    exit 1
}
