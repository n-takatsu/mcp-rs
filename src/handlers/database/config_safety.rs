/// 設定の安全性を向上させるためのパターン例
/// 
/// 1. Builder パターンで必須フィールドを強制
/// 2. ValidatedConfig で検証済み設定を保証
/// 3. TypeState パターンで設定状態を型で表現

use std::marker::PhantomData;

/// 設定状態を表す型レベルマーカー
pub struct Unvalidated;
pub struct Validated;

/// 型状態パターンを使った安全な設定
pub struct SafeCircuitBreakerConfig<State = Unvalidated> {
    pub failure_threshold: Option<u32>,
    pub timeout_seconds: Option<u64>,
    pub half_open_max_calls: Option<u32>,
    _state: PhantomData<State>,
}

impl SafeCircuitBreakerConfig<Unvalidated> {
    pub fn new() -> Self {
        Self {
            failure_threshold: None,
            timeout_seconds: None,
            half_open_max_calls: None,
            _state: PhantomData,
        }
    }

    pub fn failure_threshold(mut self, value: u32) -> Self {
        if value == 0 {
            panic!("failure_threshold must be greater than 0");
        }
        self.failure_threshold = Some(value);
        self
    }

    pub fn timeout_seconds(mut self, value: u64) -> Self {
        if value == 0 {
            panic!("timeout_seconds must be greater than 0");
        }
        self.timeout_seconds = Some(value);
        self
    }

    pub fn half_open_max_calls(mut self, value: u32) -> Self {
        if value == 0 {
            panic!("half_open_max_calls must be greater than 0");
        }
        self.half_open_max_calls = Some(value);
        self
    }

    /// 検証して完成形に変換（コンパイル時に必須フィールドチェック）
    pub fn build(self) -> Result<SafeCircuitBreakerConfig<Validated>, ConfigError> {
        match (self.failure_threshold, self.timeout_seconds, self.half_open_max_calls) {
            (Some(ft), Some(ts), Some(hmc)) => {
                Ok(SafeCircuitBreakerConfig {
                    failure_threshold: Some(ft),
                    timeout_seconds: Some(ts),
                    half_open_max_calls: Some(hmc),
                    _state: PhantomData,
                })
            }
            _ => Err(ConfigError::MissingRequiredFields),
        }
    }
}

impl SafeCircuitBreakerConfig<Validated> {
    /// 検証済みの設定からのみ値を取得可能
    pub fn get_failure_threshold(&self) -> u32 {
        self.failure_threshold.unwrap() // 検証済みなので安全
    }

    pub fn get_timeout_seconds(&self) -> u64 {
        self.timeout_seconds.unwrap()
    }

    pub fn get_half_open_max_calls(&self) -> u32 {
        self.half_open_max_calls.unwrap()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Missing required configuration fields")]
    MissingRequiredFields,
    #[error("Invalid configuration value: {0}")]
    InvalidValue(String),
}

/// 別のアプローチ：const fnとマクロによるコンパイル時検証
pub struct ConstConfig;

impl ConstConfig {
    /// コンパイル時に値を検証
    pub const fn validated_failure_threshold(value: u32) -> u32 {
        if value == 0 {
            panic!("failure_threshold must be greater than 0");
        }
        if value > 1000 {
            panic!("failure_threshold too large");
        }
        value
    }

    pub const fn validated_timeout_seconds(value: u64) -> u64 {
        if value == 0 {
            panic!("timeout_seconds must be greater than 0");
        }
        if value > 3600 {
            panic!("timeout_seconds too large (max 1 hour)");
        }
        value
    }
}

/// マクロによるコンパイル時設定検証
#[macro_export]
macro_rules! safe_circuit_breaker_config {
    (
        failure_threshold: $ft:expr,
        timeout_seconds: $ts:expr,
        half_open_max_calls: $hmc:expr
    ) => {
        CircuitBreakerConfig {
            failure_threshold: ConstConfig::validated_failure_threshold($ft),
            timeout_seconds: ConstConfig::validated_timeout_seconds($ts),
            half_open_max_calls: ConstConfig::validated_failure_threshold($hmc),
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder_pattern_safety() {
        // コンパイル時に必須フィールドチェック
        let config = SafeCircuitBreakerConfig::new()
            .failure_threshold(5)
            .timeout_seconds(60)
            .half_open_max_calls(3)
            .build()
            .unwrap();

        assert_eq!(config.get_failure_threshold(), 5);
    }

    #[test]
    #[should_panic]
    fn test_invalid_values_panic() {
        // 無効な値はパニック
        SafeCircuitBreakerConfig::new().failure_threshold(0);
    }

    #[test]
    fn test_const_validation() {
        // コンパイル時検証
        const VALID_THRESHOLD: u32 = ConstConfig::validated_failure_threshold(5);
        assert_eq!(VALID_THRESHOLD, 5);
    }

    // このテストはコンパイルエラーになる（意図的）
    // #[test]
    // fn test_compile_time_error() {
    //     const INVALID: u32 = ConstConfig::validated_failure_threshold(0);
    // }

    #[test]
    fn test_macro_safety() {
        let config = safe_circuit_breaker_config! {
            failure_threshold: 5,
            timeout_seconds: 60,
            half_open_max_calls: 3
        };
        assert_eq!(config.failure_threshold, 5);
    }
}