//! Zero Trust デモプログラム

use mcp_rs::zero_trust::{
    continuous_auth::{AuthEventType, ContinuousAuth},
    device_verifier::{DeviceInfo, DeviceVerifier},
    micro_segmentation::{AccessPolicy, MicroSegmentation, ResourceSegment},
    network_analyzer::{NetworkAnalyzer, NetworkInfo},
    AccessRequest,
};
use std::collections::HashSet;
use std::net::{IpAddr, Ipv4Addr};

fn main() {
    println!("=== Zero Trust ネットワークアクセス制御デモ ===\n");

    // 1. デバイス検証のデモ
    demo_device_verification();

    // 2. ネットワーク分析のデモ
    demo_network_analysis();

    // 3. マイクロセグメンテーションのデモ
    demo_micro_segmentation();

    // 4. 継続的認証のデモ
    demo_continuous_authentication();

    // 5. 完全なZero Trustフローのデモ
    demo_complete_flow();

    println!("\n=== デモ完了 ===");
}

fn demo_device_verification() {
    println!("--- 1. デバイス検証デモ ---");

    let mut verifier = DeviceVerifier::new();

    // デバイスの登録
    let device_id = "device-001";
    let user_agent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36";

    let device_info = DeviceInfo::from_user_agent(device_id, user_agent);
    verifier.register_device(device_info.clone());
    println!("デバイス登録: {}", device_id);

    // デバイス情報の解析
    println!(
        "OS: {}, バージョン: {}",
        device_info.os_name, device_info.os_version
    );

    // デバイスの検証
    let result = verifier.verify(&device_info);
    println!(
        "検証結果: {} (トラストスコア: {})",
        if result.success { "成功" } else { "失敗" },
        result.trust_score
    );
    println!("理由: {}\n", result.reason);
}

fn demo_network_analysis() {
    println!("--- 2. ネットワーク分析デモ ---");

    let mut analyzer = NetworkAnalyzer::new();
    let user_id = "user-001";

    // 通常の接続
    let normal_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    let mut normal_network = NetworkInfo::new(normal_ip);
    normal_network.country_code = Some("JP".to_string());
    normal_network.is_vpn = false;
    normal_network.is_tor = false;

    println!("通常の接続を分析中...");
    let result = analyzer.analyze(user_id, &normal_network);
    println!(
        "  接続: {} ({})",
        normal_network.source_ip,
        normal_network.country_code.as_ref().unwrap()
    );
    println!(
        "分析結果: {} (トラストスコア: {})",
        if result.success { "正常" } else { "異常" },
        result.trust_score
    );

    // VPN接続のテスト
    let vpn_ip = IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1));
    let mut vpn_network = NetworkInfo::new(vpn_ip);
    vpn_network.country_code = Some("US".to_string());
    vpn_network.is_vpn = true;
    vpn_network.is_tor = false;

    println!("\nVPN接続をテスト...");
    let vpn_result = analyzer.analyze(user_id, &vpn_network);
    println!(
        "VPN検知: {} (トラストスコア: {})",
        if vpn_network.is_vpn {
            "はい"
        } else {
            "いいえ"
        },
        vpn_result.trust_score
    );
    println!();
}

fn demo_micro_segmentation() {
    println!("--- 3. マイクロセグメンテーションデモ ---");

    let mut segmentation = MicroSegmentation::new();

    // カスタムセグメントの作成
    let mut api_segment = ResourceSegment::new("api");
    api_segment.add_resource("/api/data/*");
    api_segment.add_resource("/api/reports/*");
    api_segment.policies.push(
        AccessPolicy::new("/api/data/*", "/api/data", 60)
            .with_action("read")
            .with_action("write")
            .with_role("developer"),
    );

    segmentation.add_segment(api_segment);
    println!("セグメント作成: api");

    // アクセステスト
    let request = AccessRequest::new(
        "dev-001",
        "device-001",
        IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100)),
        "/api/data/users",
        "read",
    );

    let mut roles = HashSet::new();
    roles.insert("developer".to_string());

    println!("アクセスリクエスト:");
    println!("  リソース: {}", request.resource);
    println!("  アクション: {}", request.action);
    println!("  ロール: developer");

    let result = segmentation.evaluate_access(&request, &roles, 70);
    println!(
        "結果: {} (理由: {})",
        if result.success { "許可" } else { "拒否" },
        result.reason
    );
    println!();
}

fn demo_continuous_authentication() {
    println!("--- 4. 継続的認証デモ ---");

    let mut auth = ContinuousAuth::new();
    let session_id = "session-001";

    // セッション開始
    auth.start_session(session_id, "user-001", "device-001", 80);
    println!("セッション開始: {}", session_id);

    let session = auth.get_session(session_id).unwrap();
    println!(
        "初期トラストスコア: {} (リスクレベル: {:?})",
        session.current_trust_score, session.risk_level
    );

    // セッション検証
    let result = auth.verify_session(session_id);
    println!("検証: {}", if result.success { "有効" } else { "無効" });

    // トラストスコアの更新
    println!("\n場所変更を検知...");
    auth.update_trust_score(
        session_id,
        65,
        AuthEventType::LocationChange,
        "東京 -> 大阪",
    )
    .unwrap();

    let session = auth.get_session(session_id).unwrap();
    println!(
        "更新後トラストスコア: {} (リスクレベル: {:?})",
        session.current_trust_score, session.risk_level
    );

    // 異常行動の検知
    println!("\n異常行動を検知...");
    auth.handle_anomaly(session_id, "短時間に大量のリクエスト")
        .unwrap();

    let session = auth.get_session(session_id).unwrap();
    println!(
        "異常検知後: トラストスコア={}, リスクレベル={:?}",
        session.current_trust_score, session.risk_level
    );
    println!();
}

fn demo_complete_flow() {
    println!("--- 5. 完全なZero Trustフロー ---");

    // 各コンポーネントの初期化
    let mut device_verifier = DeviceVerifier::new();
    let mut network_analyzer = NetworkAnalyzer::new();
    let segmentation = MicroSegmentation::default();
    let mut continuous_auth = ContinuousAuth::new();

    let user_id = "user-001";
    let device_id = "device-001";
    let session_id = "session-001";

    println!("ステップ 1: デバイス検証");
    let user_agent = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)";
    let device_info = DeviceInfo::from_user_agent(device_id, user_agent);
    device_verifier.register_device(device_info.clone());
    let device_result = device_verifier.verify(&device_info);
    println!("  → トラストスコア: {}", device_result.trust_score);

    println!("\nステップ 2: ネットワーク分析");
    let network_ip = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 100));
    let mut network_info = NetworkInfo::new(network_ip);
    network_info.country_code = Some("JP".to_string());
    network_info.is_vpn = false;
    network_info.is_tor = false;
    let network_result = network_analyzer.analyze(user_id, &network_info);
    println!("  → トラストスコア: {}", network_result.trust_score);

    println!("\nステップ 3: 総合トラストスコア計算");
    let trust_score = (device_result.trust_score + network_result.trust_score) / 2;
    println!("  → 総合トラストスコア: {}", trust_score);

    println!("\nステップ 4: セッション開始");
    continuous_auth.start_session(session_id, user_id, device_id, trust_score);
    println!("  → セッションID: {}", session_id);

    println!("\nステップ 5: アクセス制御");
    let request = AccessRequest::new(
        user_id,
        device_id,
        network_info.source_ip,
        "/api/public/info",
        "read",
    );

    let mut roles = HashSet::new();
    roles.insert("user".to_string());

    let access_result = segmentation.evaluate_access(&request, &roles, trust_score);
    println!("  リソース: {}", request.resource);
    println!("  アクション: {}", request.action);
    println!(
        "  結果: {}",
        if access_result.success {
            "アクセス許可"
        } else {
            "アクセス拒否"
        }
    );

    println!("\nステップ 6: 継続的認証");
    let verify_result = continuous_auth.verify_session(session_id);
    println!(
        "  セッション状態: {}",
        if verify_result.success {
            "有効"
        } else {
            "無効"
        }
    );

    println!("\n✅ Zero Trustフロー完了");
}
