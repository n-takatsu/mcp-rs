use crate::security::mfa::{MfaConfig, MfaError};
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand::{thread_rng, Rng};
use serde::{Deserialize, Serialize};

/// Configuration for backup codes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCodeConfig {
    /// Whether backup codes are enabled
    pub enabled: bool,
    /// Number of backup codes to generate
    pub count: usize,
    /// Length of each backup code
    pub length: usize,
    /// Whether to use separators (e.g., XXXX-XXXX-XXXX)
    pub use_separators: bool,
}

impl Default for BackupCodeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            count: 10,
            length: 12,
            use_separators: true,
        }
    }
}

/// A single backup code with its hash
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupCode {
    /// Argon2id hash of the code
    pub hash: String,
    /// Whether the code has been used
    pub used: bool,
    /// When the code was created (Unix timestamp)
    pub created_at: u64,
    /// When the code was used (Unix timestamp, if used)
    pub used_at: Option<u64>,
}

/// Manager for backup codes
#[derive(Debug)]
pub struct BackupCodeManager {
    config: BackupCodeConfig,
}

impl BackupCodeManager {
    /// Create a new backup code manager
    pub fn new(config: BackupCodeConfig) -> Self {
        Self { config }
    }

    /// Generate a set of backup codes
    ///
    /// Returns a tuple of (plaintext_codes, hashed_codes)
    pub fn generate(&self) -> Result<(Vec<String>, Vec<BackupCode>), MfaError> {
        if !self.config.enabled {
            return Err(MfaError::NotConfigured);
        }

        let mut plaintext_codes = Vec::with_capacity(self.config.count);
        let mut hashed_codes = Vec::with_capacity(self.config.count);

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|e| MfaError::ConfigError(e.to_string()))?
            .as_secs();

        for _ in 0..self.config.count {
            let code = self.generate_code();
            // Hash the normalized code (without separators)
            let normalized_code = code.replace('-', "");
            let hash = self.hash_code(&normalized_code)?;

            plaintext_codes.push(code);
            hashed_codes.push(BackupCode {
                hash,
                used: false,
                created_at: now,
                used_at: None,
            });
        }

        Ok((plaintext_codes, hashed_codes))
    }

    /// Verify a backup code against stored hashes
    ///
    /// Returns the index of the matched code if found and unused
    pub fn verify(&self, code: &str, stored_codes: &mut [BackupCode]) -> Result<usize, MfaError> {
        if !self.config.enabled {
            return Err(MfaError::NotConfigured);
        }

        // Normalize the code (remove separators if configured)
        let normalized_code = if self.config.use_separators {
            code.replace('-', "")
        } else {
            code.to_string()
        };

        // Check against all stored codes
        for (index, backup_code) in stored_codes.iter_mut().enumerate() {
            if backup_code.used {
                continue;
            }

            // Verify the hash
            match self.verify_code(&normalized_code, &backup_code.hash) {
                Ok(true) => {
                    // Mark as used
                    backup_code.used = true;
                    backup_code.used_at = Some(
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_err(|e| MfaError::ConfigError(e.to_string()))?
                            .as_secs(),
                    );
                    return Ok(index);
                }
                Ok(false) => continue,
                Err(_) => continue, // Skip codes with hash errors
            }
        }

        Err(MfaError::InvalidCode)
    }

    /// Get count of remaining unused codes
    pub fn remaining_count(&self, stored_codes: &[BackupCode]) -> usize {
        stored_codes.iter().filter(|code| !code.used).count()
    }

    /// Check if codes should be regenerated (e.g., low count)
    pub fn should_regenerate(&self, stored_codes: &[BackupCode]) -> bool {
        let remaining = self.remaining_count(stored_codes);
        remaining < 3 // Warn when less than 3 codes remain
    }

    /// Generate a single backup code
    fn generate_code(&self) -> String {
        let mut rng = thread_rng();
        let chars: Vec<char> = "ABCDEFGHJKLMNPQRSTUVWXYZ23456789"
            .chars()
            .collect();

        let code: String = (0..self.config.length)
            .map(|_| chars[rng.gen_range(0..chars.len())])
            .collect();

        if self.config.use_separators {
            // Insert separators every 4 characters
            self.format_with_separators(&code)
        } else {
            code
        }
    }

    /// Format code with separators (XXXX-XXXX-XXXX)
    fn format_with_separators(&self, code: &str) -> String {
        code.chars()
            .collect::<Vec<char>>()
            .chunks(4)
            .map(|chunk| chunk.iter().collect::<String>())
            .collect::<Vec<String>>()
            .join("-")
    }

    /// Hash a backup code using Argon2id
    fn hash_code(&self, code: &str) -> Result<String, MfaError> {
        let salt = SaltString::generate(&mut thread_rng());
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(code.as_bytes(), &salt)
            .map_err(|e| MfaError::CryptoError(e.to_string()))?;

        Ok(password_hash.to_string())
    }

    /// Verify a code against its hash
    fn verify_code(&self, code: &str, hash: &str) -> Result<bool, MfaError> {
        let parsed_hash =
            PasswordHash::new(hash).map_err(|e| MfaError::CryptoError(e.to_string()))?;

        let argon2 = Argon2::default();

        Ok(argon2
            .verify_password(code.as_bytes(), &parsed_hash)
            .is_ok())
    }

    /// Get configuration
    pub fn config(&self) -> &BackupCodeConfig {
        &self.config
    }
}

#[cfg(all(test, feature = "mfa"))]
mod tests {
    use super::*;

    #[test]
    fn test_backup_code_generation() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config.clone());

        let result = manager.generate();
        assert!(result.is_ok());

        let (plaintext, hashed) = result.unwrap();
        assert_eq!(plaintext.len(), config.count);
        assert_eq!(hashed.len(), config.count);

        // Check code format
        for code in &plaintext {
            if config.use_separators {
                assert!(code.contains('-'));
            }
            let clean_code = code.replace('-', "");
            assert_eq!(clean_code.len(), config.length);
        }
    }

    #[test]
    fn test_backup_code_verification() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config);

        let (plaintext, mut hashed) = manager.generate().unwrap();

        // Verify first code
        let result = manager.verify(&plaintext[0], &mut hashed);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);

        // Code should now be marked as used
        assert!(hashed[0].used);
        assert!(hashed[0].used_at.is_some());

        // Verifying again should fail
        let result = manager.verify(&plaintext[0], &mut hashed);
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_code_without_separators() {
        let config = BackupCodeConfig {
            use_separators: false,
            ..Default::default()
        };
        let manager = BackupCodeManager::new(config);

        let (plaintext, mut hashed) = manager.generate().unwrap();

        // Verify a code
        let result = manager.verify(&plaintext[0], &mut hashed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_backup_code_normalization() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config);

        let (plaintext, mut hashed) = manager.generate().unwrap();

        // Original code has separators
        let code_with_sep = &plaintext[0];

        // Both should verify successfully
        let result1 = manager.verify(code_with_sep, &mut hashed);
        assert!(result1.is_ok());

        // Generate new codes for second test
        let (plaintext2, mut hashed2) = manager.generate().unwrap();
        
        // Try to use code without separators on a different set
        let result2 = manager.verify(&plaintext2[0].replace('-', ""), &mut hashed2);
        // This should succeed because it's a valid code from hashed2
        assert!(result2.is_ok());
        
        // Try to use an invalid code
        let result3 = manager.verify("INVALID-CODE", &mut hashed2);
        // This should fail
        assert!(result3.is_err());
    }

    #[test]
    fn test_remaining_count() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config.clone());

        let (plaintext, mut hashed) = manager.generate().unwrap();

        assert_eq!(manager.remaining_count(&hashed), config.count);

        // Use one code
        manager.verify(&plaintext[0], &mut hashed).unwrap();
        assert_eq!(manager.remaining_count(&hashed), config.count - 1);

        // Use another code
        manager.verify(&plaintext[1], &mut hashed).unwrap();
        assert_eq!(manager.remaining_count(&hashed), config.count - 2);
    }

    #[test]
    fn test_should_regenerate() {
        let config = BackupCodeConfig {
            count: 5,
            ..Default::default()
        };
        let manager = BackupCodeManager::new(config);

        let (plaintext, mut hashed) = manager.generate().unwrap();

        // Should not regenerate with 5 codes
        assert!(!manager.should_regenerate(&hashed));

        // Use codes until only 2 remain
        manager.verify(&plaintext[0], &mut hashed).unwrap();
        manager.verify(&plaintext[1], &mut hashed).unwrap();
        manager.verify(&plaintext[2], &mut hashed).unwrap();

        // Should regenerate with 2 codes
        assert!(manager.should_regenerate(&hashed));
    }

    #[test]
    fn test_invalid_code() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config);

        let (_, mut hashed) = manager.generate().unwrap();

        let result = manager.verify("INVALID-CODE-1234", &mut hashed);
        assert!(result.is_err());
    }

    #[test]
    fn test_code_uniqueness() {
        let config = BackupCodeConfig::default();
        let manager = BackupCodeManager::new(config);

        let (plaintext, _) = manager.generate().unwrap();

        // Check that all codes are unique
        for i in 0..plaintext.len() {
            for j in (i + 1)..plaintext.len() {
                assert_ne!(plaintext[i], plaintext[j]);
            }
        }
    }

    #[test]
    fn test_disabled_backup_codes() {
        let config = BackupCodeConfig {
            enabled: false,
            ..Default::default()
        };
        let manager = BackupCodeManager::new(config);

        // Generation should fail when disabled
        let result = manager.generate();
        assert!(result.is_err());

        // Verification should fail when disabled
        let mut dummy_codes = vec![];
        let result = manager.verify("XXXX-XXXX-XXXX", &mut dummy_codes);
        assert!(result.is_err());
    }
}
