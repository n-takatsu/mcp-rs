// Authentication Middleware for Axum

use super::provider::AuthenticationProvider;
use super::types::{AuthError, AuthMethod, AuthResult, AuthUser, Permission, Role};
use axum::{
    body::Body,
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use std::sync::Arc;

/// 認証要件
#[derive(Debug, Clone)]
pub enum AuthRequirement {
    /// 認証必須
    Required,
    /// 認証オプション
    Optional,
    /// 特定のロールが必要
    Role(Role),
    /// 特定のパーミッションが必要
    Permission(Permission),
    /// 複数のロールのいずれか
    AnyRole(Vec<Role>),
    /// 複数のパーミッションのいずれか
    AnyPermission(Vec<Permission>),
}

/// 認証ミドルウェア
#[derive(Clone)]
pub struct AuthMiddleware<P: AuthenticationProvider> {
    provider: Arc<P>,
    requirement: AuthRequirement,
}

impl<P: AuthenticationProvider> AuthMiddleware<P> {
    pub fn new(provider: Arc<P>, requirement: AuthRequirement) -> Self {
        Self {
            provider,
            requirement,
        }
    }

    /// トークンを抽出
    fn extract_token(request: &Request) -> Option<(String, AuthMethod)> {
        // Authorizationヘッダーから抽出
        if let Some(auth_header) = request.headers().get(header::AUTHORIZATION) {
            if let Ok(auth_str) = auth_header.to_str() {
                // Bearer トークン
                if let Some(token) = auth_str.strip_prefix("Bearer ") {
                    return Some((token.to_string(), AuthMethod::Bearer));
                }
                // API Key
                if let Some(token) = auth_str.strip_prefix("ApiKey ") {
                    return Some((token.to_string(), AuthMethod::ApiKey));
                }
                // Basic認証（セッショントークン）
                if let Some(token) = auth_str.strip_prefix("Session ") {
                    return Some((token.to_string(), AuthMethod::Session));
                }
            }
        }

        // Cookieから抽出
        if let Some(cookie_header) = request.headers().get(header::COOKIE) {
            if let Ok(cookie_str) = cookie_header.to_str() {
                for cookie in cookie_str.split(';') {
                    let parts: Vec<&str> = cookie.trim().splitn(2, '=').collect();
                    if parts.len() == 2 && parts[0] == "mcp_session" {
                        return Some((parts[1].to_string(), AuthMethod::Session));
                    }
                }
            }
        }

        None
    }

    /// 認証要件を確認
    fn check_requirement(&self, user: &AuthUser) -> AuthResult<()> {
        match &self.requirement {
            AuthRequirement::Required => Ok(()),
            AuthRequirement::Optional => Ok(()),
            AuthRequirement::Role(role) => {
                if user.has_role(role) || user.is_admin() {
                    Ok(())
                } else {
                    Err(AuthError::Forbidden(format!("Role {:?} required", role)))
                }
            }
            AuthRequirement::Permission(permission) => {
                if user.has_permission(permission) || user.is_admin() {
                    Ok(())
                } else {
                    Err(AuthError::Forbidden(format!(
                        "Permission {}:{} required",
                        permission.resource, permission.action
                    )))
                }
            }
            AuthRequirement::AnyRole(roles) => {
                if user.is_admin() || roles.iter().any(|r| user.has_role(r)) {
                    Ok(())
                } else {
                    Err(AuthError::Forbidden("Required role not found".to_string()))
                }
            }
            AuthRequirement::AnyPermission(permissions) => {
                if user.is_admin() || permissions.iter().any(|p| user.has_permission(p)) {
                    Ok(())
                } else {
                    Err(AuthError::Forbidden(
                        "Required permission not found".to_string(),
                    ))
                }
            }
        }
    }

    /// ミドルウェアハンドラー
    pub async fn handle(&self, mut request: Request, next: Next) -> Result<Response, AuthError> {
        // トークンを抽出
        let token_info = Self::extract_token(&request);

        match (&self.requirement, token_info) {
            // 認証オプション & トークンなし
            (AuthRequirement::Optional, None) => Ok(next.run(request).await),

            // 認証必須 & トークンなし
            (
                AuthRequirement::Required
                | AuthRequirement::Role(_)
                | AuthRequirement::Permission(_)
                | AuthRequirement::AnyRole(_)
                | AuthRequirement::AnyPermission(_),
                None,
            ) => Err(AuthError::Unauthorized(
                "Authentication required".to_string(),
            )),

            // トークンあり
            (_, Some((token, method))) => {
                // トークンを検証
                let user = self.provider.verify_token(&token, method).await?;

                // 要件をチェック
                self.check_requirement(&user)?;

                // ユーザー情報をリクエストに追加
                request.extensions_mut().insert(user);

                Ok(next.run(request).await)
            }
        }
    }
}

/// Axumレスポンスへの変換
impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::InvalidCredentials => (StatusCode::UNAUTHORIZED, "Invalid credentials"),
            AuthError::TokenExpired => (StatusCode::UNAUTHORIZED, "Token expired"),
            AuthError::TokenInvalid(_) => (StatusCode::UNAUTHORIZED, "Invalid token"),
            AuthError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "Unauthorized"),
            AuthError::Forbidden(_) => (StatusCode::FORBIDDEN, "Forbidden"),
            AuthError::UserNotFound(_) => (StatusCode::NOT_FOUND, "User not found"),
            AuthError::UserAlreadyExists(_) => (StatusCode::CONFLICT, "User already exists"),
            AuthError::WeakPassword => (StatusCode::BAD_REQUEST, "Password too weak"),
            AuthError::MfaRequired => (StatusCode::UNAUTHORIZED, "MFA required"),
            AuthError::MfaInvalid => (StatusCode::UNAUTHORIZED, "Invalid MFA code"),
            AuthError::SessionNotFound => (StatusCode::NOT_FOUND, "Session not found"),
            AuthError::ProviderError(_) => (StatusCode::BAD_GATEWAY, "Provider error"),
            AuthError::ConfigError(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Configuration error"),
            AuthError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "Internal error"),
        };

        let body = serde_json::json!({
            "error": message,
            "details": self.to_string(),
        });

        (status, axum::Json(body)).into_response()
    }
}

/// リクエストからユーザーを取得するヘルパー
pub trait AuthUserExt {
    fn auth_user(&self) -> Option<&AuthUser>;
    fn require_auth_user(&self) -> Result<&AuthUser, AuthError>;
}

impl AuthUserExt for Request {
    fn auth_user(&self) -> Option<&AuthUser> {
        self.extensions().get::<AuthUser>()
    }

    fn require_auth_user(&self) -> Result<&AuthUser, AuthError> {
        self.auth_user()
            .ok_or_else(|| AuthError::Unauthorized("Authentication required".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::security::auth::repository::memory::InMemoryUserRepository;
    use crate::security::auth::repository::UserRepository;
    use crate::security::auth::{JwtAuth, JwtConfig, MultiAuthProvider};
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware,
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn protected_handler(request: Request<Body>) -> Result<String, AuthError> {
        let user = request.require_auth_user()?;
        Ok(format!("Hello, {}!", user.username))
    }

    #[tokio::test]
    async fn test_auth_middleware_with_valid_token() {
        let jwt_config = JwtConfig::default();
        let repository: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepository::new());
        let provider = Arc::new(MultiAuthProvider::new(
            Some(jwt_config.clone()),
            None,
            None,
            None,
            12,
            repository,
        ));

        // ユーザー登録と認証
        let user = provider
            .register_user(
                "testuser".to_string(),
                "TestPass123!".to_string(),
                "test@example.com".to_string(),
            )
            .await
            .unwrap();

        // トークン生成
        let jwt = JwtAuth::new(jwt_config);
        let token = jwt.generate_access_token(&user).unwrap();

        // ミドルウェア作成
        let middleware = AuthMiddleware::new(Arc::clone(&provider), AuthRequirement::Required);

        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn(move |req, next| {
                let mw = middleware.clone();
                async move { mw.handle(req, next).await.map_err(|e| e.into_response()) }
            }));

        // リクエスト送信
        let request = Request::builder()
            .uri("/protected")
            .header("Authorization", format!("Bearer {}", token))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_auth_middleware_without_token() {
        let repository: Arc<dyn UserRepository> = Arc::new(InMemoryUserRepository::new());
        let provider = Arc::new(MultiAuthProvider::new(
            None, None, None, None, 12, repository,
        ));
        let middleware = AuthMiddleware::new(provider, AuthRequirement::Required);

        let app = Router::new()
            .route("/protected", get(protected_handler))
            .layer(middleware::from_fn(move |req, next| {
                let mw = middleware.clone();
                async move { mw.handle(req, next).await.map_err(|e| e.into_response()) }
            }));

        let request = Request::builder()
            .uri("/protected")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
}
