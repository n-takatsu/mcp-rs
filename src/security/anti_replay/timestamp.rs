//! Timestamp Validation
//!
//! Validates request timestamps to prevent replay attacks and
//! detect time-based anomalies.

use super::types::ReplayError;
use chrono::{DateTime, Utc};
use std::time::Duration;
use tracing::{debug, warn};

/// Timestamp Validator
pub struct TimestampValidator {
    /// Acceptable time window (±seconds)
    time_window: Duration,

    /// Additional drift tolerance for client clock skew
    drift_tolerance: Duration,
}

impl TimestampValidator {
    /// Create a new TimestampValidator
    pub fn new(time_window_secs: u64) -> Self {
        Self {
            time_window: Duration::from_secs(time_window_secs),
            drift_tolerance: Duration::from_secs(5), // Additional 5 seconds for clock drift
        }
    }

    /// Parse and validate a timestamp string
    pub fn validate_timestamp(&self, timestamp_str: &str) -> Result<DateTime<Utc>, ReplayError> {
        // Parse timestamp (RFC3339 format)
        let timestamp = DateTime::parse_from_rfc3339(timestamp_str)
            .map_err(|e| ReplayError::InvalidTimestamp(format!("Parse error: {}", e)))?
            .with_timezone(&Utc);

        // Validate the timestamp is within acceptable window
        self.check_window(timestamp)?;

        debug!("Timestamp validated: {}", timestamp);

        Ok(timestamp)
    }

    /// Check if timestamp is within acceptable window
    pub fn check_window(&self, req_time: DateTime<Utc>) -> Result<(), ReplayError> {
        let now = Utc::now();
        let total_window = self.time_window + self.drift_tolerance;

        // Check if request is from the future
        if req_time > now + total_window {
            warn!(
                "Future timestamp detected: {} (now: {}, window: {:?})",
                req_time, now, total_window
            );
            return Err(ReplayError::FutureTimestamp);
        }

        // Check if request is too old
        if req_time < now - total_window {
            warn!(
                "Expired timestamp detected: {} (now: {}, window: {:?})",
                req_time, now, total_window
            );
            return Err(ReplayError::ExpiredTimestamp);
        }

        let diff = (now - req_time).num_seconds().abs();
        debug!(
            "Timestamp within window: {} (diff: {}s, allowed: {:?})",
            req_time, diff, total_window
        );

        Ok(())
    }

    /// Check if timestamp is in the future
    pub fn is_future_timestamp(&self, timestamp: DateTime<Utc>) -> bool {
        timestamp > Utc::now()
    }

    /// Check if timestamp is expired
    pub fn is_expired(&self, timestamp: DateTime<Utc>) -> bool {
        let now = Utc::now();
        let total_window = self.time_window + self.drift_tolerance;
        timestamp < now - total_window
    }

    /// Get the acceptable time range
    pub fn get_acceptable_range(&self) -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        let total_window = self.time_window + self.drift_tolerance;
        let earliest = now - total_window;
        let latest = now + total_window;
        (earliest, latest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as ChronoDuration;

    #[test]
    fn test_validate_current_timestamp() {
        let validator = TimestampValidator::new(30);
        let now = Utc::now();
        let timestamp_str = now.to_rfc3339();

        let result = validator.validate_timestamp(&timestamp_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_recent_timestamp() {
        let validator = TimestampValidator::new(30);
        let timestamp = Utc::now() - ChronoDuration::seconds(20);
        let timestamp_str = timestamp.to_rfc3339();

        let result = validator.validate_timestamp(&timestamp_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_reject_future_timestamp() {
        let validator = TimestampValidator::new(30);
        let future = Utc::now() + ChronoDuration::seconds(100);
        let timestamp_str = future.to_rfc3339();

        let result = validator.validate_timestamp(&timestamp_str);
        assert!(matches!(result, Err(ReplayError::FutureTimestamp)));
    }

    #[test]
    fn test_reject_expired_timestamp() {
        let validator = TimestampValidator::new(30);
        let past = Utc::now() - ChronoDuration::seconds(100);
        let timestamp_str = past.to_rfc3339();

        let result = validator.validate_timestamp(&timestamp_str);
        assert!(matches!(result, Err(ReplayError::ExpiredTimestamp)));
    }

    #[test]
    fn test_invalid_timestamp_format() {
        let validator = TimestampValidator::new(30);

        let result = validator.validate_timestamp("invalid-timestamp");
        assert!(matches!(result, Err(ReplayError::InvalidTimestamp(_))));
    }

    #[test]
    fn test_is_future_timestamp() {
        let validator = TimestampValidator::new(30);

        let future = Utc::now() + ChronoDuration::seconds(10);
        assert!(validator.is_future_timestamp(future));

        let past = Utc::now() - ChronoDuration::seconds(10);
        assert!(!validator.is_future_timestamp(past));
    }

    #[test]
    fn test_is_expired() {
        let validator = TimestampValidator::new(30);

        let old = Utc::now() - ChronoDuration::seconds(100);
        assert!(validator.is_expired(old));

        let recent = Utc::now() - ChronoDuration::seconds(10);
        assert!(!validator.is_expired(recent));
    }

    #[test]
    fn test_acceptable_range() {
        let validator = TimestampValidator::new(30);
        let (earliest, latest) = validator.get_acceptable_range();

        let now = Utc::now();
        assert!(earliest < now);
        assert!(latest > now);

        // Range should be approximately 70 seconds (30 + 5 drift on each side)
        let range_secs = (latest - earliest).num_seconds();
        assert!((70..=72).contains(&range_secs));
    }

    #[test]
    fn test_drift_tolerance() {
        let validator = TimestampValidator::new(30);

        // Just outside the main window but within drift tolerance
        let timestamp = Utc::now() - ChronoDuration::seconds(33);
        let result = validator.check_window(timestamp);
        assert!(result.is_ok()); // Should pass due to drift tolerance
    }
}
