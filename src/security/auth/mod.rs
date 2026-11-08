// Authentication Module
//
// HTTPモードのAPIキー認証（`api_key`、#342）と、その型（`types`）。
//
// 以前あったJWT・OAuth2・セッション認証・認証ミドルウェア・ユーザーの
// 保存先・REST APIは、本体から使われていなかったため削除した（#421）。
// HTTPモードのOAuth 2.1による認可は#420で扱う。

pub mod api_key;
pub mod types;

pub use api_key::{ApiKey, ApiKeyConfig, ApiKeyManager, ApiKeyPermission};
pub use types::{
    AuthError, AuthMethod, AuthProvider, AuthResult, AuthUser, Credentials, PasswordHasher,
    Permission, Role,
};
