//! リソース制限と監視
//!
//! プラグインのCPU、メモリ、ディスクI/Oリソースの制限と監視を提供します。

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

/// リソース制限設定
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// 最大CPU使用率（%）
    pub max_cpu_percent: f64,
    /// 最大メモリ使用量（MB）
    pub max_memory_mb: u64,
    /// 最大ディスクI/O速度（MB/s）
    pub max_disk_io_mbps: u64,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_cpu_percent: 80.0,
            max_memory_mb: 1024,
            max_disk_io_mbps: 100,
        }
    }
}

impl ResourceLimits {
    /// 新しいリソース制限を作成
    ///
    /// # 引数
    ///
    /// * `max_cpu_percent` - 最大CPU使用率（%）
    /// * `max_memory_mb` - 最大メモリ使用量（MB）
    /// * `max_disk_io_mbps` - 最大ディスクI/O速度（MB/s）
    pub fn new(max_cpu_percent: f64, max_memory_mb: u64, max_disk_io_mbps: u64) -> Self {
        Self {
            max_cpu_percent,
            max_memory_mb,
            max_disk_io_mbps,
        }
    }

    /// リソース制限を検証
    pub fn validate(&self) -> Result<(), String> {
        if self.max_cpu_percent <= 0.0 || self.max_cpu_percent > 100.0 {
            return Err(format!(
                "Invalid CPU limit: {}% (must be 0 < cpu <= 100)",
                self.max_cpu_percent
            ));
        }

        if self.max_memory_mb == 0 {
            return Err("Invalid memory limit: 0 MB".to_string());
        }

        if self.max_disk_io_mbps == 0 {
            return Err("Invalid disk I/O limit: 0 MB/s".to_string());
        }

        Ok(())
    }

    /// Dockerのメモリ制限形式に変換（バイト）
    pub fn docker_memory_limit(&self) -> u64 {
        self.max_memory_mb * 1024 * 1024
    }

    /// Dockerのディスク書き込み制限形式に変換（バイト/秒）
    pub fn docker_disk_write_limit(&self) -> u64 {
        self.max_disk_io_mbps * 1024 * 1024
    }

    /// Dockerのディスク読み込み制限形式に変換（バイト/秒）
    pub fn docker_disk_read_limit(&self) -> u64 {
        self.max_disk_io_mbps * 1024 * 1024
    }
}

/// リソース使用状況
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// CPU使用率（%）
    pub cpu_percent: f64,
    /// メモリ使用量（MB）
    pub memory_mb: u64,
    /// ディスクI/O速度（MB/s）
    pub disk_io_mbps: u64,
    /// 最終更新時刻
    pub last_updated: Option<SystemTime>,
}

impl ResourceUsage {
    /// リソース使用状況が制限を超えているかチェック
    ///
    /// # 引数
    ///
    /// * `limits` - リソース制限
    pub fn exceeds_limits(&self, limits: &ResourceLimits) -> bool {
        self.cpu_percent > limits.max_cpu_percent
            || self.memory_mb > limits.max_memory_mb
            || self.disk_io_mbps > limits.max_disk_io_mbps
    }

    /// 制限超過の詳細を取得
    ///
    /// # 引数
    ///
    /// * `limits` - リソース制限
    pub fn get_violations(&self, limits: &ResourceLimits) -> Vec<String> {
        let mut violations = Vec::new();

        if self.cpu_percent > limits.max_cpu_percent {
            violations.push(format!(
                "CPU usage {}% exceeds limit {}%",
                self.cpu_percent, limits.max_cpu_percent
            ));
        }

        if self.memory_mb > limits.max_memory_mb {
            violations.push(format!(
                "Memory usage {} MB exceeds limit {} MB",
                self.memory_mb, limits.max_memory_mb
            ));
        }

        if self.disk_io_mbps > limits.max_disk_io_mbps {
            violations.push(format!(
                "Disk I/O {} MB/s exceeds limit {} MB/s",
                self.disk_io_mbps, limits.max_disk_io_mbps
            ));
        }

        violations
    }

    /// リソース使用率（%）を計算
    ///
    /// # 引数
    ///
    /// * `limits` - リソース制限
    pub fn calculate_utilization(&self, limits: &ResourceLimits) -> f64 {
        let cpu_util = self.cpu_percent / limits.max_cpu_percent;
        let memory_util = self.memory_mb as f64 / limits.max_memory_mb as f64;
        let disk_util = self.disk_io_mbps as f64 / limits.max_disk_io_mbps as f64;

        // 最大値を使用率とする
        cpu_util.max(memory_util).max(disk_util) * 100.0
    }
}

/// リソース監視統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatistics {
    /// 平均CPU使用率（%）
    pub avg_cpu_percent: f64,
    /// 最大CPU使用率（%）
    pub max_cpu_percent: f64,
    /// 平均メモリ使用量（MB）
    pub avg_memory_mb: u64,
    /// 最大メモリ使用量（MB）
    pub max_memory_mb: u64,
    /// 平均ディスクI/O速度（MB/s）
    pub avg_disk_io_mbps: u64,
    /// 最大ディスクI/O速度（MB/s）
    pub max_disk_io_mbps: u64,
    /// サンプル数
    pub sample_count: usize,
    /// 制限違反回数
    pub violation_count: usize,
}

impl Default for ResourceStatistics {
    fn default() -> Self {
        Self {
            avg_cpu_percent: 0.0,
            max_cpu_percent: 0.0,
            avg_memory_mb: 0,
            max_memory_mb: 0,
            avg_disk_io_mbps: 0,
            max_disk_io_mbps: 0,
            sample_count: 0,
            violation_count: 0,
        }
    }
}

/// リソース監視
///
/// プラグインのリソース使用状況をリアルタイムで監視します。
pub struct ResourceMonitor {
    /// プラグインID
    plugin_id: String,
    /// リソース制限
    limits: ResourceLimits,
    /// 現在のリソース使用状況
    current_usage: Arc<RwLock<ResourceUsage>>,
    /// リソース使用履歴
    usage_history: Arc<RwLock<Vec<ResourceUsage>>>,
    /// 統計情報
    statistics: Arc<RwLock<ResourceStatistics>>,
    /// 監視タスク
    monitor_task: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// 監視間隔
    monitor_interval: Duration,
    /// 履歴の最大サイズ
    max_history_size: usize,
}

impl ResourceMonitor {
    /// 新しいリソース監視を作成
    ///
    /// # 引数
    ///
    /// * `plugin_id` - プラグインID
    /// * `limits` - リソース制限
    pub fn new(plugin_id: String, limits: ResourceLimits) -> Self {
        Self {
            plugin_id,
            limits,
            current_usage: Arc::new(RwLock::new(ResourceUsage::default())),
            usage_history: Arc::new(RwLock::new(Vec::new())),
            statistics: Arc::new(RwLock::new(ResourceStatistics::default())),
            monitor_task: Arc::new(RwLock::new(None)),
            monitor_interval: Duration::from_secs(1),
            max_history_size: 100,
        }
    }

    /// 監視間隔を設定
    ///
    /// # 引数
    ///
    /// * `interval` - 監視間隔
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.monitor_interval = interval;
        self
    }

    /// 履歴の最大サイズを設定
    ///
    /// # 引数
    ///
    /// * `size` - 最大サイズ
    pub fn with_max_history_size(mut self, size: usize) -> Self {
        self.max_history_size = size;
        self
    }

    /// 監視を開始
    pub async fn start(&self) {
        let plugin_id = self.plugin_id.clone();
        let limits = self.limits.clone();
        let current_usage = self.current_usage.clone();
        let usage_history = self.usage_history.clone();
        let statistics = self.statistics.clone();
        let monitor_interval = self.monitor_interval;
        let max_history_size = self.max_history_size;

        let handle = tokio::spawn(async move {
            loop {
                // リソース使用状況を取得（実際にはDockerコンテナから取得）
                let usage = Self::fetch_resource_usage(&plugin_id).await;

                // 現在の使用状況を更新
                {
                    let mut current = current_usage.write().await;
                    *current = usage.clone();
                }

                // 履歴に追加
                {
                    let mut history = usage_history.write().await;
                    history.push(usage.clone());

                    // 履歴サイズを制限
                    if history.len() > max_history_size {
                        history.remove(0);
                    }
                }

                // 統計情報を更新
                {
                    let mut stats = statistics.write().await;
                    Self::update_statistics(&mut stats, &usage, &limits);
                }

                tokio::time::sleep(monitor_interval).await;
            }
        });

        let mut task = self.monitor_task.write().await;
        *task = Some(handle);
    }

    /// 監視を停止
    pub async fn stop(&self) {
        let mut task = self.monitor_task.write().await;
        if let Some(handle) = task.take() {
            handle.abort();
        }
    }

    /// 現在のリソース使用状況を取得
    pub async fn get_current_usage(&self) -> ResourceUsage {
        self.current_usage.read().await.clone()
    }

    /// リソース使用履歴を取得
    pub async fn get_usage_history(&self) -> Vec<ResourceUsage> {
        self.usage_history.read().await.clone()
    }

    /// 統計情報を取得
    pub async fn get_statistics(&self) -> ResourceStatistics {
        self.statistics.read().await.clone()
    }

    /// リソース使用状況を取得（実際の実装）
    ///
    /// # 引数
    ///
    /// * `plugin_id` - プラグインID
    async fn fetch_resource_usage(_plugin_id: &str) -> ResourceUsage {
        // TODO: 実際にはDockerコンテナのstatsから取得
        // docker stats --no-stream --format "{{.CPUPerc}},{{.MemUsage}},{{.NetIO}}"
        ResourceUsage {
            cpu_percent: 10.0 + (rand::random::<f64>() * 20.0),
            memory_mb: 100 + (rand::random::<u64>() % 200),
            disk_io_mbps: 10 + (rand::random::<u64>() % 50),
            last_updated: Some(SystemTime::now()),
        }
    }

    /// 統計情報を更新
    ///
    /// # 引数
    ///
    /// * `stats` - 統計情報
    /// * `usage` - リソース使用状況
    /// * `limits` - リソース制限
    fn update_statistics(
        stats: &mut ResourceStatistics,
        usage: &ResourceUsage,
        limits: &ResourceLimits,
    ) {
        stats.sample_count += 1;

        // 平均値を更新
        let n = stats.sample_count as f64;
        stats.avg_cpu_percent = (stats.avg_cpu_percent * (n - 1.0) + usage.cpu_percent) / n;
        stats.avg_memory_mb =
            ((stats.avg_memory_mb as f64 * (n - 1.0) + usage.memory_mb as f64) / n) as u64;
        stats.avg_disk_io_mbps =
            ((stats.avg_disk_io_mbps as f64 * (n - 1.0) + usage.disk_io_mbps as f64) / n) as u64;

        // 最大値を更新
        if usage.cpu_percent > stats.max_cpu_percent {
            stats.max_cpu_percent = usage.cpu_percent;
        }
        if usage.memory_mb > stats.max_memory_mb {
            stats.max_memory_mb = usage.memory_mb;
        }
        if usage.disk_io_mbps > stats.max_disk_io_mbps {
            stats.max_disk_io_mbps = usage.disk_io_mbps;
        }

        // 制限違反をカウント
        if usage.exceeds_limits(limits) {
            stats.violation_count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_cpu_percent, 80.0);
        assert_eq!(limits.max_memory_mb, 1024);
        assert_eq!(limits.max_disk_io_mbps, 100);
    }

    #[test]
    fn test_resource_limits_validation() {
        let valid_limits = ResourceLimits::new(50.0, 512, 50);
        assert!(valid_limits.validate().is_ok());

        let invalid_cpu = ResourceLimits::new(101.0, 512, 50);
        assert!(invalid_cpu.validate().is_err());

        let invalid_memory = ResourceLimits::new(50.0, 0, 50);
        assert!(invalid_memory.validate().is_err());

        let invalid_disk = ResourceLimits::new(50.0, 512, 0);
        assert!(invalid_disk.validate().is_err());
    }

    #[test]
    fn test_resource_limits_docker_conversion() {
        let limits = ResourceLimits::new(50.0, 512, 100);
        assert_eq!(limits.docker_memory_limit(), 512 * 1024 * 1024);
        assert_eq!(limits.docker_disk_write_limit(), 100 * 1024 * 1024);
        assert_eq!(limits.docker_disk_read_limit(), 100 * 1024 * 1024);
    }

    #[test]
    fn test_resource_usage_exceeds_limits() {
        let limits = ResourceLimits::new(50.0, 512, 100);

        let usage_ok = ResourceUsage {
            cpu_percent: 40.0,
            memory_mb: 400,
            disk_io_mbps: 80,
            last_updated: Some(SystemTime::now()),
        };
        assert!(!usage_ok.exceeds_limits(&limits));

        let usage_cpu_exceeded = ResourceUsage {
            cpu_percent: 60.0,
            memory_mb: 400,
            disk_io_mbps: 80,
            last_updated: Some(SystemTime::now()),
        };
        assert!(usage_cpu_exceeded.exceeds_limits(&limits));

        let usage_memory_exceeded = ResourceUsage {
            cpu_percent: 40.0,
            memory_mb: 600,
            disk_io_mbps: 80,
            last_updated: Some(SystemTime::now()),
        };
        assert!(usage_memory_exceeded.exceeds_limits(&limits));

        let usage_disk_exceeded = ResourceUsage {
            cpu_percent: 40.0,
            memory_mb: 400,
            disk_io_mbps: 120,
            last_updated: Some(SystemTime::now()),
        };
        assert!(usage_disk_exceeded.exceeds_limits(&limits));
    }

    #[test]
    fn test_resource_usage_get_violations() {
        let limits = ResourceLimits::new(50.0, 512, 100);

        let usage = ResourceUsage {
            cpu_percent: 60.0,
            memory_mb: 600,
            disk_io_mbps: 120,
            last_updated: Some(SystemTime::now()),
        };

        let violations = usage.get_violations(&limits);
        assert_eq!(violations.len(), 3);
    }

    #[test]
    fn test_resource_usage_calculate_utilization() {
        let limits = ResourceLimits::new(100.0, 1000, 100);

        let usage = ResourceUsage {
            cpu_percent: 80.0,
            memory_mb: 500,
            disk_io_mbps: 60,
            last_updated: Some(SystemTime::now()),
        };

        let utilization = usage.calculate_utilization(&limits);
        assert!((79.0..=81.0).contains(&utilization));
    }

    #[tokio::test]
    async fn test_resource_monitor_creation() {
        let limits = ResourceLimits::new(50.0, 512, 100);
        let monitor = ResourceMonitor::new("test-plugin".to_string(), limits);

        let usage = monitor.get_current_usage().await;
        assert_eq!(usage.cpu_percent, 0.0);
    }

    #[tokio::test]
    async fn test_resource_monitor_start_stop() {
        let limits = ResourceLimits::new(50.0, 512, 100);
        let monitor = ResourceMonitor::new("test-plugin".to_string(), limits);

        monitor.start().await;
        tokio::time::sleep(Duration::from_millis(100)).await;
        monitor.stop().await;

        // 監視が停止していることを確認
        let task = monitor.monitor_task.read().await;
        assert!(task.is_none());
    }

    #[tokio::test]
    async fn test_resource_statistics_default() {
        let stats = ResourceStatistics::default();
        assert_eq!(stats.sample_count, 0);
        assert_eq!(stats.violation_count, 0);
    }
}
