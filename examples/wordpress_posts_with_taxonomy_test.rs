use mcp_rs::{config::McpConfig, handlers::wordpress::WordPressHandler};
use tracing::{error, info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("📝 WordPress Posts with Categories & Tags Test\n");

    // Load configuration
    let config = McpConfig::load()?;
    let wp_config = config
        .handlers
        .wordpress
        .ok_or("WordPress configuration missing")?;

    // Create WordPress handler
    let handler = WordPressHandler::new(wp_config);

    println!("=== WordPress Posts with Categories & Tags Test ===");

    // First, let's get existing categories and tags
    println!("\n📂 Getting existing categories and tags...");

    let categories = match handler.get_categories().await {
        Ok(cats) => {
            info!("✅ Found {} categories", cats.len());
            for cat in &cats {
                println!("  Category: {} (ID: {:?})", cat.name, cat.id);
            }
            cats
        }
        Err(e) => {
            error!("❌ Failed to get categories: {}", e);
            Vec::new()
        }
    };

    let tags = match handler.get_tags().await {
        Ok(tag_list) => {
            info!("✅ Found {} tags", tag_list.len());
            for tag in &tag_list {
                println!("  Tag: {} (ID: {:?})", tag.name, tag.id);
            }
            tag_list
        }
        Err(e) => {
            error!("❌ Failed to get tags: {}", e);
            Vec::new()
        }
    };

    // Create test categories and tags if needed
    println!("\n📝 Creating test category and tag...");

    let test_category = match handler
        .create_category("MCP Test Category", Some("Test category for MCP"), None)
        .await
    {
        Ok(category) => {
            info!(
                "✅ Created test category: {} (ID: {:?})",
                category.name, category.id
            );
            category
        }
        Err(e) => {
            error!("❌ Failed to create test category: {}", e);
            return Err(e.into());
        }
    };

    let test_tag = match handler
        .create_tag("mcp-test", Some("Test tag for MCP"))
        .await
    {
        Ok(tag) => {
            info!("✅ Created test tag: {} (ID: {:?})", tag.name, tag.id);
            tag
        }
        Err(e) => {
            error!("❌ Failed to create test tag: {}", e);
            return Err(e.into());
        }
    };

    // Create a post with categories and tags
    println!("\n📝 Creating post with categories and tags...");

    let category_ids = vec![test_category.id.unwrap()];
    let tag_ids = vec![test_tag.id.unwrap()];

    match handler.create_post_with_categories_tags(
        "Test Post with Categories and Tags".to_string(),
        "<p>This is a test post created with MCP-RS, featuring both categories and tags!</p><p>Categories help organize content hierarchically, while tags provide flexible labeling.</p>".to_string(),
        Some(category_ids.clone()),
        Some(tag_ids.clone()),
        None // No featured image for this test
    ).await {
        Ok(post) => {
            info!("✅ Created post: {} (ID: {:?})", post.title.rendered, post.id);
            info!("   Categories: {:?}", post.categories);
            info!("   Tags: {:?}", post.tags);

            // Test updating the post's categories and tags
            if let Some(post_id) = post.id {
                println!("\n✏️ Testing category and tag update...");

                // Add existing categories/tags if available
                let mut updated_categories = category_ids;
                let mut updated_tags = tag_ids;

                if let Some(first_existing_cat) = categories.first().and_then(|c| c.id) {
                    updated_categories.push(first_existing_cat);
                }

                if let Some(first_existing_tag) = tags.first().and_then(|t| t.id) {
                    updated_tags.push(first_existing_tag);
                }

                match handler.update_post_categories_tags(
                    post_id,
                    Some(updated_categories),
                    Some(updated_tags)
                ).await {
                    Ok(updated_post) => {
                        info!("✅ Updated post categories and tags");
                        info!("   New Categories: {:?}", updated_post.categories);
                        info!("   New Tags: {:?}", updated_post.tags);
                    }
                    Err(e) => {
                        error!("❌ Failed to update post categories/tags: {}", e);
                    }
                }
            }
        }
        Err(e) => {
            error!("❌ Failed to create post with categories and tags: {}", e);
        }
    }

    println!("\n✅ Posts with Categories & Tags test completed!");
    println!("\n💡 Note: Test categories, tags, and posts were created. You may want to clean them up manually from your WordPress admin.");
    println!("\n🔧 AI Agent Usage Tips:");
    println!("   1. Use get_categories and get_tags to understand existing taxonomy");
    println!("   2. Implement fuzzy matching for user input (e.g., 'web dev' → 'Web Development')");
    println!("   3. Suggest existing categories/tags before creating new ones");
    println!("   4. Use hierarchical categories for better content organization");

    Ok(())
}
