//! Performance optimization advisor

use crate::error::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::analyzer::AnalysisResult;
use super::bottleneck::{Bottleneck, BottleneckCategory};

/// Optimization strategy types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum OptimizationStrategy {
    /// Query optimization
    QueryOptimization,
    /// Cache configuration
    CacheOptimization,
    /// Resource allocation
    ResourceAllocation,
    /// Scaling recommendation
    Scaling,
    /// Code optimization
    CodeOptimization,
    /// Configuration tuning
    ConfigurationTuning,
}

impl OptimizationStrategy {
    /// Returns a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            OptimizationStrategy::QueryOptimization => "Database Query Optimization",
            OptimizationStrategy::CacheOptimization => "Cache Strategy Optimization",
            OptimizationStrategy::ResourceAllocation => "Resource Allocation Adjustment",
            OptimizationStrategy::Scaling => "Scaling Recommendation",
            OptimizationStrategy::CodeOptimization => "Code-level Optimization",
            OptimizationStrategy::ConfigurationTuning => "Configuration Tuning",
        }
    }
}

/// Optimization suggestion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationSuggestion {
    /// Strategy type
    pub strategy: OptimizationStrategy,
    /// Description of the suggestion
    pub description: String,
    /// Expected impact (0.0 to 1.0)
    pub expected_impact: f64,
    /// Implementation difficulty (0.0 to 1.0)
    pub difficulty: f64,
    /// Estimated time to implement (in hours)
    pub estimated_hours: f64,
    /// Priority score (higher is more important)
    pub priority: u8,
}

impl OptimizationSuggestion {
    /// Creates a new optimization suggestion
    pub fn new(strategy: OptimizationStrategy, description: String) -> Self {
        Self {
            strategy,
            description,
            expected_impact: 0.5,
            difficulty: 0.5,
            estimated_hours: 4.0,
            priority: 50,
        }
    }

    /// Sets expected impact
    pub fn with_impact(mut self, impact: f64) -> Self {
        self.expected_impact = impact.clamp(0.0, 1.0);
        self
    }

    /// Sets difficulty
    pub fn with_difficulty(mut self, difficulty: f64) -> Self {
        self.difficulty = difficulty.clamp(0.0, 1.0);
        self
    }

    /// Sets estimated hours
    pub fn with_hours(mut self, hours: f64) -> Self {
        self.estimated_hours = hours;
        self
    }

    /// Calculates priority based on impact and difficulty
    pub fn calculate_priority(&mut self) {
        // Higher impact and lower difficulty = higher priority
        let score = (self.expected_impact / (self.difficulty + 0.1)) * 100.0;
        self.priority = score.min(100.0) as u8;
    }
}

/// Optimization report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptimizationReport {
    /// Analysis result this report is based on
    pub analysis_summary: String,
    /// List of optimization suggestions
    pub suggestions: Vec<OptimizationSuggestion>,
    /// Quick wins (easy, high-impact)
    pub quick_wins: Vec<OptimizationSuggestion>,
    /// Long-term improvements
    pub long_term: Vec<OptimizationSuggestion>,
    /// Report generated at
    pub generated_at: i64,
}

impl OptimizationReport {
    /// Creates a new optimization report
    pub fn new(analysis_summary: String) -> Self {
        Self {
            analysis_summary,
            suggestions: Vec::new(),
            quick_wins: Vec::new(),
            long_term: Vec::new(),
            generated_at: chrono::Utc::now().timestamp(),
        }
    }

    /// Adds a suggestion to the report
    pub fn add_suggestion(&mut self, mut suggestion: OptimizationSuggestion) {
        suggestion.calculate_priority();

        // Categorize suggestion
        if suggestion.expected_impact > 0.6 && suggestion.difficulty < 0.4 {
            self.quick_wins.push(suggestion.clone());
        } else if suggestion.estimated_hours > 16.0 {
            self.long_term.push(suggestion.clone());
        }

        self.suggestions.push(suggestion);
    }

    /// Sorts suggestions by priority
    pub fn sort_by_priority(&mut self) {
        self.suggestions.sort_by(|a, b| b.priority.cmp(&a.priority));
        self.quick_wins.sort_by(|a, b| b.priority.cmp(&a.priority));
        self.long_term.sort_by(|a, b| b.priority.cmp(&a.priority));
    }
}

/// Optimization advisor trait
pub trait OptimizationAdvisor: Send + Sync {
    /// Generates optimization report from analysis result
    fn generate_report(&self, analysis: &AnalysisResult) -> Result<OptimizationReport>;

    /// Suggests optimizations for specific bottleneck
    fn suggest_for_bottleneck(&self, bottleneck: &Bottleneck) -> Vec<OptimizationSuggestion>;
}

/// Default optimization advisor implementation
#[derive(Debug, Clone)]
pub struct DefaultOptimizationAdvisor;

impl DefaultOptimizationAdvisor {
    /// Creates a new default optimization advisor
    pub fn new() -> Self {
        Self
    }

    /// Generates CPU optimization suggestions
    fn suggest_cpu_optimizations(&self, _bottleneck: &Bottleneck) -> Vec<OptimizationSuggestion> {
        vec![
            OptimizationSuggestion::new(
                OptimizationStrategy::CodeOptimization,
                "Profile and optimize CPU-intensive code paths".to_string(),
            )
            .with_impact(0.7)
            .with_difficulty(0.6)
            .with_hours(8.0),
            OptimizationSuggestion::new(
                OptimizationStrategy::Scaling,
                "Consider horizontal scaling to distribute CPU load".to_string(),
            )
            .with_impact(0.8)
            .with_difficulty(0.5)
            .with_hours(4.0),
        ]
    }

    /// Generates memory optimization suggestions
    fn suggest_memory_optimizations(
        &self,
        _bottleneck: &Bottleneck,
    ) -> Vec<OptimizationSuggestion> {
        vec![
            OptimizationSuggestion::new(
                OptimizationStrategy::CodeOptimization,
                "Review and optimize memory allocations and data structures".to_string(),
            )
            .with_impact(0.6)
            .with_difficulty(0.7)
            .with_hours(12.0),
            OptimizationSuggestion::new(
                OptimizationStrategy::ResourceAllocation,
                "Increase available memory or optimize memory limits".to_string(),
            )
            .with_impact(0.7)
            .with_difficulty(0.3)
            .with_hours(2.0),
        ]
    }

    /// Generates cache optimization suggestions
    fn suggest_cache_optimizations(&self, _bottleneck: &Bottleneck) -> Vec<OptimizationSuggestion> {
        vec![
            OptimizationSuggestion::new(
                OptimizationStrategy::CacheOptimization,
                "Implement or improve caching strategy for frequently accessed data".to_string(),
            )
            .with_impact(0.8)
            .with_difficulty(0.4)
            .with_hours(6.0),
            OptimizationSuggestion::new(
                OptimizationStrategy::ConfigurationTuning,
                "Tune cache size and eviction policies".to_string(),
            )
            .with_impact(0.6)
            .with_difficulty(0.2)
            .with_hours(2.0),
        ]
    }

    /// Generates database optimization suggestions
    fn suggest_database_optimizations(
        &self,
        _bottleneck: &Bottleneck,
    ) -> Vec<OptimizationSuggestion> {
        vec![
            OptimizationSuggestion::new(
                OptimizationStrategy::QueryOptimization,
                "Analyze and optimize slow database queries".to_string(),
            )
            .with_impact(0.9)
            .with_difficulty(0.5)
            .with_hours(8.0),
            OptimizationSuggestion::new(
                OptimizationStrategy::QueryOptimization,
                "Add missing database indexes".to_string(),
            )
            .with_impact(0.8)
            .with_difficulty(0.3)
            .with_hours(4.0),
            OptimizationSuggestion::new(
                OptimizationStrategy::CacheOptimization,
                "Implement query result caching".to_string(),
            )
            .with_impact(0.7)
            .with_difficulty(0.4)
            .with_hours(6.0),
        ]
    }

    /// Generates application optimization suggestions
    fn suggest_application_optimizations(
        &self,
        _bottleneck: &Bottleneck,
    ) -> Vec<OptimizationSuggestion> {
        vec![
            OptimizationSuggestion::new(
                OptimizationStrategy::CodeOptimization,
                "Profile application code to identify slow operations".to_string(),
            )
            .with_impact(0.7)
            .with_difficulty(0.6)
            .with_hours(8.0),
            OptimizationSuggestion::new(
                OptimizationStrategy::CacheOptimization,
                "Implement application-level caching".to_string(),
            )
            .with_impact(0.6)
            .with_difficulty(0.4)
            .with_hours(6.0),
        ]
    }
}

impl Default for DefaultOptimizationAdvisor {
    fn default() -> Self {
        Self::new()
    }
}

impl OptimizationAdvisor for DefaultOptimizationAdvisor {
    fn generate_report(&self, analysis: &AnalysisResult) -> Result<OptimizationReport> {
        let mut report = OptimizationReport::new(analysis.summary.clone());

        // Generate suggestions for each bottleneck
        for bottleneck in &analysis.bottlenecks {
            let suggestions = self.suggest_for_bottleneck(bottleneck);
            for suggestion in suggestions {
                report.add_suggestion(suggestion);
            }
        }

        // Sort suggestions by priority
        report.sort_by_priority();

        Ok(report)
    }

    fn suggest_for_bottleneck(&self, bottleneck: &Bottleneck) -> Vec<OptimizationSuggestion> {
        match bottleneck.category {
            BottleneckCategory::Cpu => self.suggest_cpu_optimizations(bottleneck),
            BottleneckCategory::Memory => self.suggest_memory_optimizations(bottleneck),
            BottleneckCategory::Cache => self.suggest_cache_optimizations(bottleneck),
            BottleneckCategory::Database => self.suggest_database_optimizations(bottleneck),
            BottleneckCategory::Application => self.suggest_application_optimizations(bottleneck),
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::bottleneck::{BottleneckCategory, Severity};
    use super::*;

    #[test]
    fn test_optimization_suggestion_priority() {
        let mut suggestion = OptimizationSuggestion::new(
            OptimizationStrategy::CacheOptimization,
            "Test".to_string(),
        )
        .with_impact(0.8)
        .with_difficulty(0.2);

        suggestion.calculate_priority();
        assert!(suggestion.priority > 50);
    }

    #[test]
    fn test_suggest_cpu_optimizations() {
        let advisor = DefaultOptimizationAdvisor::new();
        let bottleneck = Bottleneck::new(
            BottleneckCategory::Cpu,
            Severity::High,
            "High CPU".to_string(),
        );

        let suggestions = advisor.suggest_for_bottleneck(&bottleneck);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_suggest_database_optimizations() {
        let advisor = DefaultOptimizationAdvisor::new();
        let bottleneck = Bottleneck::new(
            BottleneckCategory::Database,
            Severity::High,
            "Slow queries".to_string(),
        );

        let suggestions = advisor.suggest_for_bottleneck(&bottleneck);
        assert!(!suggestions.is_empty());
    }

    #[test]
    fn test_optimization_report_categorization() {
        let mut report = OptimizationReport::new("Test report".to_string());

        // Quick win
        report.add_suggestion(
            OptimizationSuggestion::new(
                OptimizationStrategy::ConfigurationTuning,
                "Quick fix".to_string(),
            )
            .with_impact(0.8)
            .with_difficulty(0.2)
            .with_hours(2.0),
        );

        // Long term
        report.add_suggestion(
            OptimizationSuggestion::new(
                OptimizationStrategy::CodeOptimization,
                "Major refactor".to_string(),
            )
            .with_impact(0.6)
            .with_difficulty(0.8)
            .with_hours(40.0),
        );

        assert!(!report.quick_wins.is_empty());
        assert!(!report.long_term.is_empty());
    }
}
