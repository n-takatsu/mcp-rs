//! セッションベースアクセス制御
//!
//! ロールベースアクセス制御（RBAC）と同時セッション制限を提供

use crate::error::SessionError;
use crate::session::{SessionId, SessionManager};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

/// セッションアクセス制御マネージャー
#[derive(Debug, Clone)]
pub struct SessionAccessControl {
    /// セッションマネージャー
    session_manager: Arc<SessionManager>,
    /// ユーザーロール情報
    user_roles: Arc<RwLock<HashMap<String, HashSet<Role>>>>,
    /// ユーザー権限情報
    user_permissions: Arc<RwLock<HashMap<String, HashSet<Permission>>>>,
    /// アクティブセッション数（ユーザーごと）
    active_sessions: Arc<RwLock<HashMap<String, HashSet<SessionId>>>>,
    /// 設定
    config: AccessControlConfig,
}

/// アクセス制御設定
#[derive(Debug, Clone)]
pub struct AccessControlConfig {
    /// 最大同時セッション数
    pub max_concurrent_sessions: usize,
    /// ロールベース制御有効化
    pub enable_rbac: bool,
}

/// ユーザーロール
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// 管理者
    Admin,
    /// 編集者
    Editor,
    /// 閲覧者
    Viewer,
    /// カスタムロール
    Custom(String),
}

/// 権限
#[derive(Debug, Clone, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub enum Permission {
    /// 読み取り
    Read,
    /// 書き込み
    Write,
    /// 削除
    Delete,
    /// 管理
    Manage,
    /// カスタム権限
    Custom(String),
}

impl SessionAccessControl {
    /// 新しいアクセス制御マネージャーを作成
    pub fn new(session_manager: Arc<SessionManager>, config: AccessControlConfig) -> Self {
        Self {
            session_manager,
            user_roles: Arc::new(RwLock::new(HashMap::new())),
            user_permissions: Arc::new(RwLock::new(HashMap::new())),
            active_sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    /// ユーザーにロールを割り当て
    #[instrument(skip(self))]
    pub async fn assign_role(&self, user_id: &str, role: Role) -> Result<(), SessionError> {
        info!("ロール割り当て: user_id={}, role={:?}", user_id, role);

        let mut roles = self.user_roles.write().await;
        roles.entry(user_id.to_string()).or_default().insert(role);

        Ok(())
    }

    /// ユーザーのロールを削除
    #[instrument(skip(self))]
    pub async fn remove_role(&self, user_id: &str, role: &Role) -> Result<bool, SessionError> {
        let mut roles = self.user_roles.write().await;

        if let Some(user_roles) = roles.get_mut(user_id) {
            Ok(user_roles.remove(role))
        } else {
            Ok(false)
        }
    }

    /// ユーザーが特定のロールを持っているか確認
    #[instrument(skip(self))]
    pub async fn has_role(&self, user_id: &str, role: &Role) -> Result<bool, SessionError> {
        let roles = self.user_roles.read().await;

        Ok(roles
            .get(user_id)
            .map(|user_roles| user_roles.contains(role))
            .unwrap_or(false))
    }

    /// ユーザーに権限を付与
    #[instrument(skip(self))]
    pub async fn grant_permission(
        &self,
        user_id: &str,
        permission: Permission,
    ) -> Result<(), SessionError> {
        info!("権限付与: user_id={}, permission={:?}", user_id, permission);

        let mut permissions = self.user_permissions.write().await;
        permissions
            .entry(user_id.to_string())
            .or_default()
            .insert(permission);

        Ok(())
    }

    /// ユーザーの権限を確認
    #[instrument(skip(self))]
    pub async fn check_permission(
        &self,
        user_id: &str,
        permission: &Permission,
    ) -> Result<bool, SessionError> {
        let permissions = self.user_permissions.read().await;
        let roles = self.user_roles.read().await;

        // 直接権限チェック
        if let Some(user_perms) = permissions.get(user_id) {
            if user_perms.contains(permission) {
                return Ok(true);
            }
        }

        // ロールベース権限チェック
        if self.config.enable_rbac {
            if let Some(user_roles) = roles.get(user_id) {
                for role in user_roles {
                    if self.role_has_permission(role, permission) {
                        return Ok(true);
                    }
                }
            }
        }

        Ok(false)
    }

    /// ロールが特定の権限を持っているか確認
    fn role_has_permission(&self, role: &Role, permission: &Permission) -> bool {
        match role {
            Role::Admin => true, // 管理者は全権限
            Role::Editor => matches!(permission, Permission::Read | Permission::Write),
            Role::Viewer => matches!(permission, Permission::Read),
            Role::Custom(_) => false,
        }
    }

    /// セッション作成時のアクセス制御チェック
    #[instrument(skip(self))]
    pub async fn can_create_session(&self, user_id: &str) -> Result<bool, SessionError> {
        debug!("セッション作成可否チェック: user_id={}", user_id);

        let active_sessions = self.active_sessions.read().await;

        if let Some(sessions) = active_sessions.get(user_id) {
            if sessions.len() >= self.config.max_concurrent_sessions {
                warn!(
                    "同時セッション数制限超過: user_id={}, current={}, max={}",
                    user_id,
                    sessions.len(),
                    self.config.max_concurrent_sessions
                );
                return Ok(false);
            }
        }

        Ok(true)
    }

    /// アクティブセッション追加
    #[instrument(skip(self))]
    pub async fn register_session(
        &self,
        user_id: &str,
        session_id: &SessionId,
    ) -> Result<(), SessionError> {
        info!(
            "セッション登録: user_id={}, session_id={}",
            user_id,
            session_id.as_str()
        );

        let mut active_sessions = self.active_sessions.write().await;
        active_sessions
            .entry(user_id.to_string())
            .or_default()
            .insert(session_id.clone());

        Ok(())
    }

    /// アクティブセッション削除
    #[instrument(skip(self))]
    pub async fn unregister_session(
        &self,
        user_id: &str,
        session_id: &SessionId,
    ) -> Result<bool, SessionError> {
        info!(
            "セッション削除: user_id={}, session_id={}",
            user_id,
            session_id.as_str()
        );

        let mut active_sessions = self.active_sessions.write().await;

        if let Some(sessions) = active_sessions.get_mut(user_id) {
            Ok(sessions.remove(session_id))
        } else {
            Ok(false)
        }
    }

    /// ユーザーのアクティブセッション数を取得
    #[instrument(skip(self))]
    pub async fn get_active_session_count(&self, user_id: &str) -> Result<usize, SessionError> {
        let active_sessions = self.active_sessions.read().await;
        Ok(active_sessions.get(user_id).map(|s| s.len()).unwrap_or(0))
    }

    /// ユーザーの全セッションを強制終了
    #[instrument(skip(self))]
    pub async fn terminate_all_sessions(&self, user_id: &str) -> Result<usize, SessionError> {
        info!("全セッション終了: user_id={}", user_id);

        let mut active_sessions = self.active_sessions.write().await;
        let sessions = active_sessions.remove(user_id).unwrap_or_default();

        let mut count = 0;
        for session_id in sessions {
            if self.session_manager.delete_session(&session_id).await? {
                count += 1;
            }
        }

        Ok(count)
    }
}

impl Default for AccessControlConfig {
    fn default() -> Self {
        Self {
            max_concurrent_sessions: 5,
            enable_rbac: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_role_assignment() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let config = AccessControlConfig::default();
        let ac = SessionAccessControl::new(session_manager, config);

        // ロール割り当て
        ac.assign_role("user1", Role::Editor).await?;

        // ロール確認
        assert!(ac.has_role("user1", &Role::Editor).await?);
        assert!(!ac.has_role("user1", &Role::Admin).await?);

        // ロール削除
        let removed = ac.remove_role("user1", &Role::Editor).await?;
        assert!(removed);
        assert!(!ac.has_role("user1", &Role::Editor).await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_permission_check() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let config = AccessControlConfig::default();
        let ac = SessionAccessControl::new(session_manager, config);

        // 直接権限付与
        ac.grant_permission("user1", Permission::Write).await?;
        assert!(ac.check_permission("user1", &Permission::Write).await?);

        // ロールベース権限
        ac.assign_role("user2", Role::Admin).await?;
        assert!(ac.check_permission("user2", &Permission::Manage).await?);

        ac.assign_role("user3", Role::Viewer).await?;
        assert!(ac.check_permission("user3", &Permission::Read).await?);
        assert!(!ac.check_permission("user3", &Permission::Write).await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_concurrent_session_limit() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let mut config = AccessControlConfig::default();
        config.max_concurrent_sessions = 2;
        let ac = SessionAccessControl::new(session_manager.clone(), config);

        // 1つ目のセッション
        let session1 = session_manager.create_session("user1".to_string()).await?;
        assert!(ac.can_create_session("user1").await?);
        ac.register_session("user1", &session1.id).await?;

        // 2つ目のセッション
        let session2 = session_manager.create_session("user1".to_string()).await?;
        assert!(ac.can_create_session("user1").await?);
        ac.register_session("user1", &session2.id).await?;

        // 3つ目のセッション（制限超過）
        assert!(!ac.can_create_session("user1").await?);

        // セッション削除
        ac.unregister_session("user1", &session1.id).await?;
        assert!(ac.can_create_session("user1").await?);

        Ok(())
    }

    #[tokio::test]
    async fn test_terminate_all_sessions() -> Result<(), SessionError> {
        let session_manager = Arc::new(SessionManager::new());
        let config = AccessControlConfig::default();
        let ac = SessionAccessControl::new(session_manager.clone(), config);

        // 複数セッション作成
        let session1 = session_manager.create_session("user1".to_string()).await?;
        let session2 = session_manager.create_session("user1".to_string()).await?;
        ac.register_session("user1", &session1.id).await?;
        ac.register_session("user1", &session2.id).await?;

        assert_eq!(ac.get_active_session_count("user1").await?, 2);

        // 全セッション終了
        let count = ac.terminate_all_sessions("user1").await?;
        assert_eq!(count, 2);
        assert_eq!(ac.get_active_session_count("user1").await?, 0);

        Ok(())
    }
}
