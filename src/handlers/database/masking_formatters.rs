//! マスキングフォーマッタ
//!
//! 各マスキングタイプの実装を提供します。

use crate::handlers::database::masking_rules::{HashAlgorithm, MaskingType};
use anyhow::{Context, Result};
use sha2::{Digest, Sha256, Sha512};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// マスキングフォーマッタ
pub struct MaskingFormatter {
    /// トークンマップ (値 -> トークン)
    token_map: Arc<RwLock<HashMap<String, String>>>,
    /// トークンカウンター
    token_counter: Arc<RwLock<u64>>,
}

impl MaskingFormatter {
    /// 新しいフォーマッタを作成
    pub fn new() -> Self {
        Self {
            token_map: Arc::new(RwLock::new(HashMap::new())),
            token_counter: Arc::new(RwLock::new(0)),
        }
    }

    /// 値をマスキング
    pub async fn mask(&self, value: &str, masking_type: &MaskingType) -> Result<String> {
        match masking_type {
            MaskingType::FullMask => Ok(self.full_mask(value)),
            MaskingType::PartialMask {
                prefix_visible,
                suffix_visible,
            } => Ok(self.partial_mask(value, *prefix_visible, *suffix_visible)),
            MaskingType::HashMask {
                algorithm,
                display_length,
            } => Ok(self.hash_mask(value, *algorithm, *display_length)),
            MaskingType::FormatPreserving {
                format_pattern,
                mask_char,
            } => self.format_preserving_mask(value, format_pattern, *mask_char),
            MaskingType::TokenMask { prefix } => self.token_mask(value, prefix).await,
        }
    }

    /// 完全マスク
    fn full_mask(&self, value: &str) -> String {
        "*".repeat(value.len().min(10))
    }

    /// 部分マスク
    fn partial_mask(&self, value: &str, prefix_visible: usize, suffix_visible: usize) -> String {
        let len = value.len();
        
        if len <= prefix_visible + suffix_visible {
            return "*".repeat(len);
        }

        let prefix = &value[..prefix_visible];
        let suffix = &value[len - suffix_visible..];
        let mask_len = len - prefix_visible - suffix_visible;
        
        format!("{}{}{}", prefix, "*".repeat(mask_len), suffix)
    }

    /// ハッシュマスク
    fn hash_mask(&self, value: &str, algorithm: HashAlgorithm, display_length: usize) -> String {
        let hash = match algorithm {
            HashAlgorithm::Sha256 => {
                let mut hasher = Sha256::new();
                hasher.update(value.as_bytes());
                format!("{:x}", hasher.finalize())
            }
            HashAlgorithm::Sha512 => {
                let mut hasher = Sha512::new();
                hasher.update(value.as_bytes());
                format!("{:x}", hasher.finalize())
            }
        };

        hash[..display_length.min(hash.len())].to_string()
    }

    /// 形式保持マスク
    fn format_preserving_mask(
        &self,
        value: &str,
        format_pattern: &str,
        mask_char: char,
    ) -> Result<String> {
        // フォーマットパターンを解析
        // 例: "###-##-####" -> "123-45-6789" -> "123-45-****"
        
        let mut result = String::new();
        let mut value_chars = value.chars();
        
        for pattern_char in format_pattern.chars() {
            if pattern_char == '#' {
                // 数字をマスク
                if let Some(c) = value_chars.next() {
                    if c.is_numeric() {
                        result.push(mask_char);
                    } else {
                        result.push(c);
                    }
                } else {
                    result.push(mask_char);
                }
            } else {
                // フォーマット文字をそのまま保持
                result.push(pattern_char);
                // 対応する文字をスキップ
                if let Some(c) = value_chars.next() {
                    if c != pattern_char {
                        // フォーマット不一致の場合は元の値を使用
                        return Ok(self.full_mask(value));
                    }
                }
            }
        }

        Ok(result)
    }

    /// トークンマスク
    async fn token_mask(&self, value: &str, prefix: &str) -> Result<String> {
        // 既存のトークンをチェック
        {
            let token_map = self.token_map.read().await;
            if let Some(token) = token_map.get(value) {
                return Ok(token.clone());
            }
        }

        // 新しいトークンを生成
        let mut counter = self.token_counter.write().await;
        *counter += 1;
        let token = format!("{}_{:08}", prefix, *counter);

        // トークンマップに保存
        let mut token_map = self.token_map.write().await;
        token_map.insert(value.to_string(), token.clone());

        Ok(token)
    }

    /// トークンを元の値に戻す (監査/デバッグ用)
    pub async fn unmask_token(&self, token: &str) -> Option<String> {
        let token_map = self.token_map.read().await;
        token_map.iter()
            .find(|(_, t)| *t == token)
            .map(|(v, _)| v.clone())
    }
}

impl Default for MaskingFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// 事前定義されたフォーマッタ
pub struct PredefinedFormatters;

impl PredefinedFormatters {
    /// クレジットカード番号用フォーマッタ
    pub fn credit_card() -> MaskingType {
        MaskingType::PartialMask {
            prefix_visible: 0,
            suffix_visible: 4,
        }
    }

    /// メールアドレス用フォーマッタ
    pub fn email() -> MaskingType {
        MaskingType::PartialMask {
            prefix_visible: 1,
            suffix_visible: 0,
        }
    }

    /// 電話番号用フォーマッタ
    pub fn phone_number() -> MaskingType {
        MaskingType::FormatPreserving {
            format_pattern: "###-####-####".to_string(),
            mask_char: '*',
        }
    }

    /// 社会保障番号用フォーマッタ (SSN)
    pub fn ssn() -> MaskingType {
        MaskingType::FormatPreserving {
            format_pattern: "###-##-####".to_string(),
            mask_char: '*',
        }
    }

    /// パスワード用フォーマッタ
    pub fn password() -> MaskingType {
        MaskingType::FullMask
    }

    /// IPアドレス用フォーマッタ
    pub fn ip_address() -> MaskingType {
        MaskingType::PartialMask {
            prefix_visible: 7,
            suffix_visible: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_full_mask() {
        let formatter = MaskingFormatter::new();
        let result = formatter.mask("secret123", &MaskingType::FullMask).await.unwrap();
        assert_eq!(result, "*********");
    }

    #[tokio::test]
    async fn test_partial_mask() {
        let formatter = MaskingFormatter::new();
        let result = formatter
            .mask(
                "1234567890",
                &MaskingType::PartialMask {
                    prefix_visible: 2,
                    suffix_visible: 2,
                },
            )
            .await
            .unwrap();
        assert_eq!(result, "12******90");
    }

    #[tokio::test]
    async fn test_hash_mask() {
        let formatter = MaskingFormatter::new();
        let result = formatter
            .mask(
                "test@example.com",
                &MaskingType::HashMask {
                    algorithm: HashAlgorithm::Sha256,
                    display_length: 8,
                },
            )
            .await
            .unwrap();
        assert_eq!(result.len(), 8);
        assert!(result.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[tokio::test]
    async fn test_format_preserving_mask() {
        let formatter = MaskingFormatter::new();
        let result = formatter
            .mask(
                "123-45-6789",
                &MaskingType::FormatPreserving {
                    format_pattern: "###-##-####".to_string(),
                    mask_char: '*',
                },
            )
            .await
            .unwrap();
        assert_eq!(result, "***-**-****");
    }

    #[tokio::test]
    async fn test_token_mask() {
        let formatter = MaskingFormatter::new();
        let result1 = formatter
            .mask(
                "sensitive_data",
                &MaskingType::TokenMask {
                    prefix: "TOKEN".to_string(),
                },
            )
            .await
            .unwrap();
        
        let result2 = formatter
            .mask(
                "sensitive_data",
                &MaskingType::TokenMask {
                    prefix: "TOKEN".to_string(),
                },
            )
            .await
            .unwrap();

        // 同じ値は同じトークン
        assert_eq!(result1, result2);
        assert!(result1.starts_with("TOKEN_"));
    }

    #[tokio::test]
    async fn test_predefined_formatters() {
        let formatter = MaskingFormatter::new();

        // クレジットカード
        let cc = formatter
            .mask("1234-5678-9012-3456", &PredefinedFormatters::credit_card())
            .await
            .unwrap();
        assert!(cc.ends_with("3456"));

        // メール
        let email = formatter
            .mask("user@example.com", &PredefinedFormatters::email())
            .await
            .unwrap();
        assert!(email.starts_with('u'));
    }
}
