use mcp_rs::config::WordPressConfig;
use mcp_rs::handlers::wordpress::{WordPressHandler, SettingsUpdateParams};
use tracing::{error, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("🏠 WordPress Site Settings Management Test\n");

    // Direct WordPress configuration for testing
    let wp_config = WordPressConfig {
        url: "http://localhost:8080".to_string(),
        username: "admin".to_string(),
        password: "password".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(30),
    };

    // Create WordPress handler
    let handler = WordPressHandler::new(wp_config);

    // Test 1: Get current settings
    println!("🔍 Getting current WordPress settings...");
    match handler.get_settings().await {
        Ok(settings) => {
            println!("✅ Current settings retrieved:");
            println!("   - Site Title: {}", settings.title.unwrap_or("N/A".to_string()));
            println!("   - Description: {}", settings.description.unwrap_or("N/A".to_string()));
            println!("   - Show on Front: {}", settings.show_on_front.unwrap_or("posts".to_string()));
            println!("   - Posts per Page: {}", settings.posts_per_page.unwrap_or(10));
            println!("   - Language: {}", settings.language.unwrap_or("en_US".to_string()));
            println!("   - Timezone: {}", settings.timezone.unwrap_or("UTC".to_string()));
            if let Some(page_id) = settings.page_on_front {
                println!("   - Front Page ID: {}", page_id);
            }
            if let Some(page_id) = settings.page_for_posts {
                println!("   - Posts Page ID: {}", page_id);
            }
        },
        Err(e) => {
            error!("❌ Failed to get settings: {}", e);
            println!("Note: This requires WordPress admin permissions and proper configuration");
            return Ok(()); // Continue with other tests
        }
    }

    println!();

    // Test 2: Update site title and description
    println!("✏️ Updating site title and description...");
    let update_params = SettingsUpdateParams {
        title: Some("MCP-RS Test Site".to_string()),
        description: Some("A WordPress site managed by MCP-RS".to_string()),
        posts_per_page: Some(5),
        ..Default::default()
    };

    match handler.update_settings(update_params).await {
        Ok(settings) => {
            println!("✅ Settings updated successfully:");
            println!("   - New Title: {}", settings.title.unwrap_or("N/A".to_string()));
            println!("   - New Description: {}", settings.description.unwrap_or("N/A".to_string()));
            println!("   - Posts per Page: {}", settings.posts_per_page.unwrap_or(10));
        },
        Err(e) => {
            error!("❌ Failed to update settings: {}", e);
        }
    }

    println!();

    // Test 3: Create a page for front page demo
    println!("📄 Creating a sample page for front page demo...");
    match handler.create_advanced_post(mcp_rs::handlers::wordpress::PostCreateParams {
        title: "Welcome to Our Site".to_string(),
        content: "<h1>Welcome!</h1><p>This is our beautiful homepage created via MCP-RS.</p><p>This page demonstrates the ability to set static pages as the front page of a WordPress site.</p>".to_string(),
        post_type: "page".to_string(),
        status: "publish".to_string(),
        ..Default::default()
    }).await {
        Ok(page) => {
            println!("✅ Sample page created:");
            println!("   - Page ID: {}", page.id.unwrap_or(0));
            println!("   - Title: {}", page.title.rendered);
            println!("   - Type: {}", page.post_type.unwrap_or("page".to_string()));

            let page_id = page.id.unwrap_or(0);
            
            // Test 4: Set this page as front page
            if page_id > 0 {
                println!();
                println!("🏠 Setting page as front page...");
                match handler.set_front_page(page_id).await {
                    Ok(settings) => {
                        println!("✅ Front page set successfully:");
                        println!("   - Show on Front: {}", settings.show_on_front.unwrap_or("page".to_string()));
                        println!("   - Front Page ID: {}", settings.page_on_front.unwrap_or(0));
                    },
                    Err(e) => {
                        error!("❌ Failed to set front page: {}", e);
                    }
                }

                println!();
                println!("🔄 Reverting to posts on front page...");
                match handler.set_front_page_to_posts(None).await {
                    Ok(_) => {
                        println!("✅ Front page reverted to latest posts");
                    },
                    Err(e) => {
                        error!("❌ Failed to revert front page: {}", e);
                    }
                }
            }
        },
        Err(e) => {
            error!("❌ Failed to create sample page: {}", e);
        }
    }

    println!();

    // Test 5: Test timezone and language settings
    println!("🌍 Testing timezone and language settings...");
    let timezone_params = SettingsUpdateParams {
        timezone: Some("Asia/Tokyo".to_string()),
        language: Some("ja".to_string()),
        ..Default::default()
    };

    match handler.update_settings(timezone_params).await {
        Ok(settings) => {
            println!("✅ Timezone and language updated:");
            println!("   - Timezone: {}", settings.timezone.unwrap_or("UTC".to_string()));
            println!("   - Language: {}", settings.language.unwrap_or("en_US".to_string()));
        },
        Err(e) => {
            error!("❌ Failed to update timezone/language: {}", e);
        }
    }

    println!();

    // Test 6: Get final settings
    println!("📊 Final settings check...");
    match handler.get_settings().await {
        Ok(settings) => {
            println!("✅ Final settings:");
            println!("   - Title: {}", settings.title.unwrap_or("N/A".to_string()));
            println!("   - Description: {}", settings.description.unwrap_or("N/A".to_string()));
            println!("   - Show on Front: {}", settings.show_on_front.unwrap_or("posts".to_string()));
            println!("   - Posts per Page: {}", settings.posts_per_page.unwrap_or(10));
            println!("   - Timezone: {}", settings.timezone.unwrap_or("UTC".to_string()));
            println!("   - Language: {}", settings.language.unwrap_or("en_US".to_string()));
        },
        Err(e) => {
            error!("❌ Failed to get final settings: {}", e);
        }
    }

    println!("\n🎯 WordPress settings management test completed!");
    println!("📝 Note: This functionality allows AI agents to:");
    println!("   - Configure site appearance and behavior");
    println!("   - Set up static front pages for business sites");
    println!("   - Manage international settings (timezone, language)");
    println!("   - Control content display (posts per page, categories)");
    
    Ok(())
}