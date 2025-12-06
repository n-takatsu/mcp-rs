//! 認証API エンドポイント
//!
//! RESTful認証APIの実装
//! - ユーザー登録
//! - ログイン/ログアウト
//! - トークンリフレッシュ
//! - ユーザー情報取得

use crate::security::auth::{
    AuthError, AuthResult, AuthUser, Credentials, JwtAuth, MultiAuthProvider,
    Role, AuthenticationProvider,
};
#[cfg(feature = "redis-backend")]
use crate::security::auth::RedisSessionStore;
use axum::{
    extract::{Json, Request, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;

/// 認証APIの状態
#[derive(Clone)]
pub struct AuthApiState {
    /// 認証プロバイダー
    pub provider: Arc<MultiAuthProvider>,
    /// JWTハンドラー
    pub jwt_auth: Arc<JwtAuth>,
    /// セッションストア（オプション）
    #[cfg(feature = "redis-backend")]
    pub session_store: Option<Arc<RwLock<RedisSessionStore>>>,
}

/// ユーザー登録リクエスト
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
    pub email: String,
}

/// ユーザー登録レスポンス
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub message: String,
}

/// ログインリクエスト
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    #[serde(default)]
    pub remember_me: bool,
}

/// ログインレスポンス
#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserInfo,
}

/// トークンリフレッシュリクエスト
#[derive(Debug, Deserialize)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

/// トークンリフレッシュレスポンス
#[derive(Debug, Serialize)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: i64,
}

/// ユーザー情報（公開用）
#[derive(Debug, Serialize)]
pub struct UserInfo {
    pub id: String,
    pub username: String,
    pub email: Option<String>,
    pub roles: Vec<String>,
}

impl From<AuthUser> for UserInfo {
    fn from(user: AuthUser) -> Self {
        Self {
            id: user.id,
            username: user.username,
            email: user.email,
            roles: user.roles.iter().map(|r| format!("{:?}", r)).collect(),
        }
    }
}

/// エラーレスポンス
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
}

/// 成功レスポンス
#[derive(Debug, Serialize)]
pub struct SuccessResponse {
    pub message: String,
}

/// 認証APIルーターを作成
pub fn create_auth_router(state: AuthApiState) -> Router {
    Router::new()
        .route("/register", post(register_user))
        .route("/login", post(login_user))
        .route("/refresh", post(refresh_token))
        .route("/logout", post(logout_user))
        .route("/me", get(get_current_user))
        .with_state(state)
}

/// ユーザー登録エンドポイント
///
/// POST /auth/register
async fn register_user(
    State(state): State<AuthApiState>,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AuthError> {
    // パスワード検証は provider.register_user 内で実施される
    let user = state
        .provider
        .register_user(req.username.clone(), req.password, req.email.clone())
        .await?;

    let response = RegisterResponse {
        user_id: user.id.clone(),
        username: user.username,
        email: user.email.clone().unwrap_or_default(),
        message: "User registered successfully".to_string(),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// ログインエンドポイント
///
/// POST /auth/login
async fn login_user(
    State(state): State<AuthApiState>,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, AuthError> {
    // 認証
    let credentials = Credentials::Password {
        username: req.email.clone(),
        password: req.password,
    };

    let user = state.provider.authenticate(credentials).await?;

    // JWTトークン生成
    let token_pair = state.jwt_auth.generate_token_pair(&user)?;

    // セッション作成（Redis有効時）
    #[cfg(feature = "redis-backend")]
    if let Some(session_store) = &state.session_store {
        let session_id = uuid::Uuid::new_v4().to_string();
        let ttl = if req.remember_me { 86400 * 30 } else { 3600 }; // 30日 or 1時間

        session_store
            .write()
            .await
            .create_session(&session_id, &user, Some(ttl))
            .await?;
    }

    let response = LoginResponse {
        access_token: token_pair.access_token,
        refresh_token: token_pair.refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 3600, // 1時間
        user: user.into(),
    };

    Ok((StatusCode::OK, Json(response)))
}

/// トークンリフレッシュエンドポイント
///
/// POST /auth/refresh
async fn refresh_token(
    State(state): State<AuthApiState>,
    Json(req): Json<RefreshRequest>,
) -> Result<impl IntoResponse, AuthError> {
    // リフレッシュトークンを検証
    let claims = state
        .jwt_auth
        .verify_refresh_token(&req.refresh_token)?;

    // ユーザー情報を取得
    let user = state
        .provider
        .get_user_by_id(&claims.sub)
        .await?;

    // 新しいトークンペアを生成
    let token_pair = state.jwt_auth.generate_token_pair(&user)?;

    let response = RefreshResponse {
        access_token: token_pair.access_token,
        refresh_token: token_pair.refresh_token,
        token_type: "Bearer".to_string(),
        expires_in: 3600,
    };

    Ok((StatusCode::OK, Json(response)))
}

/// ログアウトエンドポイント
///
/// POST /auth/logout
async fn logout_user(
    State(_state): State<AuthApiState>,
) -> Result<impl IntoResponse, AuthError> {
    // TODO: トークンのブラックリスト化（将来実装）
    // TODO: セッション削除（session_id取得方法を要検討）

    let response = SuccessResponse {
        message: "Logged out successfully".to_string(),
    };

    Ok((StatusCode::OK, Json(response)))
}

/// 現在のユーザー情報取得エンドポイント
///
/// GET /auth/me
/// Authorization: Bearer <token>
/// 
/// Note: 認証ミドルウェア経由でのみアクセス可能
async fn get_current_user(
    State(_state): State<AuthApiState>,
    request: Request,
) -> Response {
    // リクエストのextensionsからユーザーを取得
    if let Some(user) = request.extensions().get::<AuthUser>() {
        let user_info = UserInfo::from(user.clone());
        (StatusCode::OK, Json(user_info)).into_response()
    } else {
        let error = ErrorResponse {
            error: "Unauthorized".to_string(),
            message: "Authentication required. Use AuthMiddleware.".to_string(),
        };
        (StatusCode::UNAUTHORIZED, Json(error)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::{
        InMemoryUserRepository, JwtConfig, UserRepository,
    };
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    async fn create_test_state() -> AuthApiState {
        let repository: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepository::new());
        let jwt_config = JwtConfig::default();
        let jwt_auth = Arc::new(JwtAuth::new(jwt_config.clone()));

        let provider = Arc::new(MultiAuthProvider::new(
            Some(jwt_config),
            None,
            None,
            None,
            12,
            repository,
        ));

        AuthApiState {
            provider,
            jwt_auth,
            #[cfg(feature = "redis-backend")]
            session_store: None,
        }
    }

    #[tokio::test]
    async fn test_register_user_endpoint() {
        let state = create_test_state().await;
        let app = create_auth_router(state);

        let request_body = serde_json::json!({
            "username": "testuser",
            "password": "SecurePass123!",
            "email": "test@example.com"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_login_user_endpoint() {
        let state = create_test_state().await;

        // ユーザー登録
        state
            .provider
            .register_user(
                "testuser".to_string(),
                "SecurePass123!".to_string(),
                "test@example.com".to_string(),
            )
            .await
            .unwrap();

        let app = create_auth_router(state);

        let request_body = serde_json::json!({
            "email": "test@example.com",
            "password": "SecurePass123!",
            "remember_me": false
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_weak_password_rejection() {
        let state = create_test_state().await;
        let app = create_auth_router(state);

        let request_body = serde_json::json!({
            "username": "testuser",
            "password": "weak",
            "email": "test@example.com"
        });

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/register")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&request_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }
}
