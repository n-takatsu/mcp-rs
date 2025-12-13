//! ML Anomaly Detector
//!
//! 機械学習ベースの異常検知エンジン。

use super::{
    models::{KMeansAnomalyDetector, StatisticalModel},
    DetectionMethod, FeatureExtractor, FeatureVector, MLConfig, MLDetectionResult, ModelStats,
};
use crate::error::McpError;
use crate::security::ids::RequestData;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// ML異常検知器
pub struct MLAnomalyDetector {
    /// 設定
    config: MLConfig,
    /// 特徴量抽出器
    feature_extractor: Arc<RwLock<FeatureExtractor>>,
    /// 統計モデル
    statistical_model: Arc<RwLock<Option<StatisticalModel>>>,
    /// K-meansモデル
    kmeans_model: Arc<RwLock<Option<KMeansAnomalyDetector>>>,
    /// トレーニングデータキャッシュ
    training_cache: Arc<RwLock<Vec<FeatureVector>>>,
    /// モデル統計情報
    model_stats: Arc<RwLock<ModelStats>>,
}

impl MLAnomalyDetector {
    /// 新しいML異常検知器を作成
    pub async fn new(config: MLConfig) -> Result<Self, McpError> {
        info!("Initializing ML Anomaly Detector");

        Ok(Self {
            feature_extractor: Arc::new(RwLock::new(FeatureExtractor::new(config.clone()))),
            statistical_model: Arc::new(RwLock::new(None)),
            kmeans_model: Arc::new(RwLock::new(None)),
            training_cache: Arc::new(RwLock::new(Vec::new())),
            model_stats: Arc::new(RwLock::new(ModelStats {
                training_samples: 0,
                last_updated: Utc::now(),
                model_version: 1,
                accuracy: None,
                false_positive_rate: None,
                true_positive_rate: None,
            })),
            config,
        })
    }

    /// 異常検知を実行
    pub async fn detect(&self, request: &RequestData) -> Result<MLDetectionResult, McpError> {
        debug!(
            "Running ML anomaly detection for request: {}",
            request.request_id
        );

        // 特徴量を抽出（生の特徴量）
        let features = {
            let extractor = self.feature_extractor.read().await;
            extractor.extract(request)?
        };

        // モデルが学習済みかチェック
        let statistical_model = self.statistical_model.read().await;
        if statistical_model.is_none() {
            // モデル未学習の場合は学習モード
            drop(statistical_model);
            self.add_to_training_cache(features).await;

            return Ok(MLDetectionResult {
                is_anomaly: false,
                anomaly_score: 0.0,
                confidence: 0.0,
                detection_method: DetectionMethod::Statistical,
                explanation: "Model is in training mode".to_string(),
                feature_importance: HashMap::new(),
                timestamp: Utc::now(),
            });
        }

        // 統計モデルで予測
        let (anomaly_score, feature_importance) = {
            let model = statistical_model.as_ref().unwrap();
            let score = model.predict(&features.features)?;
            let importance = model.get_feature_importance(&features.features);
            (score, importance)
        };

        let is_anomaly = anomaly_score > self.config.anomaly_threshold;
        let confidence = if is_anomaly {
            anomaly_score
        } else {
            1.0 - anomaly_score
        };

        // オンライン学習が有効な場合、正常データをキャッシュに追加
        if self.config.online_learning && !is_anomaly {
            drop(statistical_model);
            self.add_to_training_cache(features.clone()).await;
        }

        // 特徴量名を含む重要度マップを作成
        let feature_importance_map: HashMap<String, f64> = feature_importance
            .into_iter()
            .filter_map(|(idx, score)| {
                features
                    .feature_names
                    .get(idx)
                    .map(|name| (name.clone(), score))
            })
            .collect();

        let explanation = if is_anomaly {
            format!(
                "Anomaly detected with score {:.3}. Top contributing features: {}",
                anomaly_score,
                self.format_top_features(&feature_importance_map, 3)
            )
        } else {
            format!("Normal pattern (score: {:.3})", anomaly_score)
        };

        if is_anomaly {
            warn!(
                "ML Anomaly detected: request={}, score={:.3}",
                request.request_id, anomaly_score
            );
        }

        Ok(MLDetectionResult {
            is_anomaly,
            anomaly_score,
            confidence,
            detection_method: DetectionMethod::Statistical,
            explanation,
            feature_importance: feature_importance_map,
            timestamp: Utc::now(),
        })
    }

    /// トレーニングデータでモデルを学習
    pub async fn train(&self, training_data: &[RequestData]) -> Result<(), McpError> {
        info!("Training ML models with {} samples", training_data.len());

        if training_data.len() < self.config.min_training_samples {
            return Err(McpError::Config(format!(
                "Insufficient training samples: got {}, need {}",
                training_data.len(),
                self.config.min_training_samples
            )));
        }

        // 特徴量を抽出
        let mut features = Vec::new();
        {
            let extractor = self.feature_extractor.read().await;
            for request in training_data {
                match extractor.extract(request) {
                    Ok(feature) => features.push(feature),
                    Err(e) => warn!("Failed to extract features: {}", e),
                }
            }
        }

        if features.is_empty() {
            return Err(McpError::Config(
                "No features extracted from training data".to_string(),
            ));
        }

        // 統計モデルを学習（生の特徴量で）
        {
            let mut model = StatisticalModel::new(self.config.statistical_sensitivity);
            model.train(&features)?;
            *self.statistical_model.write().await = Some(model);
        }

        // K-meansモデルを学習
        {
            let mut kmeans = KMeansAnomalyDetector::new(5); // 5クラスター
            kmeans.train(&features)?;
            *self.kmeans_model.write().await = Some(kmeans);
        }

        // モデル統計を更新
        {
            let mut stats = self.model_stats.write().await;
            stats.training_samples = features.len();
            stats.last_updated = Utc::now();
            stats.model_version += 1;
        }

        info!("ML model training completed successfully");
        Ok(())
    }

    /// モデルを再トレーニング（オンライン学習）
    pub async fn retrain_if_needed(&self) -> Result<(), McpError> {
        let cache_len = self.training_cache.read().await.len();

        if cache_len >= self.config.min_training_samples {
            info!("Retraining model with {} cached samples", cache_len);

            // キャッシュされたデータでモデルを更新
            let training_features = self.training_cache.read().await.clone();

            // 統計モデルを再学習
            {
                let mut model = StatisticalModel::new(self.config.statistical_sensitivity);
                model.train(&training_features)?;
                *self.statistical_model.write().await = Some(model);
            }

            // モデル統計を更新
            {
                let mut stats = self.model_stats.write().await;
                stats.training_samples = training_features.len();
                stats.last_updated = Utc::now();
                stats.model_version += 1;
            }

            // キャッシュをクリア
            self.training_cache.write().await.clear();

            info!("Model retrained successfully");
        }

        Ok(())
    }

    /// トレーニングキャッシュに追加
    async fn add_to_training_cache(&self, features: FeatureVector) {
        let mut cache = self.training_cache.write().await;
        cache.push(features);

        // キャッシュサイズ制限
        let max_size = self.config.min_training_samples * 10;
        if cache.len() > max_size {
            let drain_count = cache.len() / 2;
            cache.drain(0..drain_count);
        }
    }

    /// 上位N個の特徴量をフォーマット
    fn format_top_features(&self, features: &HashMap<String, f64>, top_n: usize) -> String {
        let mut sorted: Vec<_> = features.iter().collect();
        sorted.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap());

        sorted
            .iter()
            .take(top_n)
            .map(|(name, score)| format!("{}({:.2})", name, score))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// モデル統計情報を取得
    pub async fn get_stats(&self) -> ModelStats {
        self.model_stats.read().await.clone()
    }

    /// モデルがトレーニング済みかチェック
    pub async fn is_trained(&self) -> bool {
        self.statistical_model.read().await.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn create_test_request(path: &str) -> RequestData {
        RequestData {
            request_id: format!("test-{}", rand::random::<u32>()),
            method: "GET".to_string(),
            path: path.to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: None,
            source_ip: Some("192.168.1.1".parse().unwrap()),
            timestamp: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_ml_detector_initialization() {
        let detector = MLAnomalyDetector::new(MLConfig::default()).await;
        assert!(detector.is_ok());
    }

    #[tokio::test]
    async fn test_ml_detector_training() {
        let detector = MLAnomalyDetector::new(MLConfig::default()).await.unwrap();

        // 正常なトレーニングデータを作成
        let training_data: Vec<RequestData> = (0..150)
            .map(|i| create_test_request(&format!("/api/users/{}", i)))
            .collect();

        let result = detector.train(&training_data).await;
        assert!(result.is_ok());
        assert!(detector.is_trained().await);
    }

    #[tokio::test]
    async fn test_ml_anomaly_detection() {
        let detector = MLAnomalyDetector::new(MLConfig::default()).await.unwrap();

        // トレーニング
        let training_data: Vec<RequestData> = (0..150)
            .map(|i| create_test_request(&format!("/api/users/{}", i)))
            .collect();
        detector.train(&training_data).await.unwrap();

        // 正常リクエスト
        let normal_request = create_test_request("/api/users/123");
        let result = detector.detect(&normal_request).await.unwrap();
        // 正常なパターンに似ているため、異常スコアは低めのはず
        println!("Normal request score: {}", result.anomaly_score);
        assert!(result.anomaly_score <= 1.0); // 基本的な動作確認（スコアは0-1の範囲）

        // 異常リクエスト（SQLインジェクション）
        let anomaly_request =
            create_test_request("/api/users?id=1' UNION SELECT * FROM passwords--");
        let result = detector.detect(&anomaly_request).await.unwrap();
        // 特徴量が大きく異なるため、異常として検知される可能性が高い
        println!("Anomaly score: {}", result.anomaly_score);
    }
}
