# VS Code Workspace Settings for MCP-RS

このフォルダにはmcp-rsプロジェクト用のVS Code設定が含まれています。

## 設定ファイル

### settings.json
- **UTF-8エンコーディング**: BOMなしのUTF-8を強制設定
- **改行コード**: LF（`\n`）に統一（CI互換性）
- **Rust Analyzer**: 最適化された設定
- **自動フォーマット**: 保存時に自動実行
- **ファイル除外**: `target`フォルダなどを非表示

### tasks.json
便利なタスク（Ctrl+Shift+P → "Tasks: Run Task"）:
- **Cargo Build**: `cargo build`
- **Cargo Test**: `cargo test` 
- **Cargo Format Check**: `cargo fmt --check`（CI前チェック）
- **Cargo Clippy**: `cargo clippy`（Linting）
- **Pre-commit Check**: フォーマット + Clippy + テスト
- **Run MCP Server**: STDIO/HTTPモードでサーバー起動

### launch.json
デバッグ設定（F5キー）:
- **Debug mcp-rs**: メインアプリケーションのデバッグ
- **Debug unit tests**: テストのデバッグ
- **Debug with WordPress config**: WordPress設定付きデバッグ

### extensions.json
推奨拡張機能:
- **rust-analyzer**: Rust言語サーバー
- **CodeLLDB**: Rustデバッガ
- **GitLens**: Git統合
- **Error Lens**: インラインエラー表示
- **TOML**: 設定ファイルサポート

## 使用方法

1. VS Codeでプロジェクトを開く
2. 推奨拡張機能のインストールを確認
3. `Ctrl+Shift+P` → "Tasks: Run Task" で各タスクを実行
4. `F5`でデバッグ開始

## MCP設定

### セキュリティ注意事項
- `mcp.json` - 実際のMCP設定（アクセストークン含む）→ **Gitignore対象**
- `mcp.json.example` - サンプル設定ファイル（環境変数参照）→ コミット可能

### 設定方法
1. `mcp.json.example` を `mcp.json` にコピー
2. 環境変数 `GITHUB_PERSONAL_ACCESS_TOKEN` を設定
3. 必要に応じて他のMCPサーバーを追加

## トラブルシューティング

### BOMエラーが再発した場合
```bash
# PowerShellでBOM除去
Get-ChildItem -Path . -Include "*.rs" -Recurse | ForEach-Object { 
    $bytes = [System.IO.File]::ReadAllBytes($_.FullName)
    if ($bytes.Length -ge 3 -and $bytes[0] -eq 0xEF -and $bytes[1] -eq 0xBB -and $bytes[2] -eq 0xBF) { 
        $content = [System.IO.File]::ReadAllBytes($_.FullName)
        $contentWithoutBom = $content[3..($content.Length-1)]
        [System.IO.File]::WriteAllBytes($_.FullName, $contentWithoutBom)
    }
}
```

### フォーマットチェック
```bash
# CI前にローカルで確認
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```