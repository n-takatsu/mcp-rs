/// ポリシー設定管理システムのデモンストレーション
///
/// 新しい統合PolicyConfigを使ったポリシーファイルの読み込みと
/// 設定管理機能を実証します。
use mcp_rs::policy_config::{PolicyConfig, PolicyLoader};
use std::path::Path;
use tracing::{error, info};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ログ設定
    tracing_subscriber::fmt::init();

    info!("ポリシー設定管理システムのデモを開始");

    // 1. デフォルトポリシーの作成と表示
    let default_policy = PolicyConfig::default();
    info!("デフォルトポリシー作成完了:");
    info!("  - ID: {}", default_policy.id);
    info!("  - 名前: {}", default_policy.name);
    info!("  - バージョン: {}", default_policy.version);
    info!("  - セキュリティ有効: {}", default_policy.security.enabled);
    info!(
        "  - 暗号化: {}",
        default_policy.security.encryption.algorithm
    );
    info!(
        "  - レート制限: {} req/min",
        default_policy.security.rate_limiting.requests_per_minute
    );

    // 2. 一時ディレクトリにサンプルファイルを作成
    let temp_dir = tempfile::TempDir::new()?;

    // JSON形式で保存
    let json_file = temp_dir.path().join("sample_policy.json");
    PolicyLoader::save_to_file(&default_policy, &json_file).await?;
    info!("JSON形式でポリシーを保存: {:?}", json_file);

    // TOML形式で保存
    let toml_file = temp_dir.path().join("sample_policy.toml");
    PolicyLoader::save_to_file(&default_policy, &toml_file).await?;
    info!("TOML形式でポリシーを保存: {:?}", toml_file);

    // YAML形式で保存
    let yaml_file = temp_dir.path().join("sample_policy.yaml");
    PolicyLoader::save_to_file(&default_policy, &yaml_file).await?;
    info!("YAML形式でポリシーを保存: {:?}", yaml_file);

    // 3. 各形式からの読み込みテスト
    info!("\n=== 各形式からの読み込みテスト ===");

    // JSON読み込み
    match PolicyLoader::load_from_file(&json_file).await {
        Ok(policy) => {
            info!("✓ JSON読み込み成功: {}", policy.name);
            info!("  監視間隔: {}秒", policy.monitoring.interval_seconds);
        }
        Err(e) => error!("✗ JSON読み込み失敗: {}", e),
    }

    // TOML読み込み
    match PolicyLoader::load_from_file(&toml_file).await {
        Ok(policy) => {
            info!("✓ TOML読み込み成功: {}", policy.name);
            info!("  TLS最小バージョン: {}", policy.security.tls.min_version);
        }
        Err(e) => error!("✗ TOML読み込み失敗: {}", e),
    }

    // YAML読み込み
    match PolicyLoader::load_from_file(&yaml_file).await {
        Ok(policy) => {
            info!("✓ YAML読み込み成功: {}", policy.name);
            info!("  認証方式: {}", policy.authentication.method);
            info!("  MFA必須: {}", policy.authentication.require_mfa);
        }
        Err(e) => error!("✗ YAML読み込み失敗: {}", e),
    }

    // 4. 既存のデモファイルの読み込みテスト
    info!("\n=== 既存デモファイルの読み込みテスト ===");

    let demo_policy_path = Path::new("demo-policies/security-policy-new.toml");
    if demo_policy_path.exists() {
        match PolicyLoader::load_from_file(demo_policy_path).await {
            Ok(policy) => {
                info!("✓ デモファイル読み込み成功:");
                info!("  - ID: {}", policy.id);
                info!("  - 名前: {}", policy.name);
                info!("  - バージョン: {}", policy.version);
                info!(
                    "  - 説明: {}",
                    policy.description.unwrap_or("なし".to_string())
                );
                info!(
                    "  - 暗号化アルゴリズム: {}",
                    policy.security.encryption.algorithm
                );
                info!(
                    "  - レート制限: {} req/min",
                    policy.security.rate_limiting.requests_per_minute
                );
                info!("  - ログレベル: {}", policy.monitoring.log_level);

                // カスタム設定の表示
                if !policy.custom.is_empty() {
                    info!("  - カスタム設定:");
                    for (key, value) in &policy.custom {
                        info!("    {}: {}", key, value);
                    }
                }
            }
            Err(e) => error!("✗ デモファイル読み込み失敗: {}", e),
        }
    } else {
        info!("デモファイルが見つかりません: {:?}", demo_policy_path);
    }

    // 5. ポリシー設定の比較
    info!("\n=== 設定値の比較 ===");
    info!("デフォルト vs カスタムポリシー:");
    info!(
        "  レート制限 (デフォルト): {} req/min",
        default_policy.security.rate_limiting.requests_per_minute
    );
    if demo_policy_path.exists() {
        if let Ok(custom_policy) = PolicyLoader::load_from_file(demo_policy_path).await {
            info!(
                "  レート制限 (カスタム): {} req/min",
                custom_policy.security.rate_limiting.requests_per_minute
            );
            info!(
                "  PBKDF2反復回数 (カスタム): {}",
                custom_policy.security.encryption.pbkdf2_iterations
            );
        }
    }

    info!("\n✅ ポリシー設定管理システムのデモ完了");
    Ok(())
}
