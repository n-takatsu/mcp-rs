use crate::security::mfa::MfaError;
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Configuration for SMS authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsConfig {
    /// Whether SMS authentication is enabled
    pub enabled: bool,
    /// SMS code length (typically 6 digits)
    pub code_length: usize,
    /// Code expiration time in seconds
    pub expiration_seconds: u64,
    /// Maximum verification attempts before code invalidation
    pub max_attempts: u32,
    /// SMS provider configuration
    pub provider: SmsProviderConfig,
}

impl Default for SmsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            code_length: 6,
            expiration_seconds: 300, // 5 minutes
            max_attempts: 3,
            provider: SmsProviderConfig::Mock,
        }
    }
}

/// SMS provider configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SmsProviderConfig {
    /// Mock provider for testing (logs to console)
    Mock,
    /// Twilio SMS provider
    Twilio {
        account_sid: String,
        auth_token: String,
        from_number: String,
    },
    /// AWS SNS provider
    AwsSns {
        region: String,
        access_key_id: String,
        secret_access_key: String,
    },
    /// Custom HTTP provider
    Custom {
        endpoint: String,
        api_key: String,
        method: String,
    },
}

/// SMS verification code with metadata
#[derive(Debug, Clone)]
struct SmsCode {
    /// The verification code
    code: String,
    /// When the code was created (Unix timestamp)
    created_at: u64,
    /// Number of verification attempts
    attempts: u32,
    /// Whether the code has been used
    used: bool,
}

/// SMS authenticator
pub struct SmsAuthenticator {
    config: SmsConfig,
    /// Active codes indexed by phone number
    codes: Arc<RwLock<HashMap<String, SmsCode>>>,
}

impl SmsAuthenticator {
    /// Create a new SMS authenticator
    pub fn new(config: SmsConfig) -> Self {
        Self {
            config,
            codes: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Generate and send a verification code to a phone number
    pub async fn send_code(&self, phone_number: &str) -> Result<(), MfaError> {
        if !self.config.enabled {
            return Err(MfaError::NotConfigured);
        }

        // Validate phone number format
        self.validate_phone_number(phone_number)?;

        // Generate verification code
        let code = self.generate_code();

        // Create code entry
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MfaError::ConfigError(e.to_string()))?
            .as_secs();

        let sms_code = SmsCode {
            code: code.clone(),
            created_at: now,
            attempts: 0,
            used: false,
        };

        // Store code
        {
            let mut codes = self.codes.write().await;
            codes.insert(phone_number.to_string(), sms_code);
        }

        // Send SMS
        self.send_sms(phone_number, &code).await?;

        Ok(())
    }

    /// Verify a code for a phone number
    pub async fn verify_code(&self, phone_number: &str, code: &str) -> Result<bool, MfaError> {
        if !self.config.enabled {
            return Ok(true); // If disabled, allow all
        }

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MfaError::ConfigError(e.to_string()))?
            .as_secs();

        let mut codes = self.codes.write().await;

        // Get the stored code
        let stored_code = codes
            .get_mut(phone_number)
            .ok_or(MfaError::InvalidCode)?;

        // Check if already used
        if stored_code.used {
            return Err(MfaError::InvalidCode);
        }

        // Check if expired
        let age = now - stored_code.created_at;
        if age > self.config.expiration_seconds {
            codes.remove(phone_number);
            return Err(MfaError::CodeExpired);
        }

        // Increment attempt counter
        stored_code.attempts += 1;

        // Check max attempts
        if stored_code.attempts > self.config.max_attempts {
            codes.remove(phone_number);
            return Err(MfaError::TooManyAttempts);
        }

        // Verify the code
        if stored_code.code == code {
            stored_code.used = true;
            Ok(true)
        } else {
            // Don't remove yet, allow remaining attempts
            Ok(false)
        }
    }

    /// Clean up expired codes (should be called periodically)
    pub async fn cleanup_expired(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut codes = self.codes.write().await;
        codes.retain(|_, code| {
            let age = now - code.created_at;
            age <= self.config.expiration_seconds && !code.used
        });
    }

    /// Get count of active codes (for monitoring)
    pub async fn active_codes_count(&self) -> usize {
        self.codes.read().await.len()
    }

    /// Generate a numeric verification code
    fn generate_code(&self) -> String {
        let mut rng = thread_rng();
        let max = 10_u32.pow(self.config.code_length as u32);
        let code = rng.gen_range(0..max);
        format!("{:0width$}", code, width = self.config.code_length)
    }

    /// Validate phone number format (E.164 format)
    fn validate_phone_number(&self, phone: &str) -> Result<(), MfaError> {
        // Basic E.164 validation: +[1-9]\d{1,14}
        if !phone.starts_with('+') {
            return Err(MfaError::ConfigError(
                "Phone number must start with +".to_string(),
            ));
        }

        let digits: String = phone.chars().skip(1).filter(|c| c.is_ascii_digit()).collect();

        if digits.len() < 7 || digits.len() > 15 {
            return Err(MfaError::ConfigError(
                "Invalid phone number length".to_string(),
            ));
        }

        Ok(())
    }

    /// Send SMS via configured provider
    async fn send_sms(&self, phone_number: &str, code: &str) -> Result<(), MfaError> {
        match &self.config.provider {
            SmsProviderConfig::Mock => {
                // Mock provider: just log to console
                println!("[SMS Mock] Sending to {}: Your verification code is {}", phone_number, code);
                Ok(())
            }
            SmsProviderConfig::Twilio { account_sid, auth_token, from_number } => {
                self.send_via_twilio(phone_number, code, account_sid, auth_token, from_number)
                    .await
            }
            SmsProviderConfig::AwsSns { region, access_key_id, secret_access_key } => {
                self.send_via_aws_sns(phone_number, code, region, access_key_id, secret_access_key)
                    .await
            }
            SmsProviderConfig::Custom { endpoint, api_key, method } => {
                self.send_via_custom(phone_number, code, endpoint, api_key, method)
                    .await
            }
        }
    }

    /// Send SMS via Twilio
    async fn send_via_twilio(
        &self,
        _phone_number: &str,
        _code: &str,
        _account_sid: &str,
        _auth_token: &str,
        _from_number: &str,
    ) -> Result<(), MfaError> {
        // Twilio implementation would go here
        // For now, return a placeholder error
        Err(MfaError::ConfigError(
            "Twilio provider not yet implemented".to_string(),
        ))
    }

    /// Send SMS via AWS SNS
    async fn send_via_aws_sns(
        &self,
        _phone_number: &str,
        _code: &str,
        _region: &str,
        _access_key_id: &str,
        _secret_access_key: &str,
    ) -> Result<(), MfaError> {
        // AWS SNS implementation would go here
        // For now, return a placeholder error
        Err(MfaError::ConfigError(
            "AWS SNS provider not yet implemented".to_string(),
        ))
    }

    /// Send SMS via custom HTTP endpoint
    async fn send_via_custom(
        &self,
        _phone_number: &str,
        _code: &str,
        _endpoint: &str,
        _api_key: &str,
        _method: &str,
    ) -> Result<(), MfaError> {
        // Custom HTTP implementation would go here
        // For now, return a placeholder error
        Err(MfaError::ConfigError(
            "Custom provider not yet implemented".to_string(),
        ))
    }

    /// Get configuration
    pub fn config(&self) -> &SmsConfig {
        &self.config
    }
}

#[cfg(all(test, feature = "mfa"))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sms_code_generation() {
        let config = SmsConfig::default();
        let authenticator = SmsAuthenticator::new(config.clone());

        let result = authenticator.send_code("+1234567890").await;
        assert!(result.is_ok());

        // Check that code was stored
        assert_eq!(authenticator.active_codes_count().await, 1);
    }

    #[tokio::test]
    async fn test_sms_code_verification() {
        let config = SmsConfig::default();
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        authenticator.send_code(phone).await.unwrap();

        // Get the generated code (in production this would be sent via SMS)
        let code = {
            let codes = authenticator.codes.read().await;
            codes.get(phone).unwrap().code.clone()
        };

        // Verify with correct code
        let result = authenticator.verify_code(phone, &code).await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_sms_code_expiration() {
        let config = SmsConfig {
            expiration_seconds: 1, // 1 second expiration
            ..Default::default()
        };
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        authenticator.send_code(phone).await.unwrap();

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Try to verify
        let result = authenticator.verify_code(phone, "123456").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MfaError::CodeExpired));
    }

    #[tokio::test]
    async fn test_sms_max_attempts() {
        let config = SmsConfig {
            max_attempts: 2,
            ..Default::default()
        };
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        authenticator.send_code(phone).await.unwrap();

        // Try with wrong code twice
        let _ = authenticator.verify_code(phone, "000000").await;
        let _ = authenticator.verify_code(phone, "111111").await;

        // Third attempt should fail with TooManyAttempts
        let result = authenticator.verify_code(phone, "222222").await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), MfaError::TooManyAttempts));
    }

    #[tokio::test]
    async fn test_sms_one_time_use() {
        let config = SmsConfig::default();
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        authenticator.send_code(phone).await.unwrap();

        let code = {
            let codes = authenticator.codes.read().await;
            codes.get(phone).unwrap().code.clone()
        };

        // Verify once
        let result = authenticator.verify_code(phone, &code).await;
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Try to verify again
        let result = authenticator.verify_code(phone, &code).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_sms_cleanup_expired() {
        let config = SmsConfig {
            expiration_seconds: 1,
            ..Default::default()
        };
        let authenticator = SmsAuthenticator::new(config);

        // Send multiple codes
        authenticator.send_code("+1234567890").await.unwrap();
        authenticator.send_code("+0987654321").await.unwrap();

        assert_eq!(authenticator.active_codes_count().await, 2);

        // Wait for expiration
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;

        // Cleanup
        authenticator.cleanup_expired().await;

        assert_eq!(authenticator.active_codes_count().await, 0);
    }

    #[tokio::test]
    async fn test_sms_phone_validation() {
        let config = SmsConfig::default();
        let authenticator = SmsAuthenticator::new(config);

        // Valid phone numbers
        assert!(authenticator.send_code("+1234567890").await.is_ok());
        assert!(authenticator.send_code("+12345678901234").await.is_ok());

        // Invalid: no +
        assert!(authenticator.send_code("1234567890").await.is_err());

        // Invalid: too short
        assert!(authenticator.send_code("+123456").await.is_err());

        // Invalid: too long
        assert!(authenticator.send_code("+12345678901234567").await.is_err());
    }

    #[tokio::test]
    async fn test_sms_disabled() {
        let config = SmsConfig {
            enabled: false,
            ..Default::default()
        };
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        
        // Send should fail when disabled
        let result = authenticator.send_code(phone).await;
        assert!(result.is_err());

        // Verify should succeed when disabled (no MFA)
        let result = authenticator.verify_code(phone, "123456").await;
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_sms_code_format() {
        let config = SmsConfig {
            code_length: 8,
            ..Default::default()
        };
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        authenticator.send_code(phone).await.unwrap();

        let code = {
            let codes = authenticator.codes.read().await;
            codes.get(phone).unwrap().code.clone()
        };

        // Check code length
        assert_eq!(code.len(), 8);

        // Check code is all digits
        assert!(code.chars().all(|c| c.is_ascii_digit()));
    }

    #[tokio::test]
    async fn test_sms_wrong_code() {
        let config = SmsConfig::default();
        let authenticator = SmsAuthenticator::new(config);

        let phone = "+1234567890";
        authenticator.send_code(phone).await.unwrap();

        // Verify with wrong code
        let result = authenticator.verify_code(phone, "000000").await;
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should return false, not error

        // Code should still be available for retry
        assert_eq!(authenticator.active_codes_count().await, 1);
    }
}
