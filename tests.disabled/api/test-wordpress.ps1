# MCP-RS WordPress API テスト

param(
    [string]$ServerUrl = "http://127.0.0.1:8081/mcp",
    [string]$TestCase = "all",
    [int]$TimeoutSeconds = 30
)

$ErrorActionPreference = "Stop"

function Write-TestHeader($title) {
    Write-Host ""
    Write-Host "=" * 60 -ForegroundColor Cyan
    Write-Host "🧪 $title" -ForegroundColor Yellow
    Write-Host "=" * 60 -ForegroundColor Cyan
    Write-Host ""
}

function Write-TestResult($test, $success, $message = "", $data = $null) {
    if ($success) {
        Write-Host "✅ $test" -ForegroundColor Green
    } else {
        Write-Host "❌ $test" -ForegroundColor Red
    }
    
    if ($message) {
        Write-Host "   💬 $message" -ForegroundColor Yellow
    }
    
    if ($data -and $PSBoundParameters.ContainsKey('Verbose')) {
        Write-Host "   📄 Data: $($data | ConvertTo-Json -Compress)" -ForegroundColor Gray
    }
}

function Invoke-MCPRequest($method, $params = @{}, $id = 1) {
    $request = @{
        "jsonrpc" = "2.0"
        "method" = $method
        "params" = $params
        "id" = $id
    } | ConvertTo-Json -Depth 5 -Compress
    
    try {
        $response = Invoke-RestMethod -Uri $ServerUrl -Method Post -Body $request -ContentType "application/json" -TimeoutSec $TimeoutSeconds -ErrorAction Stop
        return $response
    } catch {
        throw $_.Exception.Message
    }
}

function Test-WordPressCategories {
    Write-TestHeader "WordPress Categories Test"
    
    try {
        # Resource読み取りテスト
        $response = Invoke-MCPRequest "resources/read" @{ "uri" = "wordpress://categories" } 1
        
        if ($response.result -and $response.result.contents) {
            $content = $response.result.contents[0].text | ConvertFrom-Json
            $categoryCount = $content.Count
            Write-TestResult "WordPress Categories Resource" $true "Found $categoryCount categories" $content
            
            # 最初のカテゴリの詳細をチェック
            if ($categoryCount -gt 0) {
                $firstCategory = $content[0]
                if ($firstCategory.id -and $firstCategory.name) {
                    Write-TestResult "Category Data Structure" $true "Valid category object with id and name"
                } else {
                    Write-TestResult "Category Data Structure" $false "Missing required fields in category object"
                }
            }
            
            return $true
        } else {
            Write-TestResult "WordPress Categories Resource" $false "No categories data in response"
            return $false
        }
    } catch {
        Write-TestResult "WordPress Categories Resource" $false $_.Exception.Message
        return $false
    }
}

function Test-WordPressPosts {
    Write-TestHeader "WordPress Posts Test"
    
    try {
        # 最新投稿取得
        $response = Invoke-MCPRequest "resources/read" @{ "uri" = "wordpress://posts?limit=10" } 2
        
        if ($response.result -and $response.result.contents) {
            $content = $response.result.contents[0].text | ConvertFrom-Json
            $postCount = $content.Count
            Write-TestResult "WordPress Posts Resource" $true "Found $postCount posts" $content
            
            # 最初の投稿の詳細をチェック
            if ($postCount -gt 0) {
                $firstPost = $content[0]
                $requiredFields = @('id', 'title', 'content', 'status', 'date')
                $missingFields = @()
                
                foreach ($field in $requiredFields) {
                    if (-not ($firstPost.PSObject.Properties.Name -contains $field)) {
                        $missingFields += $field
                    }
                }
                
                if ($missingFields.Count -eq 0) {
                    Write-TestResult "Post Data Structure" $true "All required fields present"
                } else {
                    Write-TestResult "Post Data Structure" $false "Missing fields: $($missingFields -join ', ')"
                }
            }
            
            return $true
        } else {
            Write-TestResult "WordPress Posts Resource" $false "No posts data in response"
            return $false
        }
    } catch {
        Write-TestResult "WordPress Posts Resource" $false $_.Exception.Message
        return $false
    }
}

function Test-WordPressTools {
    Write-TestHeader "WordPress Tools Test"
    
    try {
        # ツールリスト取得
        $response = Invoke-MCPRequest "tools/list" @{} 3
        
        if ($response.result -and $response.result.tools) {
            $tools = $response.result.tools
            $wpTools = $tools | Where-Object { $_.name -like "*wordpress*" }
            
            Write-TestResult "WordPress Tools List" $true "Found $($wpTools.Count) WordPress tools"
            
            # 期待されるWordPressツールをチェック
            $expectedTools = @(
                "wordpress_get_categories",
                "wordpress_get_posts", 
                "wordpress_create_post",
                "wordpress_search_posts"
            )
            
            $foundTools = @()
            foreach ($expectedTool in $expectedTools) {
                $tool = $wpTools | Where-Object { $_.name -eq $expectedTool }
                if ($tool) {
                    $foundTools += $expectedTool
                    Write-TestResult "Tool: $expectedTool" $true "Available"
                } else {
                    Write-TestResult "Tool: $expectedTool" $false "Not found"
                }
            }
            
            return $foundTools.Count -eq $expectedTools.Count
        } else {
            Write-TestResult "WordPress Tools List" $false "No tools in response"
            return $false
        }
    } catch {
        Write-TestResult "WordPress Tools List" $false $_.Exception.Message
        return $false
    }
}

function Test-WordPressToolCall {
    Write-TestHeader "WordPress Tool Call Test"
    
    try {
        # wordpress_get_categories ツールを呼び出し
        $response = Invoke-MCPRequest "tools/call" @{
            "name" = "wordpress_get_categories"
            "arguments" = @{}
        } 4
        
        if ($response.result) {
            Write-TestResult "WordPress Get Categories Tool" $true "Tool executed successfully"
            
            # レスポンスの内容をチェック
            if ($response.result.content -and $response.result.content[0].text) {
                $categories = $response.result.content[0].text | ConvertFrom-Json
                Write-TestResult "Tool Response Data" $true "Found $($categories.Count) categories via tool call"
                return $true
            } else {
                Write-TestResult "Tool Response Data" $false "No category data in tool response"
                return $false
            }
        } else {
            Write-TestResult "WordPress Get Categories Tool" $false "Tool call failed"
            return $false
        }
    } catch {
        Write-TestResult "WordPress Get Categories Tool" $false $_.Exception.Message
        return $false
    }
}

function Test-WordPressSearch {
    Write-TestHeader "WordPress Search Test"
    
    try {
        # 投稿検索ツール
        $response = Invoke-MCPRequest "tools/call" @{
            "name" = "wordpress_search_posts"
            "arguments" = @{
                "search" = "test"
                "limit" = 5
            }
        } 5
        
        if ($response.result) {
            Write-TestResult "WordPress Search Posts Tool" $true "Search executed successfully"
            
            if ($response.result.content -and $response.result.content[0].text) {
                $posts = $response.result.content[0].text | ConvertFrom-Json
                Write-TestResult "Search Results" $true "Found $($posts.Count) posts matching 'test'"
                return $true
            } else {
                Write-TestResult "Search Results" $true "No posts found (expected for empty search)"
                return $true
            }
        } else {
            Write-TestResult "WordPress Search Posts Tool" $false "Search tool call failed"
            return $false
        }
    } catch {
        Write-TestResult "WordPress Search Posts Tool" $false $_.Exception.Message
        return $false
    }
}

# メインテスト実行
Write-Host "🚀 MCP-RS WordPress API Tests" -ForegroundColor Magenta
Write-Host "🌐 Server: $ServerUrl" -ForegroundColor Gray
Write-Host "🎯 Test Case: $TestCase" -ForegroundColor Gray

$results = @{}

# 基本接続確認
try {
    Invoke-MCPRequest "initialize" @{
        "protocolVersion" = "2024-11-05"
        "capabilities" = @{
            "roots" = @{ "listChanged" = $true }
            "sampling" = @{}
        }
    } 0 | Out-Null
    Write-Host "✅ MCP Server connection established" -ForegroundColor Green
} catch {
    Write-Host "❌ Failed to connect to MCP server: $_" -ForegroundColor Red
    exit 1
}

# 個別テスト実行
if ($TestCase -eq "all" -or $TestCase -eq "categories") {
    $results["Categories"] = Test-WordPressCategories
}

if ($TestCase -eq "all" -or $TestCase -eq "posts") {
    $results["Posts"] = Test-WordPressPosts
}

if ($TestCase -eq "all" -or $TestCase -eq "tools") {
    $results["Tools"] = Test-WordPressTools
}

if ($TestCase -eq "all" -or $TestCase -eq "toolcall") {
    $results["ToolCall"] = Test-WordPressToolCall
}

if ($TestCase -eq "all" -or $TestCase -eq "search") {
    $results["Search"] = Test-WordPressSearch
}

# 結果サマリー
Write-TestHeader "Test Results Summary"

$passed = ($results.Values | Where-Object { $_ -eq $true }).Count
$total = $results.Count

foreach ($test in $results.GetEnumerator()) {
    $status = if ($test.Value) { "✅" } else { "❌" }
    Write-Host "$status $($test.Key)" -ForegroundColor $(if ($test.Value) { "Green" } else { "Red" })
}

Write-Host ""
Write-Host "📊 Total Tests: $total" -ForegroundColor Gray
Write-Host "✅ Passed: $passed" -ForegroundColor Green
Write-Host "❌ Failed: $($total - $passed)" -ForegroundColor Red

if ($passed -eq $total) {
    Write-Host ""
    Write-Host "🎉 All WordPress API tests passed!" -ForegroundColor Green
    exit 0
} else {
    Write-Host ""
    Write-Host "⚠️  Some WordPress API tests failed." -ForegroundColor Yellow
    exit 1
}