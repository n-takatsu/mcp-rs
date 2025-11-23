# Runtime Transport Switching Guide

MCP-RSでは**サーバー再起動なし**でSTDIO/HTTP Transportを動的に切り替える機能を実装しました。

## 🎯 実装内容

## 1. 動的Transport管理 (`src/transport/dynamic.rs`)

```rust
// STDIO/HTTP切り替えマネージャー
let transport_manager = DynamicTransportManager::new(initial_config)?;

// STDIOに切り替え（Claude Desktop用）
transport_manager.switch_to_stdio().await?;

// HTTPに切り替え（Web UI用）
transport_manager.switch_to_http(addr).await?;
```

## 2. ランタイム制御 (`src/runtime_control.rs`)

```rust
// ランタイム制御コマンド
pub enum RuntimeCommand {
    SwitchToStdio,
    SwitchToHttp(SocketAddr),
    ReloadConfig,
    ShowStatus,
    Shutdown,
}
```

## 🚀 使用方法

## A. CLIコマンドでの制御

```bash

## 基本起動

cargo run

## STDIO切り替え

cargo run -- --switch-stdio

## HTTP切り替え

cargo run -- --switch-http

## 設定リロード

cargo run -- --reload-config

## ステータス確認

cargo run -- --status
```

## B. 実行時インタラクティブ制御

```bash

## サーバー起動後、別ターミナルで

🎮 MCP-RS Interactive Control
════════════════════════════════════════════════════════════
ランタイム制御コマンド:
  1. STDIO切り替え
  2. HTTP切り替え
  3. 設定リロード
  4. ステータス表示
  9. サーバー終了
  0. 終了

コマンド選択 [1-4, 9, 0]: 1
✅ STDIO切り替えコマンド送信
```

## 🔧 技術的な仕組み

## 1. Transport切り替えフロー

```
現在のTransport停止 → 新Transport作成 → 起動 → 通知
     ⏸️                    🔄              🚀       📢
```

## 2. 設定変更監視

```rust
// 設定ファイル変更を監視
tokio::select! {
    _ = config_manager.get_change_receiver().changed() => {
        // Transport設定が変更されたら自動切り替え
        handle_config_change().await?;
    }
}
```

## 💡 使用シナリオ

## Scenario 1: 開発時の柔軟な切り替え

```bash

## 1. HTTP Transportで開発開始（Web UIでテスト）

cargo run

## 2. Claude Desktopでテストしたい時

## 別ターミナルで

cargo run -- --switch-stdio

## 3. 再度Web UIに戻る時

cargo run -- --switch-http
```

## Scenario 2: 設定ファイル変更による自動切り替え

```toml

## mcp-config.toml - HTTPモード

[transport]
transport_type = { Http = { addr = "127.0.0.1:8081" } }
```

```toml

## mcp-config-claude.toml - STDIOモード

[transport]
transport_type = "Stdio"
[server]
log_level = "error"  

## Claude Desktop対応

```

```bash

## 設定ファイル変更で自動切り替え

cargo run -- --config mcp-config.toml        

## HTTP

cargo run -- --config mcp-config-claude.toml 

## STDIO

```

## ⚠️ 重要な注意点

## Claude Desktop使用時の注意

STDIO Transport使用時は**必ず`log_level="error"`**に設定:

```toml

## mcp-config-claude.toml

[server]
stdio = true
log_level = "error"  

## 標準出力とJSONの混在を防ぐ

[transport]
transport_type = "Stdio"
```

## Transport切り替え時の挙動

1. **現在のTransportは完全停止**
2. **新Transportで再起動**
3. **ハンドラー（WordPress等）は継続**
4. **進行中のリクエストは中断される可能性**

## 🔍 ステータス確認

```bash

## ステータス表示例

📊 MCP-RS Runtime Status
════════════════════════════════════════════════════════════
🚀 Transport情報:
   - タイプ: stdio
   - 状態: ✅ 稼働中

⚙️ 設定情報:
   - ファイル: mcp-config-claude.toml
   - バージョン: 3
```

## 🎯 実装の意義

## Before（従来）

```
STDIO ←→ HTTP切り替え = サーバー再起動必須
     ❌ 面倒            ❌ 開発効率低下
```

## After（新実装）

```
STDIO ←→ HTTP切り替え = ランタイム切り替え
     ✅ 瞬時            ✅ 開発効率向上
```

## 📝 実装統合方法

既存の`main.rs`に統合する場合：

```rust
use mcp_rs::runtime_control::{RuntimeController, RuntimeCommand};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config().await?;

    // 動的Transport管理を追加
    let config_manager = Arc::new(DynamicConfigManager::new(config.clone(), None));
    let (runtime_controller, command_sender) = RuntimeController::new(
        config.transport,
        config_manager,
    )?;

    // ランタイム制御を並列実行
    tokio::spawn(runtime_controller.run());

    // 既存のサーバーロジック...
}
```

これにより、**`stdioのtrue,falseはサーバー再起動でしか設定変更は無理でしょうか？`**の答えは：

**❌ 従来: サーバー再起動必須**
**✅ 新実装: ランタイム切り替え可能**

となります。
