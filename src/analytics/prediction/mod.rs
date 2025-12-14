//! Prediction Analytics Module
//!
//! 予測分析システム

mod predictor;
mod trend;
mod types;

pub use predictor::TimeSeriesPredictor;
pub use trend::TrendDetector;
pub use types::{PredictionResult, TrendDirection};
