//! Optimization Advisor Module
//!
//! パフォーマンス最適化アドバイザーシステム

mod bottleneck;
mod core;
mod recommendation;

pub use bottleneck::{Bottleneck, BottleneckType};
pub use core::OptimizationAdvisor;
pub use recommendation::OptimizationRecommendation;
