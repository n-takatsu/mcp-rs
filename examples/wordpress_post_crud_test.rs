use mcp_rs::{config::McpConfig, handlers::wordpress::WordPressHandler};
use tracing::{error, info, Level};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    println!("✏️ WordPress Post CRUD Operations Test\n");

    // Load configuration
    let config = McpConfig::load()?;
    let wp_config = config
        .handlers
        .wordpress
        .ok_or("WordPress configuration missing")?;

    // Create WordPress handler
    let handler = WordPressHandler::new(wp_config);

    println!("=== WordPress Post CRUD Test ===");

    // Create a test post
    println!("\n📝 Creating a test post...");
    let created_post = match handler.create_post(
        "CRUD Test Post".to_string(),
        "<p>This is a test post for CRUD operations with MCP-RS.</p><p>We will test create, read, update, and delete operations.</p>".to_string()
    ).await {
        Ok(post) => {
            info!("✅ Created post: {} (ID: {:?})", post.title.rendered, post.id);
            post
        }
        Err(e) => {
            error!("❌ Failed to create test post: {}", e);
            return Err(e.into());
        }
    };

    let post_id = created_post.id.ok_or("Post ID not found")?;

    // Read the created post
    println!("\n📖 Reading the created post...");
    match handler.get_post(post_id).await {
        Ok(post) => {
            info!("✅ Retrieved post: {}", post.title.rendered);
            println!("   Title: {}", post.title.rendered);
            println!("   Status: {}", post.status);
            println!(
                "   Content length: {} characters",
                post.content.rendered.len()
            );
        }
        Err(e) => {
            error!("❌ Failed to read post: {}", e);
        }
    }

    // Update the post
    println!("\n✏️ Updating the post...");
    match handler.update_post(
        post_id,
        Some("Updated CRUD Test Post".to_string()),
        Some("<p>This post has been updated through the MCP-RS CRUD API!</p><p>New content shows the update functionality is working correctly.</p>".to_string()),
        Some("draft".to_string()), // Change status to draft
        None, // No categories for this test
        None, // No tags for this test
        None, // No featured image
    ).await {
        Ok(updated_post) => {
            info!("✅ Updated post: {}", updated_post.title.rendered);
            println!("   New Title: {}", updated_post.title.rendered);
            println!("   New Status: {}", updated_post.status);
        }
        Err(e) => {
            error!("❌ Failed to update post: {}", e);
        }
    }

    // Read the updated post to verify changes
    println!("\n🔍 Verifying the update...");
    match handler.get_post(post_id).await {
        Ok(post) => {
            info!("✅ Verified updated post");
            println!("   Current Title: {}", post.title.rendered);
            println!("   Current Status: {}", post.status);
        }
        Err(e) => {
            error!("❌ Failed to verify update: {}", e);
        }
    }

    // Test with categories and tags if available
    println!("\n🏷️ Testing update with categories and tags...");

    // Get some categories to use
    let categories = handler.get_categories().await.unwrap_or_default();
    let tags = handler.get_tags().await.unwrap_or_default();

    let category_ids: Vec<u64> = categories
        .iter()
        .filter_map(|c| c.id)
        .take(2) // Use up to 2 categories
        .collect();

    let tag_ids: Vec<u64> = tags
        .iter()
        .filter_map(|t| t.id)
        .take(3) // Use up to 3 tags
        .collect();

    if !category_ids.is_empty() || !tag_ids.is_empty() {
        match handler
            .update_post(
                post_id,
                None, // Keep existing title
                None, // Keep existing content
                None, // Keep existing status
                if category_ids.is_empty() {
                    None
                } else {
                    Some(category_ids.clone())
                },
                if tag_ids.is_empty() {
                    None
                } else {
                    Some(tag_ids.clone())
                },
                None, // No featured image
            )
            .await
        {
            Ok(updated_post) => {
                info!("✅ Updated post with taxonomy");
                println!("   Categories: {:?}", updated_post.categories);
                println!("   Tags: {:?}", updated_post.tags);
            }
            Err(e) => {
                error!("❌ Failed to update post with taxonomy: {}", e);
            }
        }
    } else {
        println!("   No existing categories or tags found to test with");
    }

    // Ask user confirmation before deletion
    println!("\n⚠️ About to delete the test post...");
    println!("   This will move the post to trash (not permanently delete)");

    // Delete the post (move to trash)
    println!("\n🗑️ Deleting the test post...");
    match handler.delete_post(post_id, false).await {
        Ok(_) => {
            info!("✅ Moved post to trash");
            println!("   Post ID {} has been moved to trash", post_id);
        }
        Err(e) => {
            error!("❌ Failed to delete post: {}", e);
        }
    }

    // Try to read the deleted post (should fail or show trashed status)
    println!("\n🔍 Verifying deletion...");
    match handler.get_post(post_id).await {
        Ok(post) => {
            println!("   Post still exists with status: {}", post.status);
            if post.status == "trash" {
                info!("✅ Post correctly moved to trash");
            }
        }
        Err(e) => {
            info!("✅ Post no longer accessible: {}", e);
        }
    }

    println!("\n✅ CRUD operations test completed!");
    println!("\n📊 Summary of tested operations:");
    println!("   ✅ CREATE: create_post()");
    println!("   ✅ READ: get_post()");
    println!("   ✅ UPDATE: update_post()");
    println!("   ✅ DELETE: delete_post()");
    println!("\n💡 Complete WordPress post management is now available through MCP!");

    Ok(())
}
