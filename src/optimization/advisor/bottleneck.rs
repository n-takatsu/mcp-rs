//! Bottleneck Detection Types

use serde::{Deserialize, Serialize};

/// ボトルネックタイプ
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BottleneckType {
    /// CPU
    Cpu,
    /// メモリ
    Memory,
    /// ネットワーク
    Network,
    /// データベース
    Database,
    /// ディスクI/O
    DiskIo,
}

/// ボトルネック
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bottleneck {
    /// ボトルネックタイプ
    pub bottleneck_type: BottleneckType,
    /// 深刻度（0.0-1.0）
    pub severity: f64,
    /// 説明
    pub description: String,
    /// 影響範囲
    pub impact: String,
    /// 検出メトリクス
    pub detected_value: f64,
    /// 閾値
    pub threshold: f64,
}

impl Bottleneck {
    /// 新しいボトルネックを作成
    pub fn new(
        bottleneck_type: BottleneckType,
        severity: f64,
        description: impl Into<String>,
        impact: impl Into<String>,
        detected_value: f64,
        threshold: f64,
    ) -> Self {
        Self {
            bottleneck_type,
            severity: severity.clamp(0.0, 1.0),
            description: description.into(),
            impact: impact.into(),
            detected_value,
            threshold,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bottleneck_creation() {
        let bottleneck = Bottleneck::new(
            BottleneckType::Cpu,
            0.8,
            "High CPU usage",
            "Performance degradation",
            90.0,
            80.0,
        );

        assert_eq!(bottleneck.bottleneck_type, BottleneckType::Cpu);
        assert_eq!(bottleneck.severity, 0.8);
    }
}
