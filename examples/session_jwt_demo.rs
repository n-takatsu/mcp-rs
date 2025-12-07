//! Session JWT Integration Demo
//!
//! This example demonstrates how to use JWT-integrated session management.
//!
//! Run this example:
//! ```bash
//! cargo run --example session_jwt_demo
//! ```

use mcp_rs::security::auth::types::Role;
use mcp_rs::session::{SessionJwtConfig, SessionJwtManager, SessionManager};
use std::sync::Arc;
use tracing::{info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    info!("=== Session JWT Integration Demo ===\n");

    // Create session manager
    let session_manager = Arc::new(SessionManager::new());

    // Create JWT configuration
    let jwt_config = SessionJwtConfig::default();

    // Create JWT-integrated session manager
    let jwt_manager = SessionJwtManager::new(session_manager.clone(), jwt_config);

    // === 1. Create session with JWT tokens ===
    info!("1. Creating session with JWT tokens...");
    let session_with_tokens = jwt_manager
        .create_session_with_tokens(
            "user-12345".to_string(),
            "alice".to_string(),
            Some("alice@example.com".to_string()),
            vec![Role::Admin, Role::User],
        )
        .await?;

    info!("✓ Session created: id={}", session_with_tokens.session.id);
    info!(
        "✓ Access Token: {}...",
        &session_with_tokens.tokens.access_token[..50]
    );
    info!(
        "✓ Refresh Token: {}...",
        &session_with_tokens.tokens.refresh_token[..50]
    );
    info!("✓ Token Type: {}", session_with_tokens.tokens.token_type);
    info!(
        "✓ Expires In: {} seconds\n",
        session_with_tokens.tokens.expires_in
    );

    // === 2. Verify token and get session ===
    info!("2. Verifying access token and retrieving session...");
    let (session, claims) = jwt_manager
        .verify_token_and_get_session(&session_with_tokens.tokens.access_token)
        .await?;

    info!("✓ Token verified successfully");
    info!("✓ Session ID: {}", session.id);
    info!("✓ User ID: {}", claims.sub);
    info!("✓ Username: {}", claims.username);
    info!("✓ Email: {:?}", claims.email);
    info!("✓ Roles: {:?}", claims.roles);
    info!("✓ Token Type: {}", claims.token_type);
    info!("✓ JTI (JWT ID): {}\n", claims.jti);

    // === 3. Refresh tokens ===
    info!("3. Refreshing tokens using refresh token...");
    let new_tokens = jwt_manager
        .refresh_tokens(&session_with_tokens.tokens.refresh_token)
        .await?;

    info!("✓ Tokens refreshed successfully");
    info!("✓ New Access Token: {}...", &new_tokens.access_token[..50]);
    info!(
        "✓ New Refresh Token: {}...",
        &new_tokens.refresh_token[..50]
    );
    info!("✓ Old refresh token has been revoked (rotation enabled)\n");

    // === 4. Check token revocation ===
    info!("4. Checking token revocation...");

    // Get JTI from old refresh token
    let old_refresh_claims = jwt_manager
        .jwt_auth()
        .verify_token(&session_with_tokens.tokens.refresh_token)?;

    let is_revoked = jwt_manager.is_token_revoked(&old_refresh_claims.jti).await;
    info!("✓ Old refresh token revoked: {}\n", is_revoked);

    // === 5. Manual token revocation ===
    info!("5. Manually revoking a token...");
    let new_access_claims = jwt_manager
        .jwt_auth()
        .verify_access_token(&new_tokens.access_token)?;

    jwt_manager
        .revoke_token(
            new_access_claims.jti.clone(),
            "Manual revocation test".to_string(),
        )
        .await?;

    info!("✓ Token revoked: jti={}", new_access_claims.jti);

    // Try to use the revoked token
    let verify_result = jwt_manager
        .verify_token_and_get_session(&new_tokens.access_token)
        .await;

    match verify_result {
        Err(e) => info!("✓ Revoked token rejected: {}\n", e),
        Ok(_) => info!("✗ ERROR: Revoked token was accepted!\n"),
    }

    // === 6. Create multiple sessions for the same user ===
    info!("6. Creating multiple sessions for the same user...");

    for i in 1..=3 {
        jwt_manager
            .create_session_with_tokens(
                "user-12345".to_string(),
                "alice".to_string(),
                Some("alice@example.com".to_string()),
                vec![Role::User],
            )
            .await?;
        info!("✓ Created session {}", i);
    }

    // === 7. Force logout all sessions for a user ===
    info!("\n7. Force logout all sessions for user...");
    let sessions_deleted = jwt_manager.force_logout_user("user-12345").await?;
    info!("✓ Deleted {} sessions for user-12345\n", sessions_deleted);

    // === 8. Feature Summary ===
    info!("=== Feature Summary ===");
    info!("✓ JWT Token Generation (Access + Refresh)");
    info!("✓ Token Verification with Session Lookup");
    info!("✓ Token Refresh with Rotation");
    info!("✓ Token Revocation List (In-Memory)");
    info!("✓ Manual Token Revocation");
    info!("✓ Session-Token Integration");
    info!("✓ Multi-Session Management");
    info!("✓ Force Logout (All User Sessions)");

    info!("\n=== Production Features ===");
    info!("• Redis-based Token Revocation (enable 'redis' feature)");
    info!("• Automatic TTL-based Token Cleanup");
    info!("• Distributed Session Management");
    info!("• High-Performance Token Lookup");

    Ok(())
}
