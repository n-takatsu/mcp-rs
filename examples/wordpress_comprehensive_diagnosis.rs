// WordPress接続診断・レポートシステム
// 一般ユーザー向けの包括的な診断とわかりやすいレポート生成

use std::error::Error;

#[derive(Debug, Clone)]
struct DiagnosticResult {
    test_name: String,
    status: TestStatus,
    details: String,
    user_action: Option<String>,
    technical_info: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum TestStatus {
    Success,
    Warning,
    Failed,
    Critical,
}

impl TestStatus {
    fn icon(&self) -> &str {
        match self {
            TestStatus::Success => "✅",
            TestStatus::Warning => "⚠️",
            TestStatus::Failed => "❌",
            TestStatus::Critical => "🚨",
        }
    }
    
    fn label(&self) -> &str {
        match self {
            TestStatus::Success => "成功",
            TestStatus::Warning => "注意",
            TestStatus::Failed => "失敗",
            TestStatus::Critical => "致命的",
        }
    }
}

struct WordPressDiagnostic {
    url: String,
    username: String,
    password: String,
    results: Vec<DiagnosticResult>,
    client: reqwest::Client,
}

impl WordPressDiagnostic {
    fn new(url: String, username: String, password: String) -> Self {
        Self {
            url,
            username,
            password,
            results: Vec::new(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(15))
                .build()
                .unwrap(),
        }
    }
    
    fn add_result(&mut self, result: DiagnosticResult) {
        self.results.push(result);
    }
    
    async fn test_basic_connectivity(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🌐 基本接続テスト中...");
        
        match self.client.get(&self.url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    self.add_result(DiagnosticResult {
                        test_name: "WordPress サイト基本接続".to_string(),
                        status: TestStatus::Success,
                        details: format!("サイトに正常にアクセスできました (ステータス: {})", response.status()),
                        user_action: None,
                        technical_info: Some(format!("HTTP Status: {}", response.status())),
                    });
                } else {
                    self.add_result(DiagnosticResult {
                        test_name: "WordPress サイト基本接続".to_string(),
                        status: TestStatus::Failed,
                        details: format!("サイトアクセスに失敗しました (ステータス: {})", response.status()),
                        user_action: Some("サイトのURLが正しいか、サイトが稼働中か確認してください".to_string()),
                        technical_info: Some(format!("HTTP Status: {}", response.status())),
                    });
                }
            }
            Err(e) => {
                self.add_result(DiagnosticResult {
                    test_name: "WordPress サイト基本接続".to_string(),
                    status: TestStatus::Critical,
                    details: "サイトに接続できませんでした".to_string(),
                    user_action: Some("インターネット接続とサイトURLを確認してください".to_string()),
                    technical_info: Some(format!("Error: {}", e)),
                });
            }
        }
        
        Ok(())
    }
    
    async fn test_rest_api_availability(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🔌 REST API 利用可能性テスト中...");
        
        let api_url = format!("{}/wp-json/wp/v2/", self.url);
        match self.client.get(&api_url).send().await {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(json) = response.json::<serde_json::Value>().await {
                        let namespaces = json.get("namespaces")
                            .and_then(|n| n.as_array())
                            .map(|arr| arr.len())
                            .unwrap_or(0);
                        
                        self.add_result(DiagnosticResult {
                            test_name: "WordPress REST API 利用可能性".to_string(),
                            status: TestStatus::Success,
                            details: format!("REST APIが正常に動作しています ({} 個の名前空間が利用可能)", namespaces),
                            user_action: None,
                            technical_info: Some(format!("Available namespaces: {}", namespaces)),
                        });
                    } else {
                        self.add_result(DiagnosticResult {
                            test_name: "WordPress REST API 利用可能性".to_string(),
                            status: TestStatus::Warning,
                            details: "REST APIにアクセスできますが、レスポンス形式が異常です".to_string(),
                            user_action: Some("WordPressの設定やプラグインの影響を確認してください".to_string()),
                            technical_info: Some("Invalid JSON response".to_string()),
                        });
                    }
                } else {
                    self.add_result(DiagnosticResult {
                        test_name: "WordPress REST API 利用可能性".to_string(),
                        status: TestStatus::Failed,
                        details: "REST APIが無効になっているか、アクセスが制限されています".to_string(),
                        user_action: Some("WordPress管理画面でREST APIの設定を確認してください".to_string()),
                        technical_info: Some(format!("HTTP Status: {}", response.status())),
                    });
                }
            }
            Err(e) => {
                self.add_result(DiagnosticResult {
                    test_name: "WordPress REST API 利用可能性".to_string(),
                    status: TestStatus::Critical,
                    details: "REST APIにアクセスできませんでした".to_string(),
                    user_action: Some("WordPressのパーマリンク設定を確認してください".to_string()),
                    technical_info: Some(format!("Error: {}", e)),
                });
            }
        }
        
        Ok(())
    }
    
    async fn test_application_password_introspection(&mut self) -> Result<(), Box<dyn Error>> {
        println!("🔍 アプリケーションパスワード検証中...");
        
        let introspect_url = format!("{}/wp-json/wp/v2/users/me/application-passwords/introspect", self.url);
        match self.client
            .get(&introspect_url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(data) = response.json::<serde_json::Value>().await {
                        let name = data.get("name").and_then(|n| n.as_str()).unwrap_or("不明");
                        let created = data.get("created").and_then(|c| c.as_str()).unwrap_or("不明");
                        
                        self.add_result(DiagnosticResult {
                            test_name: "アプリケーションパスワード検証".to_string(),
                            status: TestStatus::Success,
                            details: format!("アプリケーションパスワード「{}」が正常に認識されています (作成日: {})", name, created),
                            user_action: None,
                            technical_info: Some(format!("Password name: {}, Created: {}", name, created)),
                        });
                    }
                } else {
                    let status_code = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    self.add_result(DiagnosticResult {
                        test_name: "アプリケーションパスワード検証".to_string(),
                        status: TestStatus::Failed,
                        details: "アプリケーションパスワードが認識されていません".to_string(),
                        user_action: Some("WordPress管理画面で新しいアプリケーションパスワードを生成してください".to_string()),
                        technical_info: Some(format!("HTTP {}: {}", status_code, error_text)),
                    });
                }
            }
            Err(e) => {
                self.add_result(DiagnosticResult {
                    test_name: "アプリケーションパスワード検証".to_string(),
                    status: TestStatus::Critical,
                    details: "アプリケーションパスワードの検証に失敗しました".to_string(),
                    user_action: Some("ネットワーク接続とWordPressの設定を確認してください".to_string()),
                    technical_info: Some(format!("Error: {}", e)),
                });
            }
        }
        
        Ok(())
    }
    
    async fn test_user_authentication(&mut self) -> Result<(), Box<dyn Error>> {
        println!("👤 ユーザー認証テスト中...");
        
        let users_me_url = format!("{}/wp-json/wp/v2/users/me", self.url);
        match self.client
            .get(&users_me_url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(user_data) = response.json::<serde_json::Value>().await {
                        let name = user_data.get("name").and_then(|n| n.as_str()).unwrap_or("不明");
                        let roles = user_data.get("roles")
                            .and_then(|r| r.as_array())
                            .map(|arr| arr.len())
                            .unwrap_or(0);
                        
                        self.add_result(DiagnosticResult {
                            test_name: "ユーザー認証".to_string(),
                            status: TestStatus::Success,
                            details: format!("ユーザー「{}」として正常に認証されました ({} 個の権限)", name, roles),
                            user_action: None,
                            technical_info: Some(format!("User: {}, Roles count: {}", name, roles)),
                        });
                    }
                } else {
                    let status_code = response.status();
                    let error_text = response.text().await.unwrap_or_default();
                    self.add_result(DiagnosticResult {
                        test_name: "ユーザー認証".to_string(),
                        status: TestStatus::Failed,
                        details: "ユーザー認証に失敗しました".to_string(),
                        user_action: Some("ユーザー名とアプリケーションパスワードを確認してください".to_string()),
                        technical_info: Some(format!("HTTP {}: {}", status_code, error_text)),
                    });
                }
            }
            Err(e) => {
                self.add_result(DiagnosticResult {
                    test_name: "ユーザー認証".to_string(),
                    status: TestStatus::Critical,
                    details: "認証テストでエラーが発生しました".to_string(),
                    user_action: Some("ネットワーク接続とWordPressの設定を確認してください".to_string()),
                    technical_info: Some(format!("Error: {}", e)),
                });
            }
        }
        
        Ok(())
    }
    
    async fn test_content_operations(&mut self) -> Result<(), Box<dyn Error>> {
        println!("📄 コンテンツ操作テスト中...");
        
        // 投稿一覧取得テスト
        let posts_url = format!("{}/wp-json/wp/v2/posts?per_page=1", self.url);
        match self.client
            .get(&posts_url)
            .basic_auth(&self.username, Some(&self.password))
            .send()
            .await
        {
            Ok(response) => {
                if response.status().is_success() {
                    if let Ok(posts_data) = response.json::<serde_json::Value>().await {
                        if let Some(posts_array) = posts_data.as_array() {
                            self.add_result(DiagnosticResult {
                                test_name: "コンテンツ操作 (投稿一覧取得)".to_string(),
                                status: TestStatus::Success,
                                details: format!("投稿一覧を正常に取得できました ({} 件のサンプル)", posts_array.len()),
                                user_action: None,
                                technical_info: Some(format!("Posts retrieved: {}", posts_array.len())),
                            });
                        }
                    }
                } else {
                    self.add_result(DiagnosticResult {
                        test_name: "コンテンツ操作 (投稿一覧取得)".to_string(),
                        status: TestStatus::Warning,
                        details: "投稿一覧の取得に失敗しました".to_string(),
                        user_action: Some("投稿の権限設定を確認してください".to_string()),
                        technical_info: Some(format!("HTTP Status: {}", response.status())),
                    });
                }
            }
            Err(e) => {
                self.add_result(DiagnosticResult {
                    test_name: "コンテンツ操作 (投稿一覧取得)".to_string(),
                    status: TestStatus::Failed,
                    details: "投稿一覧取得でエラーが発生しました".to_string(),
                    user_action: Some("ネットワーク接続を確認してください".to_string()),
                    technical_info: Some(format!("Error: {}", e)),
                });
            }
        }
        
        Ok(())
    }
    
    fn generate_user_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str("═══════════════════════════════════════════════════════\n");
        report.push_str("          WordPress 接続診断レポート\n");
        report.push_str("═══════════════════════════════════════════════════════\n\n");
        
        report.push_str(&format!("📅 診断日時: {}\n", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()));
        report.push_str(&format!("🌐 サイトURL: {}\n", self.url));
        report.push_str(&format!("👤 ユーザー名: {}\n\n", self.username));
        
        // 概要統計
        let success_count = self.results.iter().filter(|r| r.status == TestStatus::Success).count();
        let warning_count = self.results.iter().filter(|r| r.status == TestStatus::Warning).count();
        let failed_count = self.results.iter().filter(|r| r.status == TestStatus::Failed).count();
        let critical_count = self.results.iter().filter(|r| r.status == TestStatus::Critical).count();
        
        report.push_str("📊 診断結果概要:\n");
        report.push_str(&format!("   ✅ 成功: {} 項目\n", success_count));
        report.push_str(&format!("   ⚠️  警告: {} 項目\n", warning_count));
        report.push_str(&format!("   ❌ 失敗: {} 項目\n", failed_count));
        report.push_str(&format!("   🚨 致命的: {} 項目\n\n", critical_count));
        
        // 全体判定
        if critical_count > 0 {
            report.push_str("🚨 総合判定: 致命的な問題があります\n");
            report.push_str("   → WordPress サイトへの接続ができません\n\n");
        } else if failed_count > 0 {
            report.push_str("❌ 総合判定: 設定に問題があります\n");
            report.push_str("   → 一部の機能が正常に動作しません\n\n");
        } else if warning_count > 0 {
            report.push_str("⚠️ 総合判定: 注意が必要です\n");
            report.push_str("   → 基本的な接続は可能ですが改善の余地があります\n\n");
        } else {
            report.push_str("✅ 総合判定: 正常に動作しています\n");
            report.push_str("   → WordPress との接続は完全に正常です\n\n");
        }
        
        // 詳細結果
        report.push_str("📋 詳細診断結果:\n");
        report.push_str("─────────────────────────────────────────────────────\n");
        
        for result in &self.results {
            report.push_str(&format!("{} {} ({})\n", 
                result.status.icon(), 
                result.test_name, 
                result.status.label()
            ));
            report.push_str(&format!("   詳細: {}\n", result.details));
            
            if let Some(action) = &result.user_action {
                report.push_str(&format!("   💡 対処法: {}\n", action));
            }
            
            report.push_str("\n");
        }
        
        // 推奨アクション
        if critical_count > 0 || failed_count > 0 {
            report.push_str("🔧 推奨される対処手順:\n");
            report.push_str("─────────────────────────────────────────────────────\n");
            
            let mut actions = Vec::new();
            for result in &self.results {
                if matches!(result.status, TestStatus::Critical | TestStatus::Failed) {
                    if let Some(action) = &result.user_action {
                        if !actions.contains(action) {
                            actions.push(action.clone());
                        }
                    }
                }
            }
            
            for (i, action) in actions.iter().enumerate() {
                report.push_str(&format!("{}. {}\n", i + 1, action));
            }
            
            report.push_str("\n");
        }
        
        // サポート情報
        report.push_str("📞 追加サポート:\n");
        report.push_str("─────────────────────────────────────────────────────\n");
        report.push_str("問題が解決しない場合は、以下の情報と共にサポートにお問い合わせください:\n\n");
        report.push_str("• このレポート全文\n");
        report.push_str("• WordPressのバージョン\n");
        report.push_str("• 有効なプラグイン一覧\n");
        report.push_str("• サーバー環境（共有ホスティング/VPS等）\n\n");
        
        report.push_str("═══════════════════════════════════════════════════════\n");
        
        report
    }
    
    fn generate_technical_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str("═══════════════════════════════════════════════════════\n");
        report.push_str("          WordPress 接続診断 技術レポート\n");
        report.push_str("═══════════════════════════════════════════════════════\n\n");
        
        for result in &self.results {
            report.push_str(&format!("Test: {}\n", result.test_name));
            report.push_str(&format!("Status: {:?}\n", result.status));
            report.push_str(&format!("Details: {}\n", result.details));
            
            if let Some(tech_info) = &result.technical_info {
                report.push_str(&format!("Technical Info: {}\n", tech_info));
            }
            
            report.push_str("\n");
        }
        
        report
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("🔍 WordPress 接続診断システム v1.0");
    println!("═══════════════════════════════════════════════════════\n");
    
    // 設定ファイルから読み込み
    let config_content = std::fs::read_to_string("mcp-config.toml")?;
    let config: toml::Value = toml::from_str(&config_content)?;
    
    let wp_config = config
        .get("handlers")
        .and_then(|h| h.get("wordpress"))
        .ok_or("WordPress設定が見つかりません")?;
    
    let url = wp_config.get("url")
        .and_then(|u| u.as_str())
        .ok_or("URLが設定されていません")?
        .to_string();
    
    let username = wp_config.get("username")
        .and_then(|u| u.as_str())
        .ok_or("ユーザー名が設定されていません")?
        .to_string();
    
    let password = wp_config.get("password")
        .and_then(|p| p.as_str())
        .ok_or("パスワードが設定されていません")?
        .to_string();
    
    // 診断実行
    let mut diagnostic = WordPressDiagnostic::new(url, username, password);
    
    diagnostic.test_basic_connectivity().await?;
    diagnostic.test_rest_api_availability().await?;
    diagnostic.test_application_password_introspection().await?;
    diagnostic.test_user_authentication().await?;
    diagnostic.test_content_operations().await?;
    
    println!("\n🎯 診断完了！レポートを生成中...\n");
    
    // ユーザー向けレポート表示
    println!("{}", diagnostic.generate_user_report());
    
    // レポートファイル保存
    let user_report = diagnostic.generate_user_report();
    let technical_report = diagnostic.generate_technical_report();
    
    std::fs::write("wordpress-diagnosis-report.txt", &user_report)?;
    std::fs::write("wordpress-diagnosis-technical.txt", &technical_report)?;
    
    println!("📄 レポートファイルを保存しました:");
    println!("   • wordpress-diagnosis-report.txt (一般ユーザー向け)");
    println!("   • wordpress-diagnosis-technical.txt (技術者向け)");
    
    Ok(())
}