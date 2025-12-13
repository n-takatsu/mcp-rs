//! Anomaly Detection Models
//!
//! 機械学習と統計手法を使用した異常検知モデル。

use super::{DetectionMethod, FeatureVector, MLConfig};
use crate::error::McpError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, info};

/// 異常検知モデル
#[derive(Debug)]
pub enum AnomalyModel {
    /// 統計ベース（Z-score）
    Statistical(StatisticalModel),
    /// 簡易Isolation Forest（実装予定）
    IsolationForest,
    /// アンサンブル（複数モデルの組み合わせ）
    Ensemble(Vec<AnomalyModel>),
}

/// 統計ベースモデル
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatisticalModel {
    /// 各特徴量の平均値
    pub means: Vec<f64>,
    /// 各特徴量の標準偏差
    pub std_devs: Vec<f64>,
    /// Z-scoreしきい値
    pub z_threshold: f64,
    /// トレーニングサンプル数
    pub sample_count: usize,
}

impl StatisticalModel {
    /// 新しい統計モデルを作成
    pub fn new(z_threshold: f64) -> Self {
        Self {
            means: Vec::new(),
            std_devs: Vec::new(),
            z_threshold,
            sample_count: 0,
        }
    }

    /// トレーニングデータからモデルを学習
    pub fn train(&mut self, samples: &[FeatureVector]) -> Result<(), McpError> {
        if samples.is_empty() {
            return Err(McpError::Config("No training samples provided".to_string()));
        }

        let feature_count = samples[0].features.len();
        info!(
            "Training statistical model with {} samples, {} features",
            samples.len(),
            feature_count
        );

        self.means = vec![0.0; feature_count];
        self.std_devs = vec![0.0; feature_count];
        self.sample_count = samples.len();

        // 各特徴量の平均を計算
        for sample in samples {
            for (i, &value) in sample.features.iter().enumerate() {
                self.means[i] += value;
            }
        }

        for mean in &mut self.means {
            *mean /= samples.len() as f64;
        }

        // 各特徴量の標準偏差を計算
        for sample in samples {
            for (i, &value) in sample.features.iter().enumerate() {
                self.std_devs[i] += (value - self.means[i]).powi(2);
            }
        }

        for std_dev in &mut self.std_devs {
            *std_dev = (*std_dev / samples.len() as f64).sqrt();
            // 標準偏差が0または非常に小さい場合、小さな値を設定
            if *std_dev < 1e-6 {
                *std_dev = 1e-6;
            }
        }

        info!("Statistical model training completed");
        Ok(())
    }

    /// 異常スコアを計算
    pub fn predict(&self, features: &[f64]) -> Result<f64, McpError> {
        if features.len() != self.means.len() {
            return Err(McpError::Config(format!(
                "Feature dimension mismatch: expected {}, got {}",
                self.means.len(),
                features.len()
            )));
        }

        // 各特徴量のZ-scoreを計算
        let z_scores: Vec<f64> = features
            .iter()
            .enumerate()
            .map(|(i, &value)| {
                let z = ((value - self.means[i]) / self.std_devs[i]).abs();
                // 異常に大きなZ-scoreをクランプ（例: max 10）
                z.min(10.0)
            })
            .collect();

        // 最大Z-scoreを異常スコアとして使用
        let max_z_score = z_scores.iter().cloned().fold(0.0, f64::max);

        // Z-scoreを0-1の範囲に正規化
        let anomaly_score = (max_z_score / self.z_threshold).min(1.0);

        debug!(
            "Anomaly score: {:.4}, max Z-score: {:.4}",
            anomaly_score, max_z_score
        );

        Ok(anomaly_score)
    }

    /// 特徴量の重要度を取得
    pub fn get_feature_importance(&self, features: &[f64]) -> HashMap<usize, f64> {
        let mut importance = HashMap::new();

        for (i, &value) in features.iter().enumerate() {
            if self.std_devs[i] > 0.0 {
                let z_score = ((value - self.means[i]) / self.std_devs[i]).abs();
                importance.insert(i, z_score);
            }
        }

        importance
    }
}

/// Isolation Forest（簡易実装）
///
/// 注: 完全な実装にはsmartcoreまたは専用ライブラリが必要
/// ここでは基本的なランダムツリーベースの実装を提供
#[derive(Debug, Clone)]
pub struct SimpleIsolationForest {
    /// ランダムツリーの数
    pub n_trees: usize,
    /// サンプリングサイズ
    pub sample_size: usize,
    /// 平均パス長（正常データ）
    pub average_path_length: f64,
}

impl SimpleIsolationForest {
    /// 新しいIsolation Forestモデルを作成
    pub fn new(n_trees: usize, sample_size: usize) -> Self {
        Self {
            n_trees,
            sample_size,
            average_path_length: 0.0,
        }
    }

    /// トレーニング（簡易実装）
    pub fn train(&mut self, _samples: &[FeatureVector]) -> Result<(), McpError> {
        // 実際の実装では、ランダムツリーを構築して平均パス長を計算
        // ここでは簡略化のため、固定値を使用
        self.average_path_length = 10.0;
        info!("Simple Isolation Forest training completed");
        Ok(())
    }

    /// 異常スコアを計算（簡易実装）
    pub fn predict(&self, _features: &[f64]) -> Result<f64, McpError> {
        // 実際の実装では、各ツリーでのパス長を計算
        // ここでは簡略化のため、ランダム値を返す
        // 実運用では smartcore の IsolationForest を使用すべき
        Ok(0.5)
    }
}

/// K-means Clustering ベース異常検知
#[derive(Debug, Clone)]
pub struct KMeansAnomalyDetector {
    /// クラスター数
    pub n_clusters: usize,
    /// クラスター中心
    pub centroids: Vec<Vec<f64>>,
    /// 異常判定の距離しきい値
    pub distance_threshold: f64,
}

impl KMeansAnomalyDetector {
    /// 新しいK-meansモデルを作成
    pub fn new(n_clusters: usize) -> Self {
        Self {
            n_clusters,
            centroids: Vec::new(),
            distance_threshold: 2.0,
        }
    }

    /// トレーニング（簡易K-means実装）
    pub fn train(&mut self, samples: &[FeatureVector]) -> Result<(), McpError> {
        if samples.is_empty() {
            return Err(McpError::Config("No training samples provided".to_string()));
        }

        let feature_dim = samples[0].features.len();

        // 簡易実装: ランダムにクラスター中心を初期化
        // 実運用では smartcore の KMeans を使用すべき
        self.centroids = vec![vec![0.0; feature_dim]; self.n_clusters];

        // 最初のn_clusters個のサンプルをクラスター中心として使用
        for (i, sample) in samples.iter().take(self.n_clusters).enumerate() {
            self.centroids[i] = sample.features.clone();
        }

        info!(
            "K-means clustering training completed with {} clusters",
            self.n_clusters
        );
        Ok(())
    }

    /// 異常スコアを計算（最近傍クラスターまでの距離）
    pub fn predict(&self, features: &[f64]) -> Result<f64, McpError> {
        if self.centroids.is_empty() {
            return Err(McpError::Config("Model not trained".to_string()));
        }

        // 最近傍クラスターまでのユークリッド距離を計算
        let min_distance = self
            .centroids
            .iter()
            .map(|centroid| self.euclidean_distance(features, centroid))
            .fold(f64::INFINITY, f64::min);

        // 距離を0-1の範囲に正規化
        let anomaly_score = (min_distance / self.distance_threshold).min(1.0);

        Ok(anomaly_score)
    }

    /// ユークリッド距離を計算
    fn euclidean_distance(&self, a: &[f64], b: &[f64]) -> f64 {
        a.iter()
            .zip(b.iter())
            .map(|(x, y)| (x - y).powi(2))
            .sum::<f64>()
            .sqrt()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_features(values: Vec<f64>) -> FeatureVector {
        let len = values.len();
        FeatureVector {
            features: values,
            feature_names: (0..len).map(|i| format!("f{}", i)).collect(),
            request_id: "test".to_string(),
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_statistical_model_training() {
        let mut model = StatisticalModel::new(3.0);

        let samples = vec![
            create_test_features(vec![1.0, 2.0, 3.0]),
            create_test_features(vec![1.1, 2.1, 3.1]),
            create_test_features(vec![0.9, 1.9, 2.9]),
        ];

        let result = model.train(&samples);
        assert!(result.is_ok());
        assert_eq!(model.sample_count, 3);
    }

    #[test]
    fn test_statistical_model_prediction() {
        let mut model = StatisticalModel::new(3.0);

        let samples = vec![
            create_test_features(vec![1.0, 2.0, 3.0]),
            create_test_features(vec![1.1, 2.1, 3.1]),
            create_test_features(vec![0.9, 1.9, 2.9]),
        ];

        model.train(&samples).unwrap();

        // 正常データ
        let normal_score = model.predict(&[1.0, 2.0, 3.0]).unwrap();
        assert!(normal_score < 0.5);

        // 異常データ
        let anomaly_score = model.predict(&[10.0, 20.0, 30.0]).unwrap();
        assert!(anomaly_score > 0.5);
    }

    #[test]
    fn test_kmeans_training() {
        let mut model = KMeansAnomalyDetector::new(2);

        let samples = vec![
            create_test_features(vec![1.0, 2.0]),
            create_test_features(vec![1.1, 2.1]),
            create_test_features(vec![5.0, 6.0]),
        ];

        let result = model.train(&samples);
        assert!(result.is_ok());
        assert_eq!(model.centroids.len(), 2);
    }
}
