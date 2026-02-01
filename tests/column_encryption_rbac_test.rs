//! Column Encryption RBAC Integration Tests

#![cfg(feature = "database")]

#[cfg(test)]
mod tests {
    use mcp_rs::handlers::database::column_encryption_rbac::{
        ColumnEncryptionRbac, EncryptionAuditLog, EncryptionOperation,
    };
    use mcp_rs::security::auth::types::{AuthUser, Role};
    use sqlx::PgPool;
    use std::sync::Arc;

    async fn setup_test_db() -> PgPool {
        // This would need a test database connection in a real test
        // For now, this is a placeholder
        let database_url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| "".to_string());
        if database_url.is_empty() {
            // Skip tests if no test database is configured
            panic!("TEST_DATABASE_URL not configured, skipping tests");
        }
        PgPool::connect(&database_url).await.unwrap()
    }

    #[tokio::test]
    #[ignore] // Requires test database
    async fn test_admin_has_all_permissions() {
        let pool = setup_test_db().await;
        let rbac = ColumnEncryptionRbac::new(pool);

        let mut admin_user = AuthUser::new("admin1".to_string(), "Admin User".to_string());
        admin_user.roles.insert(Role::Admin);

        // Admin should have all permissions
        let can_encrypt = rbac
            .check_permission(&admin_user, "users", "email", EncryptionOperation::Encrypt)
            .await
            .unwrap();
        assert!(can_encrypt);

        let can_decrypt = rbac
            .check_permission(&admin_user, "users", "email", EncryptionOperation::Decrypt)
            .await
            .unwrap();
        assert!(can_decrypt);

        let can_rotate = rbac
            .check_permission(
                &admin_user,
                "users",
                "email",
                EncryptionOperation::RotateKey,
            )
            .await
            .unwrap();
        assert!(can_rotate);
    }

    #[tokio::test]
    #[ignore] // Requires test database
    async fn test_user_without_permission() {
        let pool = setup_test_db().await;
        let rbac = ColumnEncryptionRbac::new(pool);

        let mut regular_user = AuthUser::new("user1".to_string(), "Regular User".to_string());
        regular_user.roles.insert(Role::User);

        // Regular user without granted permission should not have access
        let can_decrypt = rbac
            .check_permission(&regular_user, "users", "ssn", EncryptionOperation::Decrypt)
            .await
            .unwrap();
        assert!(!can_decrypt);
    }

    #[tokio::test]
    #[ignore] // Requires test database
    async fn test_grant_and_check_permission() {
        let pool = setup_test_db().await;
        let rbac = Arc::new(ColumnEncryptionRbac::new(pool));

        // Grant permission
        rbac.grant_permission("User", "users", "email", false, true, false)
            .await
            .unwrap();

        let mut user = AuthUser::new("user1".to_string(), "User".to_string());
        user.roles.insert(Role::User);

        // Should have decrypt permission now
        let can_decrypt = rbac
            .check_permission(&user, "users", "email", EncryptionOperation::Decrypt)
            .await
            .unwrap();
        assert!(can_decrypt);

        // But not encrypt permission
        let can_encrypt = rbac
            .check_permission(&user, "users", "email", EncryptionOperation::Encrypt)
            .await
            .unwrap();
        assert!(!can_encrypt);
    }

    #[tokio::test]
    #[ignore] // Requires test database
    async fn test_wildcard_permissions() {
        let pool = setup_test_db().await;
        let rbac = ColumnEncryptionRbac::new(pool);

        // Grant wildcard permission
        rbac.grant_permission("User", "users", "*", true, true, false)
            .await
            .unwrap();

        let mut user = AuthUser::new("user1".to_string(), "User".to_string());
        user.roles.insert(Role::User);

        // Should work for any column in users table
        let can_decrypt_email = rbac
            .check_permission(&user, "users", "email", EncryptionOperation::Decrypt)
            .await
            .unwrap();
        assert!(can_decrypt_email);

        let can_decrypt_phone = rbac
            .check_permission(&user, "users", "phone", EncryptionOperation::Decrypt)
            .await
            .unwrap();
        assert!(can_decrypt_phone);
    }

    #[tokio::test]
    #[ignore] // Requires test database
    async fn test_audit_log() {
        let pool = setup_test_db().await;
        let rbac = ColumnEncryptionRbac::new(pool.clone());

        let audit_log = EncryptionAuditLog {
            user_id: "user123".to_string(),
            operation: EncryptionOperation::Decrypt,
            table_name: "users".to_string(),
            column_name: "email".to_string(),
            success: true,
            error_message: None,
            request_ip: Some("192.168.1.100".to_string()),
            user_agent: Some("TestAgent/1.0".to_string()),
        };

        rbac.audit_log(&audit_log).await.unwrap();

        // Verify log was recorded
        let logs = rbac
            .get_audit_logs(Some("user123"), None, 10)
            .await
            .unwrap();
        assert!(!logs.is_empty());
        assert_eq!(logs[0].user_id, "user123");
        assert_eq!(logs[0].table_name, "users");
        assert_eq!(logs[0].column_name, "email");
        assert!(logs[0].success);
    }

    #[tokio::test]
    #[ignore] // Requires test database
    async fn test_revoke_permission() {
        let pool = setup_test_db().await;
        let rbac = ColumnEncryptionRbac::new(pool);

        // Grant then revoke
        rbac.grant_permission("User", "users", "email", true, true, false)
            .await
            .unwrap();

        rbac.revoke_permission("User", "users", "email")
            .await
            .unwrap();

        let mut user = AuthUser::new("user1".to_string(), "User".to_string());
        user.roles.insert(Role::User);

        // Should no longer have permission
        let can_decrypt = rbac
            .check_permission(&user, "users", "email", EncryptionOperation::Decrypt)
            .await
            .unwrap();
        assert!(!can_decrypt);
    }

    #[test]
    fn test_encryption_operation_serialization() {
        let op = EncryptionOperation::Decrypt;
        let serialized = serde_json::to_string(&op).unwrap();
        assert_eq!(serialized, "\"decrypt\"");

        let deserialized: EncryptionOperation = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized, EncryptionOperation::Decrypt);
    }
}
