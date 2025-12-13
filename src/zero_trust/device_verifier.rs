//! デバイス検証モジュール

use crate::zero_trust::{TrustScore, VerificationResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// デバイス情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    /// デバイスID
    pub device_id: String,
    /// デバイスフィンガープリント
    pub fingerprint: String,
    /// OS名
    pub os_name: String,
    /// OSバージョン
    pub os_version: String,
    /// ブラウザ/クライアント情報
    pub user_agent: String,
    /// セキュリティパッチレベル
    pub security_patch_level: Option<String>,
    /// 企業デバイス登録済みフラグ
    pub is_managed: bool,
    /// 追加属性
    pub attributes: HashMap<String, String>,
}

impl DeviceInfo {
    /// User-Agentからデバイス情報を解析
    pub fn from_user_agent(device_id: impl Into<String>, user_agent: impl Into<String>) -> Self {
        let user_agent_str = user_agent.into();
        let (os_name, os_version) = Self::parse_os(&user_agent_str);

        Self {
            device_id: device_id.into(),
            fingerprint: Self::generate_fingerprint(&user_agent_str),
            os_name,
            os_version,
            user_agent: user_agent_str,
            security_patch_level: None,
            is_managed: false,
            attributes: HashMap::new(),
        }
    }

    /// OSとバージョンを解析
    fn parse_os(user_agent: &str) -> (String, String) {
        if user_agent.contains("Windows NT 10.0") {
            ("Windows".to_string(), "10".to_string())
        } else if user_agent.contains("Windows NT 6.3") {
            ("Windows".to_string(), "8.1".to_string())
        } else if user_agent.contains("Mac OS X") {
            let version = user_agent
                .split("Mac OS X ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("Unknown");
            ("macOS".to_string(), version.to_string())
        } else if user_agent.contains("Linux") {
            ("Linux".to_string(), "Unknown".to_string())
        } else if user_agent.contains("Android") {
            let version = user_agent
                .split("Android ")
                .nth(1)
                .and_then(|s| s.split(';').next())
                .unwrap_or("Unknown");
            ("Android".to_string(), version.to_string())
        } else if user_agent.contains("iOS") || user_agent.contains("iPhone") {
            let version = user_agent
                .split("OS ")
                .nth(1)
                .and_then(|s| s.split_whitespace().next())
                .unwrap_or("Unknown");
            ("iOS".to_string(), version.replace('_', "."))
        } else {
            ("Unknown".to_string(), "Unknown".to_string())
        }
    }

    /// デバイスフィンガープリントを生成
    fn generate_fingerprint(user_agent: &str) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        user_agent.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }
}

/// デバイス検証器
pub struct DeviceVerifier {
    /// 管理対象デバイスリスト
    managed_devices: HashMap<String, DeviceInfo>,
    /// 最小OSバージョン要件
    min_os_versions: HashMap<String, String>,
}

impl DeviceVerifier {
    /// 新しい検証器を作成
    pub fn new() -> Self {
        let mut min_os_versions = HashMap::new();
        min_os_versions.insert("Windows".to_string(), "10".to_string());
        min_os_versions.insert("macOS".to_string(), "12.0".to_string());
        min_os_versions.insert("Android".to_string(), "11".to_string());
        min_os_versions.insert("iOS".to_string(), "15.0".to_string());

        Self {
            managed_devices: HashMap::new(),
            min_os_versions,
        }
    }

    /// 管理対象デバイスを登録
    pub fn register_device(&mut self, device: DeviceInfo) {
        self.managed_devices
            .insert(device.device_id.clone(), device);
    }

    /// デバイスを検証
    pub fn verify(&self, device: &DeviceInfo) -> VerificationResult {
        let mut trust_score: TrustScore = 50; // 基本スコア
        let mut reasons = Vec::new();

        // 管理対象デバイスチェック
        if device.is_managed || self.managed_devices.contains_key(&device.device_id) {
            trust_score += 20;
            reasons.push("Managed device");
        } else {
            reasons.push("Unmanaged device");
        }

        // OSバージョンチェック
        if let Some(min_version) = self.min_os_versions.get(&device.os_name) {
            if Self::version_compare(&device.os_version, min_version) >= 0 {
                trust_score += 15;
                reasons.push("OS version meets requirements");
            } else {
                trust_score = trust_score.saturating_sub(20);
                reasons.push("OS version below minimum");
            }
        }

        // セキュリティパッチレベルチェック
        if device.security_patch_level.is_some() {
            trust_score += 10;
            reasons.push("Security patch level available");
        }

        // User-Agent妥当性チェック
        if !device.user_agent.is_empty() && device.user_agent.len() > 20 {
            trust_score += 5;
        } else {
            trust_score = trust_score.saturating_sub(10);
            reasons.push("Suspicious user agent");
        }

        let success = trust_score >= 60;

        VerificationResult {
            success,
            trust_score: trust_score.min(100),
            reason: reasons.join(", "),
            details: HashMap::new(),
        }
    }

    /// バージョン文字列を比較
    fn version_compare(version1: &str, version2: &str) -> i32 {
        let v1_parts: Vec<u32> = version1.split('.').filter_map(|s| s.parse().ok()).collect();
        let v2_parts: Vec<u32> = version2.split('.').filter_map(|s| s.parse().ok()).collect();

        for i in 0..v1_parts.len().max(v2_parts.len()) {
            let v1 = v1_parts.get(i).copied().unwrap_or(0);
            let v2 = v2_parts.get(i).copied().unwrap_or(0);

            if v1 > v2 {
                return 1;
            } else if v1 < v2 {
                return -1;
            }
        }

        0
    }
}

impl Default for DeviceVerifier {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_info_from_user_agent() {
        let device = DeviceInfo::from_user_agent(
            "device123",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        );

        assert_eq!(device.os_name, "Windows");
        assert_eq!(device.os_version, "10");
        assert!(!device.fingerprint.is_empty());
    }

    #[test]
    fn test_device_verifier() {
        let verifier = DeviceVerifier::new();

        let mut device = DeviceInfo::from_user_agent(
            "device123",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        );
        device.is_managed = true;
        device.security_patch_level = Some("2024-01".to_string());

        let result = verifier.verify(&device);

        assert!(result.success);
        assert!(result.trust_score >= 60);
    }

    #[test]
    fn test_version_compare() {
        assert_eq!(DeviceVerifier::version_compare("10.0", "9.0"), 1);
        assert_eq!(DeviceVerifier::version_compare("9.0", "10.0"), -1);
        assert_eq!(DeviceVerifier::version_compare("10.0", "10.0"), 0);
        assert_eq!(DeviceVerifier::version_compare("10.2.1", "10.2"), 1);
    }

    #[test]
    fn test_parse_macos() {
        let device = DeviceInfo::from_user_agent(
            "device123",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        );

        assert_eq!(device.os_name, "macOS");
        assert!(device.os_version.starts_with("10"));
    }

    #[test]
    fn test_unmanaged_device() {
        let verifier = DeviceVerifier::new();

        let device = DeviceInfo::from_user_agent(
            "device123",
            "Mozilla/5.0 (Windows NT 6.1) AppleWebKit/537.36", // Windows 7
        );

        let result = verifier.verify(&device);

        assert!(!result.success);
        assert!(result.trust_score < 60);
    }
}
