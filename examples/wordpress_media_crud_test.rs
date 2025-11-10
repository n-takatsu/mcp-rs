use mcp_rs::config::WordPressConfig;
use mcp_rs::handlers::wordpress::{MediaUpdateParams, WordPressHandler};
use std::collections::HashMap;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    // 直接WordPress設定を作成（テスト用）
    let wp_config = WordPressConfig {
        url: "https://demo.wp-api.org/wp-json".to_string(),
        username: "demo".to_string(),
        password: "demo".to_string(),
        enabled: Some(true),
        timeout_seconds: Some(30),
        rate_limit: None,
        encrypted_credentials: None, // 平文認証情報を使用
    };

    let handler = WordPressHandler::new(wp_config);

    println!("🚀 WordPress Media CRUD Test\n");

    // 1. メディア一覧の取得
    println!("📋 Getting all media files...");
    match handler.get_media().await {
        Ok(media_list) => {
            println!("✅ Found {} media files", media_list.len());

            // 最初の数件を表示
            for (i, media) in media_list.iter().take(3).enumerate() {
                println!(
                    "  {}. ID: {:?}, File: {}",
                    i + 1,
                    media.id,
                    media.source_url.as_deref().unwrap_or("No URL")
                );
            }

            // 最初のメディアで詳細テストを実行
            if let Some(first_media) = media_list.first() {
                if let Some(media_id) = first_media.id {
                    println!(
                        "\n🔍 Testing detailed operations with media ID: {}",
                        media_id
                    );

                    // 2. 単一メディアの取得
                    println!("\n📁 Getting media item details...");
                    match handler.get_media_item(media_id).await {
                        Ok(media) => {
                            println!("✅ Media Details:");
                            println!("   ID: {:?}", media.id);
                            println!(
                                "   Title: {}",
                                media
                                    .title
                                    .as_ref()
                                    .map(|t| t.rendered.as_str())
                                    .unwrap_or("No title")
                            );
                            println!(
                                "   Alt Text: {}",
                                media.alt_text.as_deref().unwrap_or("No alt text")
                            );
                            println!(
                                "   Caption: {}",
                                media
                                    .caption
                                    .as_ref()
                                    .map(|c| c.rendered.as_str())
                                    .unwrap_or("No caption")
                            );
                            println!(
                                "   Description: {}",
                                media
                                    .description
                                    .as_ref()
                                    .map(|d| d.rendered.as_str())
                                    .unwrap_or("No description")
                            );
                            println!(
                                "   MIME Type: {}",
                                media.mime_type.as_deref().unwrap_or("Unknown")
                            );
                            println!(
                                "   URL: {}",
                                media.source_url.as_deref().unwrap_or("No URL")
                            );

                            // 3. メディアメタデータの更新
                            println!("\n✏️ Updating media metadata...");
                            let update_params = MediaUpdateParams {
                                title: Some("Updated Media Title".to_string()),
                                alt_text: Some(
                                    "Updated alternative text for accessibility".to_string(),
                                ),
                                caption: Some("Updated caption with description".to_string()),
                                description: Some(
                                    "Updated detailed description of the media file".to_string(),
                                ),
                                post: None, // 投稿に添付しない
                            };

                            match handler.update_media(media_id, update_params).await {
                                Ok(updated_media) => {
                                    println!("✅ Media updated successfully!");
                                    println!(
                                        "   New Title: {}",
                                        updated_media
                                            .title
                                            .as_ref()
                                            .map(|t| t.rendered.as_str())
                                            .unwrap_or("No title")
                                    );
                                    println!(
                                        "   New Alt Text: {}",
                                        updated_media.alt_text.as_deref().unwrap_or("No alt text")
                                    );
                                }
                                Err(e) => println!("❌ Failed to update media: {}", e),
                            }

                            // 4. 更新後の確認
                            println!("\n🔍 Verifying updates...");
                            match handler.get_media_item(media_id).await {
                                Ok(verified_media) => {
                                    println!("✅ Verification successful:");
                                    println!(
                                        "   Title: {}",
                                        verified_media
                                            .title
                                            .as_ref()
                                            .map(|t| t.rendered.as_str())
                                            .unwrap_or("No title")
                                    );
                                    println!(
                                        "   Alt Text: {}",
                                        verified_media.alt_text.as_deref().unwrap_or("No alt text")
                                    );
                                }
                                Err(e) => println!("❌ Failed to verify updates: {}", e),
                            }
                        }
                        Err(e) => println!("❌ Failed to get media item: {}", e),
                    }
                }
            }
        }
        Err(e) => println!("❌ Failed to get media list: {}", e),
    }

    // 5. 新しいメディアファイルのアップロード（テスト用小画像）
    println!("\n📤 Testing media upload...");

    // 1x1ピクセルの透明PNG（Base64）
    let test_image_b64 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChAFJGbXJEQAAAABJRU5ErkJggg==";

    use base64::{engine::general_purpose, Engine as _};
    let test_image_data = general_purpose::STANDARD
        .decode(test_image_b64)
        .expect("Failed to decode test image");

    match handler
        .upload_media(&test_image_data, "test-image.png", "image/png")
        .await
    {
        Ok(uploaded_media) => {
            println!("✅ Test image uploaded!");
            println!("   ID: {:?}", uploaded_media.id);
            println!(
                "   URL: {}",
                uploaded_media.source_url.as_deref().unwrap_or("No URL")
            );

            if let Some(uploaded_id) = uploaded_media.id {
                // 6. アップロードしたテストメディアにメタデータを追加
                println!("\n🏷️ Adding metadata to uploaded media...");
                let metadata_params = MediaUpdateParams {
                    title: Some("Test Media Upload".to_string()),
                    alt_text: Some("Test image for CRUD operations".to_string()),
                    caption: Some("Automatically uploaded test image".to_string()),
                    description: Some("1x1 pixel test image for media CRUD testing".to_string()),
                    post: None,
                };

                match handler.update_media(uploaded_id, metadata_params).await {
                    Ok(_) => println!("✅ Metadata added to uploaded media"),
                    Err(e) => println!("❌ Failed to add metadata: {}", e),
                }

                // 7. テストメディアの削除（オプション）
                println!("\n🗑️ Cleaning up test media (moving to trash)...");
                match handler.delete_media(uploaded_id, Some(false)).await {
                    Ok(_) => println!("✅ Test media moved to trash"),
                    Err(e) => println!("❌ Failed to delete test media: {}", e),
                }
            }
        }
        Err(e) => println!("❌ Failed to upload test image: {}", e),
    }

    // 8. メディア統計情報
    println!("\n📊 Media Statistics:");
    match handler.get_media().await {
        Ok(final_media_list) => {
            let mut stats = HashMap::new();
            for media in &final_media_list {
                if let Some(mime_type) = &media.mime_type {
                    *stats.entry(mime_type.clone()).or_insert(0) += 1;
                }
            }

            println!("   Total media files: {}", final_media_list.len());
            println!("   File types:");
            for (mime_type, count) in stats {
                println!("     {}: {} files", mime_type, count);
            }
        }
        Err(e) => println!("❌ Failed to get final media statistics: {}", e),
    }

    println!("\n🎉 Media CRUD test completed!");

    Ok(())
}
