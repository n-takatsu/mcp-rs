# GitHub Project Board - CLI自動化スクリプト

## 前提条件

```powershell
# GitHub CLIのインストール確認
gh --version

# 認証確認
gh auth status
```

## Project作成スクリプト

### 1. Project作成

```powershell
# Project作成（GitHub CLIではGraphQL APIを使用）
gh api graphql -f query='
mutation {
  createProjectV2(input: {
    ownerId: "'"$(gh api user -q .node_id)"'"
    title: "v0.2.0-beta Release"
    repositoryId: "'"$(gh api repos/n-takatsu/mcp-rs -q .node_id)"'"
  }) {
    projectV2 {
      id
      url
    }
  }
}'
```

### 2. カスタムフィールド作成

```powershell
# Priorityフィールド追加
$PROJECT_ID = "取得したProject ID"

gh api graphql -f query='
mutation {
  createProjectV2Field(input: {
    projectId: "'"$PROJECT_ID"'"
    dataType: SINGLE_SELECT
    name: "Priority"
    singleSelectOptions: [
      {name: "P0 (Critical)", color: RED}
      {name: "P1 (High)", color: ORANGE}
      {name: "P2 (Medium)", color: YELLOW}
      {name: "P3 (Low)", color: GREEN}
    ]
  }) {
    projectV2Field {
      id
    }
  }
}'
```

### 3. Issueを追加

```powershell
# Issue #211-214をProjectに追加
$ISSUES = @(211, 212, 213, 214)

foreach ($ISSUE in $ISSUES) {
    gh api graphql -f query='
    mutation {
      addProjectV2ItemByNumber(input: {
        projectId: "'"$PROJECT_ID"'"
        repositoryId: "'"$(gh api repos/n-takatsu/mcp-rs -q .node_id)"'"
        number: '"$ISSUE"'
      }) {
        item {
          id
        }
      }
    }'
}
```

### 4. ステータス設定

```powershell
# すべてのIssueをBacklogに設定
# (Status field IDとBacklog option IDが必要)

$ITEM_IDS = @("取得したItem ID配列")
$STATUS_FIELD_ID = "Status field ID"
$BACKLOG_OPTION_ID = "Backlog option ID"

foreach ($ITEM_ID in $ITEM_IDS) {
    gh api graphql -f query='
    mutation {
      updateProjectV2ItemFieldValue(input: {
        projectId: "'"$PROJECT_ID"'"
        itemId: "'"$ITEM_ID"'"
        fieldId: "'"$STATUS_FIELD_ID"'"
        value: {
          singleSelectOptionId: "'"$BACKLOG_OPTION_ID"'"
        }
      }) {
        projectV2Item {
          id
        }
      }
    }'
}
```

## 完全自動化スクリプト

```powershell
# project-board-setup.ps1

# 設定
$REPO = "n-takatsu/mcp-rs"
$PROJECT_TITLE = "v0.2.0-beta Release"
$ISSUES = @(211, 212, 213, 214)

Write-Host "🚀 GitHub Project Board自動セットアップ開始..." -ForegroundColor Green

# 1. リポジトリとユーザー情報取得
Write-Host "📊 リポジトリ情報取得中..." -ForegroundColor Cyan
$REPO_DATA = gh api repos/$REPO | ConvertFrom-Json
$USER_DATA = gh api user | ConvertFrom-Json

$REPO_ID = $REPO_DATA.node_id
$OWNER_ID = $USER_DATA.node_id

Write-Host "  Repository ID: $REPO_ID" -ForegroundColor Gray
Write-Host "  Owner ID: $OWNER_ID" -ForegroundColor Gray

# 2. Project作成
Write-Host "`n📋 Project作成中..." -ForegroundColor Cyan
$CREATE_PROJECT_QUERY = @"
mutation {
  createProjectV2(input: {
    ownerId: \"$OWNER_ID\"
    title: \"$PROJECT_TITLE\"
  }) {
    projectV2 {
      id
      number
      url
    }
  }
}
"@

$PROJECT_RESULT = gh api graphql -f query="$CREATE_PROJECT_QUERY" | ConvertFrom-Json
$PROJECT_ID = $PROJECT_RESULT.data.createProjectV2.projectV2.id
$PROJECT_URL = $PROJECT_RESULT.data.createProjectV2.projectV2.url

Write-Host "  ✅ Project作成完了: $PROJECT_URL" -ForegroundColor Green

# 3. Issueを追加
Write-Host "`n📝 Issueを追加中..." -ForegroundColor Cyan
foreach ($ISSUE_NUM in $ISSUES) {
    $ADD_ISSUE_QUERY = @"
mutation {
  addProjectV2ItemByNumber(input: {
    projectId: \"$PROJECT_ID\"
    repositoryId: \"$REPO_ID\"
    number: $ISSUE_NUM
  }) {
    item {
      id
    }
  }
}
"@
    
    $RESULT = gh api graphql -f query="$ADD_ISSUE_QUERY" | ConvertFrom-Json
    Write-Host "  ✅ Issue #$ISSUE_NUM 追加完了" -ForegroundColor Green
}

Write-Host "`n🎉 Project Board セットアップ完了!" -ForegroundColor Green
Write-Host "📊 Project URL: $PROJECT_URL" -ForegroundColor Cyan
Write-Host "`n次のステップ:" -ForegroundColor Yellow
Write-Host "  1. Project Boardを開く: $PROJECT_URL"
Write-Host "  2. カラム（Status）を設定: Backlog, In Progress, Review, Done"
Write-Host "  3. カスタムフィールドを追加: Priority, Estimated Effort"
Write-Host "  4. Workflow Automationを設定"

```

## スクリプト実行方法

```powershell
# スクリプトに実行権限を付与
Set-ExecutionPolicy -Scope Process -ExecutionPolicy Bypass

# スクリプト実行
.\scripts\project-board-setup.ps1
```

## 注意事項

- GitHub CLIでのProject V2操作にはGraphQL APIの知識が必要
- Field IDやOption IDは動的に取得する必要がある
- 一部の設定（カラム名、自動化）はWeb UIでの手動設定が必要

## 参考

- [GitHub CLI GraphQL](https://cli.github.com/manual/gh_api)
- [GitHub Projects V2 GraphQL API](https://docs.github.com/en/graphql/reference/mutations#createprojectv2)
