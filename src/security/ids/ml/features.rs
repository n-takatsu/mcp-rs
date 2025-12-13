//! Feature Extraction
//!
//! リクエストデータから機械学習用の特徴量を抽出します。

use super::{FeatureVector, MLConfig};
use crate::error::McpError;
use crate::security::ids::RequestData;
use chrono::{Datelike, Timelike, Utc};
use std::collections::HashMap;
use tracing::debug;

/// 特徴量抽出器
pub struct FeatureExtractor {
    /// 設定
    config: MLConfig,
    /// 特徴量の統計情報（正規化用）
    feature_stats: HashMap<String, FeatureStats>,
}

/// 特徴量統計情報
#[derive(Debug, Clone)]
struct FeatureStats {
    /// 平均値
    mean: f64,
    /// 標準偏差
    std_dev: f64,
    /// 最小値
    min: f64,
    /// 最大値
    max: f64,
}

impl FeatureExtractor {
    /// 新しい特徴量抽出器を作成
    pub fn new(config: MLConfig) -> Self {
        Self {
            config,
            feature_stats: HashMap::new(),
        }
    }

    /// リクエストデータから特徴量を抽出
    pub fn extract(&self, request: &RequestData) -> Result<FeatureVector, McpError> {
        debug!("Extracting features from request: {}", request.request_id);

        let mut features = Vec::new();
        let mut feature_names = Vec::new();

        // 1. パス長
        let path_length = request.path.len() as f64;
        features.push(path_length);
        feature_names.push("path_length".to_string());

        // 2. クエリパラメータ数
        let query_param_count = request.query_params.len() as f64;
        features.push(query_param_count);
        feature_names.push("query_param_count".to_string());

        // 3. ヘッダー数
        let header_count = request.headers.len() as f64;
        features.push(header_count);
        feature_names.push("header_count".to_string());

        // 4. ボディサイズ
        let body_size = request.body.as_ref().map(|b| b.len() as f64).unwrap_or(0.0);
        features.push(body_size);
        feature_names.push("body_size".to_string());

        // 5. メソッドエンコーディング（GET=0, POST=1, PUT=2, DELETE=3, その他=4）
        let method_encoded = match request.method.as_str() {
            "GET" => 0.0,
            "POST" => 1.0,
            "PUT" => 2.0,
            "DELETE" => 3.0,
            _ => 4.0,
        };
        features.push(method_encoded);
        feature_names.push("method".to_string());

        // 6. パスの深さ（スラッシュの数）
        let path_depth = request.path.matches('/').count() as f64;
        features.push(path_depth);
        feature_names.push("path_depth".to_string());

        // 7. 特殊文字の数（SQL Injectionなどの指標）
        let special_chars_count = self.count_special_chars(&request.path);
        features.push(special_chars_count);
        feature_names.push("special_chars".to_string());

        // 8. クエリパラメータの平均長
        let avg_query_param_length = if !request.query_params.is_empty() {
            request
                .query_params
                .values()
                .map(|v| v.len())
                .sum::<usize>() as f64
                / request.query_params.len() as f64
        } else {
            0.0
        };
        features.push(avg_query_param_length);
        feature_names.push("avg_query_param_length".to_string());

        // 9. 時刻特徴（時）
        let hour_of_day = request.timestamp.hour() as f64;
        features.push(hour_of_day);
        feature_names.push("hour_of_day".to_string());

        // 10. 曜日
        let day_of_week = request.timestamp.weekday().num_days_from_monday() as f64;
        features.push(day_of_week);
        feature_names.push("day_of_week".to_string());

        // 11. パスのエントロピー（情報量）
        let path_entropy = self.calculate_entropy(&request.path);
        features.push(path_entropy);
        feature_names.push("path_entropy".to_string());

        // 12. 数字の割合
        let digit_ratio = self.calculate_digit_ratio(&request.path);
        features.push(digit_ratio);
        feature_names.push("digit_ratio".to_string());

        // 13. 大文字の割合
        let uppercase_ratio = self.calculate_uppercase_ratio(&request.path);
        features.push(uppercase_ratio);
        feature_names.push("uppercase_ratio".to_string());

        // 14. User-Agent長
        let user_agent_length = request
            .headers
            .get("User-Agent")
            .map(|ua| ua.len() as f64)
            .unwrap_or(0.0);
        features.push(user_agent_length);
        feature_names.push("user_agent_length".to_string());

        // 15. Refererの有無
        let has_referer = request.headers.contains_key("Referer") as i32 as f64;
        features.push(has_referer);
        feature_names.push("has_referer".to_string());

        // 16. Cookieの有無
        let has_cookie = request.headers.contains_key("Cookie") as i32 as f64;
        features.push(has_cookie);
        feature_names.push("has_cookie".to_string());

        // 17. SQLキーワードの数
        let sql_keywords = self.count_sql_keywords(&request.path);
        features.push(sql_keywords);
        feature_names.push("sql_keywords".to_string());

        // 18. XSSパターンの数
        let xss_patterns = self.count_xss_patterns(&request.path);
        features.push(xss_patterns);
        feature_names.push("xss_patterns".to_string());

        // 19. パストラバーサルパターンの数
        let path_traversal = self.count_path_traversal_patterns(&request.path);
        features.push(path_traversal);
        feature_names.push("path_traversal".to_string());

        // 20. Base64エンコードの可能性
        let base64_likelihood = self.calculate_base64_likelihood(&request.path);
        features.push(base64_likelihood);
        feature_names.push("base64_likelihood".to_string());

        // 特徴量を正規化
        let normalized_features = self.normalize_features(&features);

        Ok(FeatureVector {
            features: normalized_features,
            feature_names,
            request_id: request.request_id.clone(),
            timestamp: Utc::now(),
        })
    }

    /// 特徴量を正規化（Z-score normalization）
    pub fn normalize_features(&self, features: &[f64]) -> Vec<f64> {
        features
            .iter()
            .enumerate()
            .map(|(i, &value)| {
                if let Some(stats) = self.feature_stats.get(&i.to_string()) {
                    if stats.std_dev > 0.0 {
                        (value - stats.mean) / stats.std_dev
                    } else {
                        0.0
                    }
                } else {
                    // 統計情報がない場合はMin-Max正規化
                    value / 100.0
                }
            })
            .collect()
    }

    /// 特殊文字の数をカウント
    fn count_special_chars(&self, text: &str) -> f64 {
        text.chars()
            .filter(|c| !c.is_alphanumeric() && *c != '/' && *c != '-' && *c != '_')
            .count() as f64
    }

    /// エントロピーを計算
    fn calculate_entropy(&self, text: &str) -> f64 {
        if text.is_empty() {
            return 0.0;
        }

        let mut char_counts: HashMap<char, usize> = HashMap::new();
        for c in text.chars() {
            *char_counts.entry(c).or_insert(0) += 1;
        }

        let len = text.len() as f64;
        char_counts
            .values()
            .map(|&count| {
                let p = count as f64 / len;
                -p * p.log2()
            })
            .sum()
    }

    /// 数字の割合を計算
    fn calculate_digit_ratio(&self, text: &str) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        text.chars().filter(|c| c.is_numeric()).count() as f64 / text.len() as f64
    }

    /// 大文字の割合を計算
    fn calculate_uppercase_ratio(&self, text: &str) -> f64 {
        if text.is_empty() {
            return 0.0;
        }
        text.chars().filter(|c| c.is_uppercase()).count() as f64 / text.len() as f64
    }

    /// SQLキーワードの数をカウント
    fn count_sql_keywords(&self, text: &str) -> f64 {
        let keywords = [
            "select", "union", "insert", "update", "delete", "drop", "create", "alter", "exec",
            "execute",
        ];
        let text_lower = text.to_lowercase();
        keywords
            .iter()
            .filter(|&&keyword| text_lower.contains(keyword))
            .count() as f64
    }

    /// XSSパターンの数をカウント
    fn count_xss_patterns(&self, text: &str) -> f64 {
        let patterns = ["<script", "javascript:", "onerror=", "onload=", "alert("];
        let text_lower = text.to_lowercase();
        patterns
            .iter()
            .filter(|&&pattern| text_lower.contains(pattern))
            .count() as f64
    }

    /// パストラバーサルパターンの数をカウント
    fn count_path_traversal_patterns(&self, text: &str) -> f64 {
        let patterns = ["../", "..\\", "%2e%2e", "....//"];
        patterns
            .iter()
            .filter(|&&pattern| text.contains(pattern))
            .count() as f64
    }

    /// Base64エンコードの可能性を計算
    fn calculate_base64_likelihood(&self, text: &str) -> f64 {
        if text.len() < 4 {
            return 0.0;
        }

        // Base64文字の割合
        let base64_chars = text
            .chars()
            .filter(|c| c.is_alphanumeric() || *c == '+' || *c == '/' || *c == '=')
            .count();

        let ratio = base64_chars as f64 / text.len() as f64;

        // 長さが4の倍数かチェック
        let length_check = if text.len().is_multiple_of(4) {
            0.3
        } else {
            0.0
        };

        (ratio * 0.7 + length_check).min(1.0)
    }

    /// 統計情報を更新（学習時に使用）
    pub fn update_stats(&mut self, samples: &[FeatureVector]) {
        if samples.is_empty() {
            return;
        }

        let feature_count = samples[0].features.len();

        for i in 0..feature_count {
            let values: Vec<f64> = samples.iter().map(|s| s.features[i]).collect();

            let mean = values.iter().sum::<f64>() / values.len() as f64;
            let variance =
                values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / values.len() as f64;
            let std_dev = variance.sqrt();
            let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
            let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

            self.feature_stats.insert(
                i.to_string(),
                FeatureStats {
                    mean,
                    std_dev,
                    min,
                    max,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_request() -> RequestData {
        RequestData {
            request_id: "test-123".to_string(),
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: Some("192.168.1.1".parse().unwrap()),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_feature_extraction() {
        let extractor = FeatureExtractor::new(MLConfig::default());
        let request = create_test_request();

        let result = extractor.extract(&request);
        assert!(result.is_ok());

        let features = result.unwrap();
        assert_eq!(features.features.len(), 20);
        assert_eq!(features.feature_names.len(), 20);
    }

    #[test]
    fn test_special_chars_count() {
        let extractor = FeatureExtractor::new(MLConfig::default());
        assert_eq!(extractor.count_special_chars("hello"), 0.0);
        assert_eq!(extractor.count_special_chars("hello!@#"), 3.0);
    }

    #[test]
    fn test_entropy_calculation() {
        let extractor = FeatureExtractor::new(MLConfig::default());
        let entropy = extractor.calculate_entropy("aaaa");
        assert!(entropy < 0.1); // 低エントロピー

        let entropy = extractor.calculate_entropy("abcdefgh");
        assert!(entropy > 2.0); // 高エントロピー
    }

    #[test]
    fn test_sql_keyword_detection() {
        let extractor = FeatureExtractor::new(MLConfig::default());
        assert_eq!(extractor.count_sql_keywords("/api/users"), 0.0);
        assert!(extractor.count_sql_keywords("/api?query=SELECT * FROM users") > 0.0);
    }
}
