use mcp_rs::{config::McpConfig, handlers::wordpress::WordPressHandler};
use tracing::{error, info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("🏷️ WordPress Categories & Tags Test\n");

    // Load configuration
    let config = McpConfig::load()?;
    let wp_config = config
        .handlers
        .wordpress
        .ok_or("WordPress configuration missing")?;

    // Create WordPress handler
    let handler = WordPressHandler::new(wp_config);

    println!("=== WordPress Categories & Tags Test ===");

    // Test categories
    println!("\n📂 Testing Categories:");

    // Get existing categories
    match handler.get_categories().await {
        Ok(categories) => {
            info!("✅ Found {} categories", categories.len());
            for category in &categories {
                println!(
                    "  • {} (ID: {:?}): {}",
                    category.name, category.id, category.description
                );
            }
        }
        Err(e) => {
            error!("❌ Failed to get categories: {}", e);
        }
    }

    // Create a test category
    println!("\n📝 Creating test category...");
    match handler
        .create_category(
            "MCP Test Category",
            Some("Category created by MCP-RS"),
            None,
        )
        .await
    {
        Ok(category) => {
            info!(
                "✅ Created category: {} (ID: {:?})",
                category.name, category.id
            );

            // Try to update the category
            if let Some(category_id) = category.id {
                println!("✏️ Updating category...");
                match handler
                    .update_category(
                        category_id,
                        Some("MCP Updated Category"),
                        Some("Updated description"),
                    )
                    .await
                {
                    Ok(updated_category) => {
                        info!("✅ Updated category: {}", updated_category.name);
                    }
                    Err(e) => {
                        error!("❌ Failed to update category: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to create category: {}", e);
        }
    }

    // Test tags
    println!("\n🏷️ Testing Tags:");

    // Get existing tags
    match handler.get_tags().await {
        Ok(tags) => {
            info!("✅ Found {} tags", tags.len());
            for tag in &tags {
                println!("  • {} (ID: {:?}): {}", tag.name, tag.id, tag.description);
            }
        }
        Err(e) => {
            error!("❌ Failed to get tags: {}", e);
        }
    }

    // Create a test tag
    println!("\n🏷️ Creating test tag...");
    match handler
        .create_tag("mcp-test", Some("Tag created by MCP-RS"))
        .await
    {
        Ok(tag) => {
            info!("✅ Created tag: {} (ID: {:?})", tag.name, tag.id);

            // Try to update the tag
            if let Some(tag_id) = tag.id {
                println!("✏️ Updating tag...");
                match handler
                    .update_tag(tag_id, Some("mcp-updated"), Some("Updated tag description"))
                    .await
                {
                    Ok(updated_tag) => {
                        info!("✅ Updated tag: {}", updated_tag.name);
                    }
                    Err(e) => {
                        error!("❌ Failed to update tag: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to create tag: {}", e);
        }
    }

    println!("\n✅ Categories & Tags test completed!");
    println!("\n💡 Note: Test categories and tags were created. You may want to clean them up manually from your WordPress admin.");

    Ok(())
}
