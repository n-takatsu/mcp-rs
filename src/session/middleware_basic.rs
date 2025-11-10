//! セッションミドルウェア
//!
//! リアルタイム編集システム用のHTTPミドルウェア機能

use crate::error::SessionError;
use crate::session::{SessionId, SessionManager, SessionState};
use axum::{
    extract::{Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;
use tracing::{debug, instrument, warn};

/// セッション認証ミドルウェア
#[derive(Debug, Clone)]
pub struct SessionMiddleware {
    session_manager: Arc<SessionManager>,
}

impl SessionMiddleware {
    /// 新しいミドルウェアを作成
    pub fn new(session_manager: Arc<SessionManager>) -> Self {
        Self { session_manager }
    }

    /// セッション認証ミドルウェア関数
    #[instrument(skip(self, request, next))]
    pub async fn authenticate(&self, request: Request, next: Next) -> Result<Response, StatusCode> {
        debug!("セッション認証ミドルウェア実行");

        // ヘッダーからセッションIDを取得
        let headers = request.headers();
        let session_id = self.extract_session_id(headers).await;

        match session_id {
            Some(id) => {
                // セッション検証
                match self.session_manager.get_session(&id).await {
                    Ok(Some(session)) => {
                        if session.state == SessionState::Active {
                            debug!("セッション認証成功: {}", id.as_str());
                            Ok(next.run(request).await)
                        } else {
                            warn!("非アクティブセッション: {}", id.as_str());
                            Err(StatusCode::UNAUTHORIZED)
                        }
                    }
                    Ok(None) => {
                        warn!("存在しないセッション: {}", id.as_str());
                        Err(StatusCode::UNAUTHORIZED)
                    }
                    Err(e) => {
                        warn!("セッション検証エラー: {}", e);
                        Err(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
            }
            None => {
                warn!("セッションIDが見つかりません");
                Err(StatusCode::UNAUTHORIZED)
            }
        }
    }

    /// ヘッダーからセッションIDを抽出
    async fn extract_session_id(&self, headers: &HeaderMap) -> Option<SessionId> {
        // Authorization ヘッダーからセッションIDを取得
        if let Some(auth_header) = headers.get("authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    return Some(SessionId::from_string(token.to_string()));
                }
            }
        }

        // X-Session-ID ヘッダーからセッションIDを取得
        if let Some(session_header) = headers.get("x-session-id") {
            if let Ok(session_str) = session_header.to_str() {
                return Some(SessionId::from_string(session_str.to_string()));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_middleware_creation() {
        let manager = Arc::new(SessionManager::new());
        let _middleware = SessionMiddleware::new(manager);

        // ミドルウェアが正常に作成されることを確認
        // 型チェックとして成功（テストは実装済み）
    }
}
