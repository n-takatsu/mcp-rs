//! Container Image Security Scanner
//!
//! This module provides container image vulnerability scanning capabilities
//! using Trivy or other security scanning tools.

use crate::error::McpError;
use serde::{Deserialize, Serialize};
use std::process::Command;
use tracing::{debug, error, info, warn};

/// Type of scanner to use
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScannerType {
    /// Aqua Security Trivy scanner
    Trivy,
    /// Anchore Engine scanner
    Anchore,
    /// Clair scanner
    Clair,
}

/// ImageScanner provides container image vulnerability scanning
pub struct ImageScanner {
    scanner_type: ScannerType,
}

impl ImageScanner {
    /// Create a new ImageScanner with the specified scanner type
    pub fn new(scanner_type: ScannerType) -> Self {
        Self { scanner_type }
    }

    /// Scan a container image for vulnerabilities
    pub async fn scan_image(&self, image: &str) -> Result<ScanReport, McpError> {
        info!(image = %image, scanner = ?self.scanner_type, "Starting image scan");

        match self.scanner_type {
            ScannerType::Trivy => self.scan_with_trivy(image).await,
            ScannerType::Anchore => {
                warn!("Anchore scanner not yet implemented, falling back to Trivy");
                self.scan_with_trivy(image).await
            }
            ScannerType::Clair => {
                warn!("Clair scanner not yet implemented, falling back to Trivy");
                self.scan_with_trivy(image).await
            }
        }
    }

    /// Scan image using Trivy
    async fn scan_with_trivy(&self, image: &str) -> Result<ScanReport, McpError> {
        debug!("Scanning image with Trivy: {}", image);

        // Check if Trivy is installed
        let trivy_check = Command::new("trivy").arg("--version").output();

        match trivy_check {
            Ok(output) if output.status.success() => {
                debug!("Trivy found: {}", String::from_utf8_lossy(&output.stdout));
            }
            _ => {
                return Err(McpError::SecurityError(
                    "Trivy not found. Please install Trivy: https://aquasecurity.github.io/trivy/".to_string(),
                ));
            }
        }

        // Run Trivy scan
        let output = Command::new("trivy")
            .args(&[
                "image",
                "--format",
                "json",
                "--severity",
                "HIGH,CRITICAL",
                "--quiet",
                image,
            ])
            .output()
            .map_err(|e| McpError::SecurityError(format!("Failed to run Trivy: {}", e)))?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr);
            error!(image = %image, error = %error_msg, "Trivy scan failed");
            return Err(McpError::SecurityError(format!("Trivy scan failed: {}", error_msg)));
        }

        let scan_result: TrivyScanResult = serde_json::from_slice(&output.stdout)
            .map_err(|e| McpError::SecurityError(format!("Failed to parse Trivy output: {}", e)))?;

        // Convert Trivy results to our ScanReport format
        let vulnerabilities = self.convert_trivy_results(&scan_result);

        let report = ScanReport {
            image: image.to_string(),
            scanner: "Trivy".to_string(),
            scan_time: chrono::Utc::now().to_rfc3339(),
            vulnerabilities,
            summary: self.create_summary(&scan_result),
        };

        info!(
            image = %image,
            total_vulns = report.vulnerabilities.len(),
            "Image scan completed"
        );

        Ok(report)
    }

    /// Convert Trivy results to our vulnerability format
    fn convert_trivy_results(&self, trivy_result: &TrivyScanResult) -> Vec<Vulnerability> {
        let mut vulnerabilities = Vec::new();

        for result in &trivy_result.results {
            if let Some(vulns) = &result.vulnerabilities {
                for vuln in vulns {
                    vulnerabilities.push(Vulnerability {
                        id: vuln.vulnerability_id.clone(),
                        package_name: vuln.pkg_name.clone(),
                        installed_version: vuln.installed_version.clone(),
                        fixed_version: vuln.fixed_version.clone(),
                        severity: vuln.severity.clone(),
                        description: vuln.description.clone().unwrap_or_default(),
                        references: vuln.references.clone().unwrap_or_default(),
                    });
                }
            }
        }

        vulnerabilities
    }

    /// Create a summary from Trivy results
    fn create_summary(&self, trivy_result: &TrivyScanResult) -> ScanSummary {
        let mut critical = 0;
        let mut high = 0;
        let mut medium = 0;
        let mut low = 0;

        for result in &trivy_result.results {
            if let Some(vulns) = &result.vulnerabilities {
                for vuln in vulns {
                    match vuln.severity.to_uppercase().as_str() {
                        "CRITICAL" => critical += 1,
                        "HIGH" => high += 1,
                        "MEDIUM" => medium += 1,
                        "LOW" => low += 1,
                        _ => {}
                    }
                }
            }
        }

        ScanSummary {
            critical,
            high,
            medium,
            low,
            total: critical + high + medium + low,
        }
    }

    /// Check if an image passes the security threshold
    pub async fn check_vulnerabilities(&self, report: &ScanReport) -> Result<bool, McpError> {
        // Fail if there are any CRITICAL vulnerabilities
        if report.summary.critical > 0 {
            warn!(
                critical_count = report.summary.critical,
                "Image has CRITICAL vulnerabilities"
            );
            return Ok(false);
        }

        // Fail if there are more than 5 HIGH vulnerabilities
        if report.summary.high > 5 {
            warn!(
                high_count = report.summary.high,
                "Image has too many HIGH vulnerabilities"
            );
            return Ok(false);
        }

        Ok(true)
    }
}

/// Container image scan report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    /// Image that was scanned
    pub image: String,
    
    /// Scanner used
    pub scanner: String,
    
    /// Timestamp of the scan
    pub scan_time: String,
    
    /// List of vulnerabilities found
    pub vulnerabilities: Vec<Vulnerability>,
    
    /// Summary of vulnerabilities by severity
    pub summary: ScanSummary,
}

/// Individual vulnerability
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    /// Vulnerability ID (CVE, GHSA, etc.)
    pub id: String,
    
    /// Name of the affected package
    pub package_name: String,
    
    /// Installed version
    pub installed_version: String,
    
    /// Fixed version (if available)
    pub fixed_version: Option<String>,
    
    /// Severity level
    pub severity: String,
    
    /// Description of the vulnerability
    pub description: String,
    
    /// References for more information
    pub references: Vec<String>,
}

/// Summary of vulnerabilities by severity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSummary {
    /// Number of CRITICAL vulnerabilities
    pub critical: usize,
    
    /// Number of HIGH vulnerabilities
    pub high: usize,
    
    /// Number of MEDIUM vulnerabilities
    pub medium: usize,
    
    /// Number of LOW vulnerabilities
    pub low: usize,
    
    /// Total number of vulnerabilities
    pub total: usize,
}

// Trivy JSON output structures
#[derive(Debug, Deserialize)]
struct TrivyScanResult {
    #[serde(rename = "Results")]
    results: Vec<TrivyResult>,
}

#[derive(Debug, Deserialize)]
struct TrivyResult {
    #[serde(rename = "Target")]
    target: String,
    
    #[serde(rename = "Vulnerabilities")]
    vulnerabilities: Option<Vec<TrivyVulnerability>>,
}

#[derive(Debug, Deserialize)]
struct TrivyVulnerability {
    #[serde(rename = "VulnerabilityID")]
    vulnerability_id: String,
    
    #[serde(rename = "PkgName")]
    pkg_name: String,
    
    #[serde(rename = "InstalledVersion")]
    installed_version: String,
    
    #[serde(rename = "FixedVersion")]
    fixed_version: Option<String>,
    
    #[serde(rename = "Severity")]
    severity: String,
    
    #[serde(rename = "Description")]
    description: Option<String>,
    
    #[serde(rename = "References")]
    references: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scanner_creation() {
        let scanner = ImageScanner::new(ScannerType::Trivy);
        assert_eq!(scanner.scanner_type, ScannerType::Trivy);
    }

    #[test]
    fn test_scan_summary() {
        let summary = ScanSummary {
            critical: 1,
            high: 3,
            medium: 5,
            low: 10,
            total: 19,
        };

        assert_eq!(summary.critical, 1);
        assert_eq!(summary.total, 19);
    }

    #[tokio::test]
    async fn test_vulnerability_check() {
        let scanner = ImageScanner::new(ScannerType::Trivy);
        
        let report = ScanReport {
            image: "test:latest".to_string(),
            scanner: "Trivy".to_string(),
            scan_time: chrono::Utc::now().to_rfc3339(),
            vulnerabilities: vec![],
            summary: ScanSummary {
                critical: 0,
                high: 0,
                medium: 0,
                low: 0,
                total: 0,
            },
        };

        let result = scanner.check_vulnerabilities(&report).await.unwrap();
        assert!(result, "Clean image should pass");
        
        let report_with_critical = ScanReport {
            summary: ScanSummary {
                critical: 1,
                high: 0,
                medium: 0,
                low: 0,
                total: 1,
            },
            ..report
        };

        let result = scanner.check_vulnerabilities(&report_with_critical).await.unwrap();
        assert!(!result, "Image with critical vulnerability should fail");
    }
}
