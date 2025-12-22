# LLM統合ガイド

MCP-RS WebSocketを使用したLLM（大規模言語モデル）統合ガイド

## 目次

- [概要](#概要)
- [LLMストリーミングの仕組み](#llmストリーミングの仕組み)
- [基本的な実装](#基本的な実装)
- [高度な機能](#高度な機能)
- [ベストプラクティス](#ベストプラクティス)
- [トラブルシューティング](#トラブルシューティング)

---

## 概要

MCP-RS WebSocketのLLMストリーミング機能は、大規模言語モデルの応答を効率的にストリーミング配信するための機能です。

### 主な機能

- **チャンクベースストリーミング**: 長い応答を小さなチャンクに分割
- **レイテンシ制御**: チャンク間の遅延を設定可能
- **バックプレッシャー対応**: クライアントの受信速度に合わせて調整
- **エラーハンドリング**: ストリーミング中断時の適切な処理
- **メトリクス統合**: ストリーミングパフォーマンスの監視

---

## LLMストリーミングの仕組み

### 従来の応答方式（非ストリーミング）

```
クライアント                              サーバー
    |                                        |
    |-------- リクエスト送信 ---------------->|
    |                                        | LLM処理中...
    |                                        | (長時間待機)
    |                                        |
    |<------- 完全な応答を一度に受信 ---------|
    |                                        |
```

**問題点**:

- 最初の応答まで長時間待機
- ユーザーエクスペリエンスが悪い
- ネットワークタイムアウトのリスク

### ストリーミング方式

```
クライアント                              サーバー
    |                                        |
    |-------- リクエスト送信 ---------------->|
    |                                        | LLM処理開始
    |<------- チャンク1 --------------------|
    |<------- チャンク2 --------------------|
    |<------- チャンク3 --------------------|
    |          ...                           |
    |<------- チャンクN（完了） -------------|
    |                                        |
```

**利点**:

- 即座にフィードバック開始
- 優れたユーザーエクスペリエンス
- 長い応答でも安定動作

---

## 基本的な実装

### 1. シンプルなLLMストリーミングサーバー

```rust
use axum::{
    extract::ws::{WebSocket, WebSocketUpgrade},
    routing::get,
    Router,
};
use mcp_rs::transport::websocket::{LlmStreamConfig, LlmStreamer};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize)]
struct ChatRequest {
    message: String,
}

#[derive(Serialize)]
struct ChatResponse {
    content: String,
    done: bool,
}

#[tokio::main]
async fn main() {
    // LLMストリーマー設定
    let llm_config = LlmStreamConfig {
        chunk_size: 20,      // 1チャンクあたり20文字
        delay_ms: 50,        // チャンク間50ms遅延
        ..Default::default()
    };
    
    let streamer = Arc::new(LlmStreamer::new(llm_config));

    // ルーター構築
    let app = Router::new()
        .route("/chat", get(chat_handler))
        .with_state(streamer);

    // サーバー起動
    axum::Server::bind(&"127.0.0.1:3000".parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn chat_handler(
    ws: WebSocketUpgrade,
    State(streamer): State<Arc<LlmStreamer>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_chat(socket, streamer))
}

async fn handle_chat(mut socket: WebSocket, streamer: Arc<LlmStreamer>) {
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            // リクエスト解析
            let request: ChatRequest = serde_json::from_str(&text).unwrap();
            
            // LLM応答生成（実際にはLLM APIを呼び出す）
            let llm_response = generate_llm_response(&request.message).await;
            
            // ストリーミング開始
            let stream_id = streamer.start_stream(&llm_response).await.unwrap();
            
            // チャンクを順次送信
            loop {
                match streamer.next_chunk(&stream_id).await {
                    Ok(Some(chunk)) => {
                        let response = ChatResponse {
                            content: chunk,
                            done: false,
                        };
                        
                        let json = serde_json::to_string(&response).unwrap();
                        socket.send(Message::Text(json)).await.ok();
                    }
                    Ok(None) => {
                        // ストリーミング完了
                        let done_response = ChatResponse {
                            content: String::new(),
                            done: true,
                        };
                        
                        let json = serde_json::to_string(&done_response).unwrap();
                        socket.send(Message::Text(json)).await.ok();
                        break;
                    }
                    Err(e) => {
                        eprintln!("Streaming error: {}", e);
                        break;
                    }
                }
            }
            
            // ストリーム終了
            streamer.end_stream(&stream_id).await.ok();
        }
    }
}

// LLM応答生成（実際のLLM APIに置き換える）
async fn generate_llm_response(message: &str) -> String {
    format!(
        "You asked: '{}'. This is a simulated LLM response. \
         In a real implementation, this would call an LLM API like OpenAI, Claude, or Llama.",
        message
    )
}
```

### 2. クライアント側の実装（JavaScript）

```javascript
const ws = new WebSocket('ws://localhost:3000/chat');

let fullResponse = '';

ws.onopen = () => {
    console.log('Connected to LLM chat server');
    
    // チャットメッセージ送信
    ws.send(JSON.stringify({
        message: 'Explain quantum computing in simple terms'
    }));
};

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    
    if (data.done) {
        console.log('Streaming complete!');
        console.log('Full response:', fullResponse);
    } else {
        // チャンクを受信
        fullResponse += data.content;
        
        // UIに逐次表示
        updateChatUI(data.content);
    }
};

ws.onerror = (error) => {
    console.error('WebSocket error:', error);
};

function updateChatUI(chunk) {
    // DOMに追加（例）
    const chatBox = document.getElementById('chat-box');
    chatBox.textContent += chunk;
}
```

---

## 高度な機能

### 1. 実際のLLM APIとの統合（OpenAI）

```rust
use reqwest::Client;
use serde_json::json;
use futures_util::StreamExt;

async fn generate_openai_response(message: &str, api_key: &str) -> String {
    let client = Client::new();
    
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": message}
            ],
            "stream": false  // まず非ストリーミングで取得
        }))
        .send()
        .await
        .unwrap();
    
    let data: serde_json::Value = response.json().await.unwrap();
    data["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .to_string()
}

// OpenAI のストリーミングAPIを直接使用する場合
async fn stream_openai_response(
    message: &str,
    api_key: &str,
    socket: &mut WebSocket,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    
    let mut stream = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&json!({
            "model": "gpt-4",
            "messages": [
                {"role": "user", "content": message}
            ],
            "stream": true  // ストリーミング有効
        }))
        .send()
        .await?
        .bytes_stream();
    
    // SSEイベントをパースしてWebSocketで送信
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        let text = String::from_utf8_lossy(&chunk);
        
        // "data: " プレフィックスを削除してJSONパース
        for line in text.lines() {
            if line.starts_with("data: ") {
                let json_str = &line[6..];
                
                if json_str == "[DONE]" {
                    break;
                }
                
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(content) = data["choices"][0]["delta"]["content"].as_str() {
                        let response = ChatResponse {
                            content: content.to_string(),
                            done: false,
                        };
                        
                        socket.send(Message::Text(
                            serde_json::to_string(&response)?
                        )).await?;
                    }
                }
            }
        }
    }
    
    Ok(())
}
```

### 2. エラーハンドリングとリトライ

```rust
use tokio::time::{sleep, Duration};

async fn handle_chat_with_retry(
    mut socket: WebSocket,
    streamer: Arc<LlmStreamer>,
    max_retries: usize,
) {
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            let request: ChatRequest = serde_json::from_str(&text).unwrap();
            
            let mut retries = 0;
            loop {
                match generate_llm_response_with_timeout(&request.message).await {
                    Ok(llm_response) => {
                        // ストリーミング送信
                        stream_response(&mut socket, &streamer, &llm_response).await;
                        break;
                    }
                    Err(e) if retries < max_retries => {
                        // リトライ
                        retries += 1;
                        eprintln!("LLM API error (retry {}/{}): {}", retries, max_retries, e);
                        sleep(Duration::from_secs(1 << retries)).await;  // 指数バックオフ
                    }
                    Err(e) => {
                        // 最大リトライ超過
                        let error_response = ChatResponse {
                            content: format!("Error: {}", e),
                            done: true,
                        };
                        socket.send(Message::Text(
                            serde_json::to_string(&error_response).unwrap()
                        )).await.ok();
                        break;
                    }
                }
            }
        }
    }
}

async fn generate_llm_response_with_timeout(
    message: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    // タイムアウト付きLLM呼び出し
    tokio::time::timeout(
        Duration::from_secs(30),
        generate_llm_response(message),
    )
    .await
    .map_err(|_| "LLM API timeout".into())
}
```

### 3. チャンクサイズの動的調整

```rust
impl LlmStreamer {
    pub async fn adjust_chunk_size_by_network(&self, latency_ms: u64) {
        let new_chunk_size = if latency_ms < 50 {
            10  // 低レイテンシ: 小さいチャンク
        } else if latency_ms < 200 {
            20  // 中レイテンシ: 中サイズチャンク
        } else {
            50  // 高レイテンシ: 大きいチャンク
        };
        
        // 設定更新（実装は省略）
    }
}
```

---

## ベストプラクティス

### 1. 適切なチャンクサイズ

**推奨値**:
```rust
LlmStreamConfig {
    chunk_size: 20,  // 20文字/チャンク
    delay_ms: 50,    // 50ms間隔
    // ...
}
```

**考慮事項**:

- **小さすぎる**: オーバーヘッド増加
- **大きすぎる**: ストリーミング効果が薄れる
- **ネットワーク状態**: 遅いネットワークでは大きめに

### 2. タイムアウトの設定

```rust
use tokio::time::timeout;

let result = timeout(
    Duration::from_secs(30),
    streamer.next_chunk(&stream_id),
).await;

match result {
    Ok(Ok(Some(chunk))) => {
        // チャンク送信
    }
    Ok(Ok(None)) => {
        // 完了
    }
    Ok(Err(e)) => {
        // ストリーミングエラー
    }
    Err(_) => {
        // タイムアウト
    }
}
```

### 3. メトリクス収集

```rust
use mcp_rs::transport::websocket::WebSocketMetrics;

async fn stream_with_metrics(
    socket: &mut WebSocket,
    streamer: &LlmStreamer,
    metrics: &WebSocketMetrics,
    response: &str,
) {
    let stream_id = streamer.start_stream(response).await.unwrap();
    let start = std::time::Instant::now();
    
    let mut chunk_count = 0;
    
    loop {
        match streamer.next_chunk(&stream_id).await {
            Ok(Some(chunk)) => {
                chunk_count += 1;
                socket.send(Message::Text(chunk)).await.ok();
                metrics.increment_messages_sent();
            }
            Ok(None) => {
                break;
            }
            Err(e) => {
                metrics.increment_errors();
                break;
            }
        }
    }
    
    let duration = start.elapsed();
    metrics.observe_latency(duration.as_secs_f64());
    
    println!(
        "Streamed {} chunks in {:.2}s ({:.1} chunks/sec)",
        chunk_count,
        duration.as_secs_f64(),
        chunk_count as f64 / duration.as_secs_f64()
    );
    
    streamer.end_stream(&stream_id).await.ok();
}
```

### 4. バックプレッシャー対応

```rust
use tokio::sync::mpsc;

async fn stream_with_backpressure(
    socket: &mut WebSocket,
    llm_response: String,
) {
    let (tx, mut rx) = mpsc::channel::<String>(10);  // バッファサイズ10
    
    // チャンク生成タスク
    tokio::spawn(async move {
        for chunk in llm_response.chars().collect::<Vec<_>>().chunks(20) {
            let chunk_str: String = chunk.iter().collect();
            
            // チャネルがいっぱいなら待機（バックプレッシャー）
            if tx.send(chunk_str).await.is_err() {
                break;
            }
            
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });
    
    // チャンク送信
    while let Some(chunk) = rx.recv().await {
        if socket.send(Message::Text(chunk)).await.is_err() {
            break;
        }
    }
}
```

### 5. クライアント側バッファリング

```javascript
class LlmStreamBuffer {
    constructor() {
        this.buffer = '';
        this.displayedLength = 0;
        this.animationFrameId = null;
    }
    
    addChunk(chunk) {
        this.buffer += chunk;
        this.startAnimation();
    }
    
    startAnimation() {
        if (this.animationFrameId) return;
        
        const animate = () => {
            if (this.displayedLength < this.buffer.length) {
                // 文字を1つずつ表示
                const char = this.buffer[this.displayedLength];
                updateChatUI(char);
                this.displayedLength++;
                
                this.animationFrameId = requestAnimationFrame(animate);
            } else {
                this.animationFrameId = null;
            }
        };
        
        this.animationFrameId = requestAnimationFrame(animate);
    }
}

const buffer = new LlmStreamBuffer();

ws.onmessage = (event) => {
    const data = JSON.parse(event.data);
    
    if (!data.done) {
        buffer.addChunk(data.content);
    }
};
```

---

## トラブルシューティング

### 問題: ストリーミングが遅い

**原因**: チャンク間遅延が大きすぎる

**解決策**:

```rust
LlmStreamConfig {
    delay_ms: 25,  // 50ms → 25ms に減らす
    // ...
}
```

### 問題: ネットワークエラーが多い

**原因**: チャンクサイズが小さすぎる

**解決策**:

```rust
LlmStreamConfig {
    chunk_size: 50,  // 20 → 50 に増やす
    // ...
}
```

### 問題: メモリ使用量が高い

**原因**: 多数のストリームが同時実行中

**解決策**:

```rust
// ストリーム数を制限
use tokio::sync::Semaphore;

let stream_semaphore = Arc::new(Semaphore::new(10));  // 最大10ストリーム

// ストリーム開始前
let permit = stream_semaphore.acquire().await.unwrap();
let stream_id = streamer.start_stream(response).await.unwrap();
// ...
drop(permit);  // ストリーム終了時に解放
```

---

## サンプルコード

完全な実装例: [websocket_llm_chat.rs](../examples/websocket_llm_chat.rs)

---

## 関連ドキュメント

- [WebSocket統合ガイド](websocket-guide.md)
- [パフォーマンスチューニング](websocket-performance.md)
- [統合テスト](../tests/websocket_integration_tests.rs)

---

## ライセンス

MIT または Apache-2.0
