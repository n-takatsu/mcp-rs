//! Audit Log Analysis Types
//!
//! 監査ログ分析システムで使用する型定義

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 監査ログエントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    /// ログID
    pub id: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// ユーザーID
    pub user_id: String,
    /// アクション
    pub action: String,
    /// リソース
    pub resource: String,
    /// 詳細情報
    pub details: HashMap<String, String>,
    /// IPアドレス
    pub ip_address: Option<String>,
    /// ユーザーエージェント
    pub user_agent: Option<String>,
    /// 結果（成功/失敗）
    pub result: ActionResult,
}

/// アクション結果
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActionResult {
    /// 成功
    Success,
    /// 失敗
    Failure,
    /// 拒否
    Denied,
}

/// 権限昇格イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivilegeEscalationEvent {
    /// イベントID
    pub id: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// ユーザーID
    pub user_id: String,
    /// 元の権限
    pub from_role: String,
    /// 新しい権限
    pub to_role: String,
    /// 権限昇格タイプ
    pub escalation_type: EscalationType,
    /// リスクスコア（0-100）
    pub risk_score: u8,
    /// 説明
    pub description: String,
}

/// 権限昇格タイプ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EscalationType {
    /// ロール変更
    RoleChange,
    /// 権限追加
    PermissionGrant,
    /// 異常な権限使用
    AbnormalUsage,
    /// 横方向移動
    LateralMovement,
    /// 複数権限取得
    MultipleGrants,
}

/// データ流出イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExfiltrationEvent {
    /// イベントID
    pub id: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// ユーザーID
    pub user_id: String,
    /// データ量（バイト）
    pub data_volume: u64,
    /// データタイプ
    pub data_type: String,
    /// 流出パターン
    pub pattern: ExfiltrationPattern,
    /// リスクスコア（0-100）
    pub risk_score: u8,
    /// 説明
    pub description: String,
}

/// データ流出パターン
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ExfiltrationPattern {
    /// 大量データアクセス
    MassAccess,
    /// 異常なエクスポート
    AbnormalExport,
    /// データコピー
    DataCopy,
    /// 機密データアクセス
    SensitiveDataAccess,
    /// 異常な時間帯
    UnusualTime,
}

/// 相関イベント
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelatedEvent {
    /// イベントID
    pub id: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// 関連ログエントリ
    pub related_logs: Vec<String>,
    /// 攻撃シナリオ
    pub attack_scenario: Option<AttackScenario>,
    /// 信頼度（0-100）
    pub confidence: u8,
    /// 説明
    pub description: String,
}

/// 攻撃シナリオ
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AttackScenario {
    /// 偵察
    Reconnaissance,
    /// 初期侵入
    InitialAccess,
    /// 実行
    Execution,
    /// 永続化
    Persistence,
    /// 権限昇格
    PrivilegeEscalation,
    /// 防御回避
    DefenseEvasion,
    /// 認証情報アクセス
    CredentialAccess,
    /// 発見
    Discovery,
    /// 横方向移動
    LateralMovement,
    /// 収集
    Collection,
    /// 流出
    Exfiltration,
    /// 影響
    Impact,
}

/// アラート
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    /// アラートID
    pub id: String,
    /// タイムスタンプ
    pub timestamp: DateTime<Utc>,
    /// 深刻度
    pub severity: AlertSeverity,
    /// タイトル
    pub title: String,
    /// 説明
    pub description: String,
    /// ソース
    pub source: String,
    /// 影響を受けるリソース
    pub affected_resources: Vec<String>,
    /// 推奨アクション
    pub recommended_actions: Vec<String>,
    /// ステータス
    pub status: AlertStatus,
}

/// アラート深刻度
#[derive(Debug, Clone, Hash, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum AlertSeverity {
    /// 低
    Low,
    /// 中
    Medium,
    /// 高
    High,
    /// 緊急
    Critical,
}

/// アラートステータス
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AlertStatus {
    /// 新規
    New,
    /// 調査中
    Investigating,
    /// 対応中
    Responding,
    /// 解決済み
    Resolved,
    /// 偽陽性
    FalsePositive,
}

/// 分析結果
#[derive(Debug, Clone, Default)]
pub struct AnalysisResult {
    /// 権限昇格イベント
    pub privilege_events: Vec<PrivilegeEscalationEvent>,
    /// データ流出イベント
    pub exfiltration_events: Vec<ExfiltrationEvent>,
    /// 相関イベント
    pub correlated_events: Vec<CorrelatedEvent>,
    /// アラート生成フラグ
    pub alert_generated: bool,
}

impl AnalysisResult {
    /// アラートを生成すべきか判定
    pub fn should_generate_alert(&self) -> bool {
        !self.privilege_events.is_empty()
            || !self.exfiltration_events.is_empty()
            || self.correlated_events.iter().any(|e| e.confidence > 70)
    }

    /// 深刻度を計算
    pub fn calculate_severity(&self) -> AlertSeverity {
        let max_privilege_risk = self
            .privilege_events
            .iter()
            .map(|e| e.risk_score)
            .max()
            .unwrap_or(0);

        let max_exfiltration_risk = self
            .exfiltration_events
            .iter()
            .map(|e| e.risk_score)
            .max()
            .unwrap_or(0);

        let max_risk = max_privilege_risk.max(max_exfiltration_risk);

        match max_risk {
            0..=30 => AlertSeverity::Low,
            31..=60 => AlertSeverity::Medium,
            61..=85 => AlertSeverity::High,
            _ => AlertSeverity::Critical,
        }
    }

    /// 説明を生成
    pub fn generate_description(&self) -> String {
        let mut parts = Vec::new();

        if !self.privilege_events.is_empty() {
            parts.push(format!(
                "{}件の権限昇格イベントを検出",
                self.privilege_events.len()
            ));
        }

        if !self.exfiltration_events.is_empty() {
            parts.push(format!(
                "{}件のデータ流出パターンを検出",
                self.exfiltration_events.len()
            ));
        }

        if !self.correlated_events.is_empty() {
            parts.push(format!(
                "{}件の相関イベントを検出",
                self.correlated_events.len()
            ));
        }

        parts.join("、")
    }

    /// 影響を受けるリソースを取得
    pub fn get_affected_resources(&self) -> Vec<String> {
        let mut resources = Vec::new();

        for event in &self.privilege_events {
            resources.push(format!("user:{}", event.user_id));
        }

        for event in &self.exfiltration_events {
            resources.push(format!(
                "user:{}, data_type:{}",
                event.user_id, event.data_type
            ));
        }

        resources
    }

    /// 推奨アクションを取得
    pub fn get_recommended_actions(&self) -> Vec<String> {
        let mut actions = Vec::new();

        if !self.privilege_events.is_empty() {
            actions.push("権限変更履歴を確認してください".to_string());
            actions.push("不正な権限昇格の可能性を調査してください".to_string());
        }

        if !self.exfiltration_events.is_empty() {
            actions.push("データアクセスログを詳細に調査してください".to_string());
            actions.push("影響を受けたデータの範囲を特定してください".to_string());
        }

        if !self.correlated_events.is_empty() {
            actions.push("関連するすべてのログを確認してください".to_string());
            actions.push("攻撃シナリオの全体像を把握してください".to_string());
        }

        actions
    }
}

/// 分析統計
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisStatistics {
    /// 分析されたログ総数
    pub total_logs_analyzed: u64,
    /// 検出された権限昇格数
    pub privilege_escalations_detected: u64,
    /// 検出されたデータ流出数
    pub exfiltrations_detected: u64,
    /// 総アラート数
    pub total_alerts: u64,
    /// 高深刻度アラート数
    pub high_severity_alerts: u64,
}
