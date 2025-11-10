use ratatui::{
    style::{Color, Style},
    symbols,
    widgets::{Axis, BarChart, Chart, Dataset, GraphType},
};
use std::collections::VecDeque;

/// トラフィック分散チャート
#[derive(Debug)]
pub struct TrafficChart {
    /// データポイント履歴
    data: VecDeque<(f64, f64)>, // (time, percentage)
    /// 最大データポイント数
    max_points: usize,
}

impl TrafficChart {
    pub fn new(max_points: usize) -> Self {
        Self {
            data: VecDeque::with_capacity(max_points),
            max_points,
        }
    }

    /// データポイントを追加
    pub fn add_data_point(&mut self, time: f64, percentage: f64) {
        if self.data.len() >= self.max_points {
            self.data.pop_front();
        }
        self.data.push_back((time, percentage));
    }

    /// チャートデータを取得
    pub fn get_data(&self) -> &VecDeque<(f64, f64)> {
        &self.data
    }
}

/// メトリクスチャート
#[derive(Debug)]
pub struct MetricsChart {
    /// 成功率データ
    success_rate_data: VecDeque<(f64, f64, f64)>, // (time, stable_rate, canary_rate)
    /// レスポンス時間データ
    response_time_data: VecDeque<(f64, f64, f64)>, // (time, stable_time, canary_time)
    /// 最大データポイント数
    max_points: usize,
}

impl MetricsChart {
    pub fn new(max_points: usize) -> Self {
        Self {
            success_rate_data: VecDeque::with_capacity(max_points),
            response_time_data: VecDeque::with_capacity(max_points),
            max_points,
        }
    }

    /// 成功率データを追加
    pub fn add_success_rate_data(&mut self, time: f64, stable_rate: f64, canary_rate: f64) {
        if self.success_rate_data.len() >= self.max_points {
            self.success_rate_data.pop_front();
        }
        self.success_rate_data
            .push_back((time, stable_rate, canary_rate));
    }

    /// レスポンス時間データを追加
    pub fn add_response_time_data(&mut self, time: f64, stable_time: f64, canary_time: f64) {
        if self.response_time_data.len() >= self.max_points {
            self.response_time_data.pop_front();
        }
        self.response_time_data
            .push_back((time, stable_time, canary_time));
    }

    /// 成功率データを取得
    pub fn get_success_rate_data(&self) -> &VecDeque<(f64, f64, f64)> {
        &self.success_rate_data
    }

    /// レスポンス時間データを取得
    pub fn get_response_time_data(&self) -> &VecDeque<(f64, f64, f64)> {
        &self.response_time_data
    }
}

/// イベントログ
#[derive(Debug)]
pub struct EventLog {
    /// イベント履歴
    events: VecDeque<String>,
    /// 最大イベント数
    max_events: usize,
}

impl EventLog {
    pub fn new(max_events: usize) -> Self {
        Self {
            events: VecDeque::with_capacity(max_events),
            max_events,
        }
    }

    /// イベントを追加
    pub fn add_event(&mut self, event: String) {
        if self.events.len() >= self.max_events {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }

    /// イベント履歴を取得
    pub fn get_events(&self) -> &VecDeque<String> {
        &self.events
    }

    /// 最新のイベントを取得
    pub fn get_latest_events(&self, count: usize) -> Vec<&String> {
        self.events.iter().rev().take(count).collect()
    }
}
