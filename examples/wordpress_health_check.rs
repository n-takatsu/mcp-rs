use mcp_rs::{config::WordPressConfig, handlers::wordpress::WordPressHandler};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    println!("🔍 WordPress Health Check Example\n");

    // Create WordPress configuration from environment variables or defaults
    let wp_config = WordPressConfig {
        url: "https://demo.wp-api.org/wp-json".to_string(),
        username: "demo".to_string(),
        password: "demo".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(30),
        rate_limit: None,
        encrypted_credentials: None, // 平文認証情報を使用
    };

    info!("Configuration created");
    info!("WordPress URL: {}", wp_config.url);
    info!("Username: {}", wp_config.username);
    info!("Timeout: {}秒", wp_config.timeout_seconds.unwrap_or(30));

    println!("=== WordPress Environment Health Check ===");
    println!("URL: {}", wp_config.url);
    println!("Username: {}", wp_config.username);
    println!("Timeout: {}s", wp_config.timeout_seconds.unwrap_or(30));
    println!();

    // Create WordPress handler
    let handler = WordPressHandler::new(wp_config);

    println!("=== WordPress Environment Health Check ===\n");

    // Perform health check
    let health_result = handler.health_check().await;

    // Display results
    display_health_results(&health_result);

    // Provide recommendations
    provide_recommendations(&health_result);

    Ok(())
}

fn display_health_results(health: &mcp_rs::handlers::wordpress::WordPressHealthCheck) {
    let status_emoji = if health.error_details.is_empty() {
        "✅"
    } else {
        "⚠️"
    };
    let status_text = if health.error_details.is_empty() {
        "HEALTHY"
    } else {
        "ISSUES DETECTED"
    };

    println!("{} Overall Status: {}\n", status_emoji, status_text);

    if let Some(site_info) = &health.site_info {
        println!("🌐 Site Information:");
        println!("   Name: {}", site_info.name);
        println!("   URL: {}", site_info.url);
        println!("   Description: {}", site_info.description);
        if let Some(email) = &site_info.admin_email {
            println!("   Admin Email: {}", email);
        }
        if let Some(tz) = &site_info.timezone_string {
            println!("   Timezone: {}", tz);
        }
        println!();
    }

    println!("📊 Detailed Health Status:");
    println!(
        "   Site Accessible:        {}",
        if health.site_accessible {
            "✅ YES"
        } else {
            "❌ NO"
        }
    );
    println!(
        "   REST API Available:     {}",
        if health.rest_api_available {
            "✅ YES"
        } else {
            "❌ NO"
        }
    );
    println!(
        "   Authentication Valid:   {}",
        if health.authentication_valid {
            "✅ YES"
        } else {
            "❌ NO"
        }
    );
    println!(
        "   Permissions Adequate:   {}",
        if health.permissions_adequate {
            "✅ YES"
        } else {
            "❌ NO"
        }
    );
    println!(
        "   Media Upload Possible:  {}",
        if health.media_upload_possible {
            "✅ YES"
        } else {
            "❌ NO"
        }
    );

    if !health.error_details.is_empty() {
        println!("\n🚨 Issues Detected:");
        for (i, error) in health.error_details.iter().enumerate() {
            println!("   {}. {}", i + 1, error);
        }
    }

    println!();
}

fn provide_recommendations(health: &mcp_rs::handlers::wordpress::WordPressHealthCheck) {
    if health.error_details.is_empty() {
        println!(
            "🎉 Congratulations! Your WordPress environment is fully configured and ready to use."
        );
        println!("\n💡 You can now use the following MCP tools:");
        println!("   • create_post - Create new blog posts");
        println!("   • upload_media - Upload images and files");
        println!("   • create_post_with_featured_image - Create posts with featured images");
        println!("   • set_featured_image - Add featured images to existing posts");
        println!("   • get_posts - Retrieve existing posts");
        println!("   • get_comments - View post comments");
        return;
    }

    println!("💡 Recommendations to fix issues:\n");

    if !health.site_accessible {
        println!("🔧 Site Accessibility Issues:");
        println!("   • Verify the WordPress URL in your configuration");
        println!("   • Check if the site is online and accessible");
        println!("   • Test the URL in a web browser");
        println!("   • Check network connectivity and firewall settings");
        println!();
    }

    if !health.rest_api_available {
        println!("🔧 REST API Issues:");
        println!("   • Ensure WordPress REST API is enabled");
        println!("   • Check permalink structure (not 'Plain')");
        println!("   • Verify .htaccess file configuration");
        println!("   • Check for conflicting plugins that might disable REST API");
        println!();
    }

    if !health.authentication_valid {
        println!("🔧 Authentication Issues:");
        println!("   • Verify application password is correct");
        println!("   • Check username matches exactly (case-sensitive)");
        println!("   • Regenerate application password if needed");
        println!("   • Ensure user account is active and not suspended");
        println!();
    }

    if !health.permissions_adequate {
        println!("🔧 Permission Issues:");
        println!("   • User needs Editor or Administrator role");
        println!("   • Check user has 'publish_posts' capability");
        println!("   • Verify user can 'upload_files'");
        println!("   • Contact site administrator to adjust permissions");
        println!();
    }

    if !health.media_upload_possible {
        println!("🔧 Media Upload Issues:");
        println!("   • Check file upload permissions on server");
        println!("   • Verify upload_max_filesize is adequate");
        println!("   • Check WordPress media upload settings");
        println!("   • Ensure uploads directory is writable");
        println!();
    }

    println!(
        "📖 For more help, check the documentation or run this check again after making changes."
    );
}
