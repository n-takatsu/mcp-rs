//! Performance improvement recommendations

use crate::error::Result;
use serde::{Deserialize, Serialize};

use super::analyzer::AnalysisResult;
use super::bottleneck::Bottleneck;
use super::optimizer::{OptimizationReport, OptimizationStrategy, OptimizationSuggestion};

/// Impact level for recommendations
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ImpactLevel {
    /// Low impact
    Low,
    /// Medium impact
    Medium,
    /// High impact
    High,
    /// Critical impact
    Critical,
}

impl ImpactLevel {
    /// Converts impact score to level
    pub fn from_score(score: f64) -> Self {
        if score >= 0.8 {
            ImpactLevel::Critical
        } else if score >= 0.6 {
            ImpactLevel::High
        } else if score >= 0.4 {
            ImpactLevel::Medium
        } else {
            ImpactLevel::Low
        }
    }
}

/// Performance improvement recommendation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Recommendation title
    pub title: String,
    /// Detailed description
    pub description: String,
    /// Implementation steps
    pub steps: Vec<String>,
    /// Expected impact level
    pub impact_level: ImpactLevel,
    /// Impact estimate details
    pub impact_estimate: ImpactEstimate,
    /// Priority (0-100)
    pub priority: u8,
}

impl Recommendation {
    /// Creates a new recommendation
    pub fn new(title: String, description: String) -> Self {
        Self {
            title,
            description,
            steps: Vec::new(),
            impact_level: ImpactLevel::Medium,
            impact_estimate: ImpactEstimate::default(),
            priority: 50,
        }
    }

    /// Adds an implementation step
    pub fn add_step(&mut self, step: String) {
        self.steps.push(step);
    }

    /// Sets impact level
    pub fn with_impact_level(mut self, level: ImpactLevel) -> Self {
        self.impact_level = level;
        self
    }

    /// Sets impact estimate
    pub fn with_impact_estimate(mut self, estimate: ImpactEstimate) -> Self {
        self.impact_level = ImpactLevel::from_score(estimate.overall_score());
        self.impact_estimate = estimate;
        self
    }

    /// Sets priority
    pub fn with_priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
}

/// Detailed impact estimate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactEstimate {
    /// Expected CPU improvement (percentage)
    pub cpu_improvement: f64,
    /// Expected memory improvement (percentage)
    pub memory_improvement: f64,
    /// Expected response time improvement (percentage)
    pub response_time_improvement: f64,
    /// Expected throughput improvement (percentage)
    pub throughput_improvement: f64,
    /// Implementation cost (hours)
    pub implementation_hours: f64,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f64,
}

impl ImpactEstimate {
    /// Creates a new impact estimate
    pub fn new() -> Self {
        Self {
            cpu_improvement: 0.0,
            memory_improvement: 0.0,
            response_time_improvement: 0.0,
            throughput_improvement: 0.0,
            implementation_hours: 0.0,
            confidence: 0.5,
        }
    }

    /// Calculates overall impact score (0.0 to 1.0)
    pub fn overall_score(&self) -> f64 {
        let weighted_score = (self.cpu_improvement * 0.25
            + self.memory_improvement * 0.25
            + self.response_time_improvement * 0.30
            + self.throughput_improvement * 0.20)
            / 100.0;

        weighted_score * self.confidence
    }

    /// Returns ROI score (impact per hour)
    pub fn roi_score(&self) -> f64 {
        if self.implementation_hours > 0.0 {
            self.overall_score() / self.implementation_hours
        } else {
            0.0
        }
    }
}

impl Default for ImpactEstimate {
    fn default() -> Self {
        Self::new()
    }
}

/// Recommendation engine trait
pub trait RecommendationEngine: Send + Sync {
    /// Generates recommendations from optimization report
    fn generate_recommendations(&self, report: &OptimizationReport) -> Result<Vec<Recommendation>>;

    /// Prioritizes recommendations
    fn prioritize_recommendations(&self, recommendations: &[Recommendation])
        -> Vec<Recommendation>;
}

/// Default recommendation engine implementation
#[derive(Debug, Clone)]
pub struct DefaultRecommendationEngine;

impl DefaultRecommendationEngine {
    /// Creates a new default recommendation engine
    pub fn new() -> Self {
        Self
    }

    /// Converts optimization suggestion to recommendation
    fn suggestion_to_recommendation(&self, suggestion: &OptimizationSuggestion) -> Recommendation {
        let mut recommendation = Recommendation::new(
            suggestion.strategy.description().to_string(),
            suggestion.description.clone(),
        );

        // Generate implementation steps based on strategy
        let steps = self.generate_steps(suggestion.strategy);
        for step in steps {
            recommendation.add_step(step);
        }

        // Create impact estimate
        let impact_estimate = ImpactEstimate {
            cpu_improvement: self.estimate_cpu_impact(suggestion.strategy),
            memory_improvement: self.estimate_memory_impact(suggestion.strategy),
            response_time_improvement: self.estimate_response_time_impact(suggestion.strategy),
            throughput_improvement: self.estimate_throughput_impact(suggestion.strategy),
            implementation_hours: suggestion.estimated_hours,
            confidence: suggestion.expected_impact,
        };

        recommendation = recommendation.with_impact_estimate(impact_estimate);
        recommendation = recommendation.with_priority(suggestion.priority);

        recommendation
    }

    /// Generates implementation steps for a strategy
    fn generate_steps(&self, strategy: OptimizationStrategy) -> Vec<String> {
        match strategy {
            OptimizationStrategy::QueryOptimization => vec![
                "1. Identify slow queries using database profiling tools".to_string(),
                "2. Analyze query execution plans".to_string(),
                "3. Add appropriate indexes or optimize existing queries".to_string(),
                "4. Test performance improvements".to_string(),
                "5. Monitor query performance after deployment".to_string(),
            ],
            OptimizationStrategy::CacheOptimization => vec![
                "1. Analyze data access patterns".to_string(),
                "2. Identify frequently accessed data".to_string(),
                "3. Implement or configure caching layer".to_string(),
                "4. Set appropriate cache TTL and eviction policies".to_string(),
                "5. Monitor cache hit rate and adjust as needed".to_string(),
            ],
            OptimizationStrategy::ResourceAllocation => vec![
                "1. Review current resource allocation".to_string(),
                "2. Analyze resource usage patterns".to_string(),
                "3. Adjust resource limits based on analysis".to_string(),
                "4. Test with new allocation".to_string(),
                "5. Monitor for improvements or issues".to_string(),
            ],
            OptimizationStrategy::Scaling => vec![
                "1. Assess current load and capacity".to_string(),
                "2. Determine scaling strategy (horizontal/vertical)".to_string(),
                "3. Configure auto-scaling rules if applicable".to_string(),
                "4. Test scaling behavior under load".to_string(),
                "5. Monitor system performance after scaling".to_string(),
            ],
            OptimizationStrategy::CodeOptimization => vec![
                "1. Profile application code to identify hotspots".to_string(),
                "2. Analyze algorithmic complexity".to_string(),
                "3. Refactor inefficient code paths".to_string(),
                "4. Add performance tests".to_string(),
                "5. Validate improvements with benchmarks".to_string(),
            ],
            OptimizationStrategy::ConfigurationTuning => vec![
                "1. Review current configuration settings".to_string(),
                "2. Research optimal configuration values".to_string(),
                "3. Test configuration changes in staging".to_string(),
                "4. Gradually roll out to production".to_string(),
                "5. Monitor impact and adjust as needed".to_string(),
            ],
        }
    }

    /// Estimates CPU improvement
    fn estimate_cpu_impact(&self, strategy: OptimizationStrategy) -> f64 {
        match strategy {
            OptimizationStrategy::QueryOptimization => 15.0,
            OptimizationStrategy::CacheOptimization => 20.0,
            OptimizationStrategy::ResourceAllocation => 5.0,
            OptimizationStrategy::Scaling => 30.0,
            OptimizationStrategy::CodeOptimization => 25.0,
            OptimizationStrategy::ConfigurationTuning => 10.0,
        }
    }

    /// Estimates memory improvement
    fn estimate_memory_impact(&self, strategy: OptimizationStrategy) -> f64 {
        match strategy {
            OptimizationStrategy::QueryOptimization => 10.0,
            OptimizationStrategy::CacheOptimization => 15.0,
            OptimizationStrategy::ResourceAllocation => 20.0,
            OptimizationStrategy::Scaling => 25.0,
            OptimizationStrategy::CodeOptimization => 20.0,
            OptimizationStrategy::ConfigurationTuning => 10.0,
        }
    }

    /// Estimates response time improvement
    fn estimate_response_time_impact(&self, strategy: OptimizationStrategy) -> f64 {
        match strategy {
            OptimizationStrategy::QueryOptimization => 40.0,
            OptimizationStrategy::CacheOptimization => 50.0,
            OptimizationStrategy::ResourceAllocation => 15.0,
            OptimizationStrategy::Scaling => 30.0,
            OptimizationStrategy::CodeOptimization => 35.0,
            OptimizationStrategy::ConfigurationTuning => 20.0,
        }
    }

    /// Estimates throughput improvement
    fn estimate_throughput_impact(&self, strategy: OptimizationStrategy) -> f64 {
        match strategy {
            OptimizationStrategy::QueryOptimization => 25.0,
            OptimizationStrategy::CacheOptimization => 40.0,
            OptimizationStrategy::ResourceAllocation => 20.0,
            OptimizationStrategy::Scaling => 50.0,
            OptimizationStrategy::CodeOptimization => 30.0,
            OptimizationStrategy::ConfigurationTuning => 15.0,
        }
    }
}

impl Default for DefaultRecommendationEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl RecommendationEngine for DefaultRecommendationEngine {
    fn generate_recommendations(&self, report: &OptimizationReport) -> Result<Vec<Recommendation>> {
        let mut recommendations = Vec::new();

        for suggestion in &report.suggestions {
            let recommendation = self.suggestion_to_recommendation(suggestion);
            recommendations.push(recommendation);
        }

        Ok(recommendations)
    }

    fn prioritize_recommendations(
        &self,
        recommendations: &[Recommendation],
    ) -> Vec<Recommendation> {
        let mut sorted = recommendations.to_vec();

        // Sort by priority and impact level
        sorted.sort_by(|a, b| {
            let priority_cmp = b.priority.cmp(&a.priority);
            if priority_cmp == std::cmp::Ordering::Equal {
                b.impact_level.cmp(&a.impact_level)
            } else {
                priority_cmp
            }
        });

        sorted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_impact_level_from_score() {
        assert_eq!(ImpactLevel::from_score(0.9), ImpactLevel::Critical);
        assert_eq!(ImpactLevel::from_score(0.7), ImpactLevel::High);
        assert_eq!(ImpactLevel::from_score(0.5), ImpactLevel::Medium);
        assert_eq!(ImpactLevel::from_score(0.2), ImpactLevel::Low);
    }

    #[test]
    fn test_impact_estimate_overall_score() {
        let estimate = ImpactEstimate {
            cpu_improvement: 20.0,
            memory_improvement: 15.0,
            response_time_improvement: 30.0,
            throughput_improvement: 25.0,
            implementation_hours: 8.0,
            confidence: 0.9,
        };

        let score = estimate.overall_score();
        assert!(score > 0.0 && score <= 1.0);
    }

    #[test]
    fn test_impact_estimate_roi_score() {
        let estimate = ImpactEstimate {
            cpu_improvement: 20.0,
            memory_improvement: 20.0,
            response_time_improvement: 30.0,
            throughput_improvement: 30.0,
            implementation_hours: 4.0,
            confidence: 0.9,
        };

        let roi = estimate.roi_score();
        assert!(roi > 0.0);
    }

    #[test]
    fn test_recommendation_creation() {
        let recommendation = Recommendation::new(
            "Test Recommendation".to_string(),
            "Test description".to_string(),
        );

        assert_eq!(recommendation.title, "Test Recommendation");
        assert_eq!(recommendation.steps.len(), 0);
    }

    #[test]
    fn test_generate_recommendations() {
        let engine = DefaultRecommendationEngine::new();
        let mut report = OptimizationReport::new("Test report".to_string());

        report.add_suggestion(
            OptimizationSuggestion::new(
                OptimizationStrategy::CacheOptimization,
                "Improve cache".to_string(),
            )
            .with_impact(0.8)
            .with_difficulty(0.3),
        );

        let recommendations = engine.generate_recommendations(&report).unwrap();
        assert!(!recommendations.is_empty());
    }

    #[test]
    fn test_prioritize_recommendations() {
        let engine = DefaultRecommendationEngine::new();
        let recommendations = vec![
            Recommendation::new("Low priority".to_string(), "".to_string()).with_priority(30),
            Recommendation::new("High priority".to_string(), "".to_string()).with_priority(80),
            Recommendation::new("Medium priority".to_string(), "".to_string()).with_priority(50),
        ];

        let sorted = engine.prioritize_recommendations(&recommendations);
        assert_eq!(sorted[0].priority, 80);
        assert_eq!(sorted[2].priority, 30);
    }
}
