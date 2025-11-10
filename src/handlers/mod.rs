//! Handler implementations for different target systems
//!
//! このモジュールは様々な対象システムに対するMCPハンドラーを提供します。
//! 各ハンドラーは共通のMcpHandlerトレイトを実装し、プラガブルな設計となっています。

// 現在実装されているハンドラー
pub mod wordpress;

// 拡張性のための汎用システム
pub mod generic;
pub mod multi;

// データベースハンドラー
#[cfg(feature = "database")]
pub mod database;

// 将来実装予定のハンドラー
// pub mod filesystem;
// pub mod cloud;
// pub mod webapi;
// pub mod messagequeue;

pub use wordpress::*;
