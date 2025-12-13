# LLM統合システム

MCP-RSのLLM統合システムは、複数のLLMプロバイダー（OpenAI、Azure OpenAI、ローカルモデル）との統合機能を提供します。

## 📋 目次

- [概要](#概要)
- [機能](#機能)
- [アーキテクチャ](#アーキテクチャ)
- [使用方法](#使用方法)
- [API リファレンス](#apiリファレンス)
- [サンプルコード](#サンプルコード)
- [設定](#設定)
- [トラブルシューティング](#トラブルシューティング)

## 概要

LLM統合システムは、以下の主要機能を提供します：

- **マルチプロバイダー対応**: OpenAI、Azure OpenAI、ローカルLLM
- **ストリーミングレスポンス**: リアルタイムトークン生成
- **セキュアなAPI キー管理**: secrecyクレートによる安全な管理
- **プロバイダー切り替え**: 実行時の動的切り替え
- **高度なパラメータ制御**: 温度、max_tokens、top_pなど

## 機能

### ✅ 実装済み機能

| 機能 | 説明 | 状態 |
|------|------|------|
| OpenAI統合 | GPT-3.5/GPT-4モデルサポート | ✅ |
| Azure OpenAI統合 | エンタープライズ向けAzure統合 | ✅ |
| ストリーミングレスポンス | リアルタイムトークン生成 | ✅ |
| API キー管理 | secrecyによるセキュアな管理 | ✅ |
| プロバイダー切り替え | 実行時の動的切り替え | ✅ |
| 環境変数サポート | `OPENAI_API_KEY`による設定 | ✅ |
| エラーハンドリング | 詳細なエラー型定義 | ✅ |

### 🚧 今後の機能

| 機能 | 説明 | 優先度 |
|------|------|--------|
| ローカルLLM対応 | llama.cpp、candleサポート | 高 |
| カスタムプロバイダー | 任意のOpenAI互換API | 中 |
| トークンカウント | tiktoken-rsによる事前計算 | 中 |
| レート制限 | プロバイダー別制限管理 | 低 |

## アーキテクチャ

```
src/llm/
├── mod.rs              # モジュールルート
├── client.rs           # LlmClient: メインクライアント
├── config.rs           # 設定管理
├── error.rs            # エラー型定義
├── types.rs            # 共通型定義
├── streaming.rs        # ストリーミングヘルパー
└── providers/
    ├── mod.rs          # プロバイダートレイト
    └── openai.rs       # OpenAI実装
```

### コンポーネント構成

```
┌─────────────────────────────────────────┐
│          LlmClient                      │
│  ┌───────────────────────────────────┐  │
│  │  Configuration (LlmConfig)        │  │
│  │  - provider: LlmProvider          │  │
│  │  - api_key: SecretString          │  │
│  │  - model: String                  │  │
│  └───────────────────────────────────┘  │
│                                         │
│  ┌───────────────────────────────────┐  │
│  │  Provider (trait LlmProvider)     │  │
│  │  - complete()                     │  │
│  │  - complete_stream()              │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
                 ↓
    ┌────────────────────────┐
    │  OpenAI / Azure        │
    │  (async-openai)        │
    └────────────────────────┘
```

## 使用方法

### 基本的な使用方法

```rust
use mcp_rs::llm::{client::LlmClient, config::LlmConfig, types::Message};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. 設定を作成
    let config = LlmConfig::openai("your-api-key", "gpt-3.5-turbo");
    
    // 2. クライアントを初期化
    let client = LlmClient::new(config)?;
    
    // 3. シンプルなテキスト完了
    let response = client.complete_text("Hello, how are you?").await?;
    println!("Response: {}", response);
    
    Ok(())
}
```

### 環境変数からの設定

```rust
// OPENAI_API_KEY環境変数から自動設定
let config = LlmConfig::openai_from_env()?;
let client = LlmClient::new(config)?;
```

### 会話形式のリクエスト

```rust
let messages = vec![
    Message::system("You are a helpful assistant"),
    Message::user("What is Rust?"),
];

let response = client.chat(messages).await?;
println!("Response: {}", response.content);
println!("Tokens used: {}", response.usage.total_tokens);
```

### システムプロンプト付き完了

```rust
let response = client.complete_with_system(
    "You are a Rust expert",
    "Explain ownership in simple terms"
).await?;
```

### ストリーミングレスポンス

```rust
use mcp_rs::llm::{
    streaming::StreamHelper,
    types::LlmRequest,
};

let request = LlmRequest::new(messages).with_streaming(true);
let stream = client.complete_stream(request).await?;

// リアルタイム表示
let content = StreamHelper::process_stream(stream, |chunk| {
    print!("{}", chunk);
}).await?;
```

### カスタムパラメータ

```rust
let request = LlmRequest::new(messages)
    .with_temperature(0.8)      // 創造性: 0.0-2.0
    .with_max_tokens(1000)      // 最大トークン数
    .with_model("gpt-4");       // モデル指定

let response = client.complete(request).await?;
```

## API リファレンス

### LlmConfig

```rust
pub struct LlmConfig {
    pub provider: LlmProvider,
    pub default_model: String,
    pub default_temperature: f32,
    pub default_max_tokens: usize,
    pub timeout_secs: u64,
    // ...
}
```

**主要メソッド**:
- `openai(api_key, model)` - OpenAI設定を作成
- `azure_openai(api_key, endpoint, model)` - Azure OpenAI設定
- `local(endpoint, model)` - ローカルLLM設定（予定）
- `openai_from_env()` - 環境変数から設定
- `validate()` - 設定を検証

### LlmClient

```rust
pub struct LlmClient { /* ... */ }
```

**主要メソッド**:
- `new(config)` - 新しいクライアントを作成
- `complete(request)` - 完了リクエスト
- `complete_text(prompt)` - シンプルな完了
- `complete_stream(request)` - ストリーミング完了
- `chat(messages)` - 会話形式の完了
- `switch_provider(config)` - プロバイダー切り替え

### Message

```rust
pub struct Message {
    pub role: Role,
    pub content: String,
}
```

**ヘルパーメソッド**:
- `Message::system(content)` - システムメッセージ
- `Message::user(content)` - ユーザーメッセージ
- `Message::assistant(content)` - アシスタントメッセージ

### LlmRequest

```rust
pub struct LlmRequest {
    pub messages: Vec<Message>,
    pub model: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<usize>,
    pub stream: bool,
}
```

**ビルダーメソッド**:
- `new(messages)` - リクエストを作成
- `with_model(model)` - モデルを設定
- `with_temperature(temp)` - 温度を設定
- `with_max_tokens(tokens)` - 最大トークン数を設定
- `with_streaming(stream)` - ストリーミングを有効化

### LlmResponse

```rust
pub struct LlmResponse {
    pub content: String,
    pub model: String,
    pub usage: TokenUsage,
    pub id: Option<String>,
    pub finish_reason: Option<String>,
}
```

## サンプルコード

### シンプルなチャットボット

```rust
use mcp_rs::llm::{client::LlmClient, config::LlmConfig, types::Message};
use std::io::{self, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = LlmConfig::openai_from_env()?;
    let client = LlmClient::new(config)?;
    
    let mut messages = vec![
        Message::system("You are a helpful assistant"),
    ];
    
    loop {
        print!("You: ");
        io::stdout().flush()?;
        
        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        if input.trim() == "exit" {
            break;
        }
        
        messages.push(Message::user(input.trim()));
        
        let response = client.chat(messages.clone()).await?;
        println!("Assistant: {}", response.content);
        
        messages.push(Message::assistant(response.content));
    }
    
    Ok(())
}
```

### マルチプロバイダー対応アプリ

```rust
async fn create_client(provider_type: &str) -> Result<LlmClient, Box<dyn std::error::Error>> {
    let config = match provider_type {
        "openai" => LlmConfig::openai_from_env()?,
        "azure" => {
            let api_key = std::env::var("AZURE_OPENAI_API_KEY")?;
            let endpoint = std::env::var("AZURE_OPENAI_ENDPOINT")?;
            LlmConfig::azure_openai(api_key, endpoint, "gpt-4")
        },
        _ => return Err("Unknown provider".into()),
    };
    
    Ok(LlmClient::new(config)?)
}
```

### エラーハンドリング

```rust
match client.complete_text("Hello").await {
    Ok(response) => println!("Success: {}", response),
    Err(LlmError::ApiError(msg)) => eprintln!("API Error: {}", msg),
    Err(LlmError::RateLimitError(msg)) => eprintln!("Rate limit: {}", msg),
    Err(LlmError::Timeout(secs)) => eprintln!("Timeout after {}s", secs),
    Err(e) => eprintln!("Error: {}", e),
}
```

## 設定

### 環境変数

| 変数名 | 説明 | 必須 | デフォルト |
|--------|------|------|-----------|
| `OPENAI_API_KEY` | OpenAI APIキー | ✓ | - |
| `OPENAI_MODEL` | デフォルトモデル | | `gpt-3.5-turbo` |
| `AZURE_OPENAI_API_KEY` | Azure OpenAI APIキー | Azure使用時 | - |
| `AZURE_OPENAI_ENDPOINT` | Azure OpenAIエンドポイント | Azure使用時 | - |

### パラメータ範囲

| パラメータ | 範囲 | 推奨値 | 説明 |
|-----------|------|--------|------|
| temperature | 0.0 - 2.0 | 0.7 | 高いほど創造的 |
| max_tokens | 1 - 100000 | 2048 | 生成する最大トークン数 |
| top_p | 0.0 - 1.0 | 1.0 | 核サンプリング |
| timeout_secs | 1 - 300 | 60 | タイムアウト（秒） |

## トラブルシューティング

### よくある問題

#### 1. API キーが見つからない

**エラー**: `ConfigError: OPENAI_API_KEY not set`

**解決策**:
```bash
# 環境変数を設定
export OPENAI_API_KEY="your-api-key"

# または.envファイルを使用
echo "OPENAI_API_KEY=your-api-key" > .env
```

#### 2. レート制限エラー

**エラー**: `RateLimitError: Rate limit exceeded`

**解決策**:
- リクエスト頻度を下げる
- 有料プランにアップグレード
- リトライロジックを実装

```rust
use tokio::time::{sleep, Duration};

for retry in 0..3 {
    match client.complete_text("Hello").await {
        Ok(response) => return Ok(response),
        Err(LlmError::RateLimitError(_)) if retry < 2 => {
            sleep(Duration::from_secs(2u64.pow(retry))).await;
            continue;
        }
        Err(e) => return Err(e),
    }
}
```

#### 3. タイムアウト

**エラー**: `Timeout: Request timeout after 60s`

**解決策**:
```rust
let mut config = LlmConfig::openai_from_env()?;
config.timeout_secs = 120; // タイムアウトを120秒に延長
```

#### 4. トークン制限超過

**エラー**: `TokenLimitExceeded: requested 5000, max 4096`

**解決策**:
```rust
let request = LlmRequest::new(messages)
    .with_max_tokens(2000);  // トークン数を制限
```

## デモ実行

```bash
# LLM統合デモを実行（API キー不要のデモモード）
cargo run --example llm_integration_demo --features llm-integration

# 実際のAPI呼び出しを含むデモ
export OPENAI_API_KEY="your-api-key"
cargo run --example llm_integration_demo --features llm-integration
```

## テスト

```bash
# ユニットテストを実行
cargo test --features llm-integration --test llm_integration_test

# すべてのテストを実行
cargo test --features llm-integration
```

## パフォーマンス

### ベンチマーク結果

| 操作 | 平均時間 | 備考 |
|------|---------|------|
| クライアント初期化 | < 1ms | 初回のみ |
| 通常の完了 | 1-3s | モデル・プロンプトに依存 |
| ストリーミング開始 | < 500ms | 最初のトークンまで |

### 最適化のヒント

1. **接続の再利用**: `LlmClient`インスタンスを再利用
2. **ストリーミング**: 長い応答にはストリーミングを使用
3. **max_tokens制限**: 不要な長い応答を避ける
4. **並列リクエスト**: 独立したリクエストは並列実行

## セキュリティ

### API キー管理

- ✅ `secrecy`クレートによる自動ゼロ化
- ✅ シリアライズ時の自動除外
- ✅ デバッグ出力でのマスキング

### ベストプラクティス

1. API キーをコードにハードコードしない
2. 環境変数または安全な設定ファイルを使用
3. 本番環境ではタイムアウトを設定
4. エラーメッセージに機密情報を含めない

## ライセンス

MIT OR Apache-2.0

## 関連リンク

- [Issue #49 - LLM統合システム開発](https://github.com/n-takatsu/mcp-rs/issues/49)
- [OpenAI API Documentation](https://platform.openai.com/docs/api-reference)
- [async-openai crate](https://docs.rs/async-openai/)
