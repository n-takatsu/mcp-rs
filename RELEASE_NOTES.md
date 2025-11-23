# mcp-rs Release Notes

## Version History (0.01 Increment Versioning)

Our project follows a detailed 0.01 increment versioning strategy to provide granular tracking of development progress and feature implementation.

## 🚀 v0.15.0 - ユーザーフレンドリーな設定管理システム

**Release Date:** 2025-11-08  
**Focus:** 初心者から上級者まで使いやすい包括的な設定管理とセットアップ体験

### 🎯 Major Features

#### 🔧 対話的設定セットアップ

- **`--setup-config`**: WordPress接続テスト付きの対話的設定作成ウィザード
- **`--demo-setup`**: 安全なデモンストレーションモードでの機能体験
- **リアルタイム接続検証**: WordPress API接続の即座テストと詳細エラー診断
- **クロスプラットフォーム対応**: crossterm使用のターミナルUI

#### 🔄 動的設定管理システム

- **`--switch-config`**: 実行時の設定ファイル動的切り替え
- **`--config <file>`**: カスタム設定ファイルの指定起動
- **`--reload-config`**: 設定の動的再読み込み（実行中サーバー向け）
- **設定ファイル自動検出**: `mcp-config.toml` → `config.toml` → `config/mcp.toml`

#### 🤖 自動化とユーザビリティ

- **設定ファイル不存在時の自動セットアップ起動**
- **包括的なヘルプシステム** (`--help`)
- **WordPress接続テスト機能** - 認証情報の即座検証
- **分かりやすい日本語エラーメッセージ**

### 🔧 Technical Improvements

#### 🏗️ 新しいモジュール構造

- **`src/setup/`**: 対話的セットアップシステム
  - `ui.rs`: crossterm使用のターミナルUI実装
  - `validator.rs`: WordPress接続検証とテスト
  - `demo.rs`: 安全なデモンストレーション環境
- **`src/config/dynamic.rs`**: 動的設定管理とリアルタイム切り替え

#### 🛡️ ロバストな入力処理

- **EOF検出と再試行制限**: パイプ入力時の無限ループ防止
- **入力ストリーム終了の適切な処理**
- **非対話環境での自動フォールバック**

#### 🎨 ユーザーエクスペリエンス向上

- **スピナーアニメーション**: 接続テスト中の視覚的フィードバック
- **カラフルなターミナル出力**: 成功/エラー/警告の色分け表示
- **プログレス表示**: セットアップ進行状況の明確な表示

### 🌐 ドキュメントとGitHub Pages

- **美しいランディングページ**: レスポンシブデザインのindex.html
- **GitHub Pages 404エラー修正**: 適切なJekyll設定とpermalink構造
- **包括的なREADME更新**: 動的設定管理機能の詳細説明

### 🧪 Testing & Validation

- **フォーマットチェック完全対応**: `cargo fmt --all -- --check` 通過
- **Clippy警告完全修正**: 冗長なクロージャの最適化
- **全コマンドオプションの動作確認**: `--setup-config`, `--switch-config`, etc.
- **WordPress接続テスト**: 実際のAPI接続による検証

### 📦 Dependencies Added

- `ratatui = "0.27"` - Terminal UI framework
- `crossterm = "0.27"` - Cross-platform terminal manipulation
- `tui-input = "0.8"` - Input handling utilities

---

## ✅ v0.14.0 - Policy Hot-Reload System (Epic #15)

**Release Date:** 2025-11-04  
**Focus:** Live Configuration Management

### 🎯 Major Features

- **Real-time Policy Monitoring**
  - File system watcher with debouncing (200ms)
  - Automatic detection of `.toml` policy changes
  - Non-blocking reload operations
  
- **4-Level Validation Pipeline**
  1. **Syntax Validation**: TOML parsing and structure verification
  2. **Semantic Validation**: Business logic and constraint checking
  3. **Security Validation**: Security rule verification and threat detection
  4. **Integration Validation**: Cross-component compatibility testing
  
- **Policy Application Engine**
  - Diff-based policy updates for minimal disruption
  - Rollback capabilities on validation failures
  - Comprehensive audit logging for all changes

### 📊 Performance Metrics

- **Reload Time**: 15-35ms end-to-end
- **Validation Speed**: 2-8ms per validation level
- **Memory Usage**: <5MB additional overhead
- **File Watch Latency**: <100ms detection time

### 🧪 Testing Coverage

- 6 comprehensive integration tests
- Hot-reload stress testing with rapid file changes
- Error handling validation for malformed policies
- Performance benchmarking suite

---

## 🗓️ Upcoming Releases

## v0.16.0 - Advanced Dashboard Features (Planned: 2025-11-06)

- Real-time charts and graphs visualization
- Historical metrics with trend analysis
- Alert system integration
- Export capabilities for monitoring data

## v0.17.0 - Auto-scaling & Health Checks (Planned: 2025-11-08)

- Automatic traffic adjustment based on SLA metrics
- Health check integration for validation
- Circuit breaker pattern implementation
- Promotion criteria automation

## v0.18.0 - Multi-Environment Deployment (Planned: 2025-11-10)

- Staging → Production pipeline automation
- Environment-specific policy management
- Cross-environment metrics comparison
- Advanced rollback strategies

---

## 📋 Version Numbering Strategy

We use a **0.01 increment versioning** approach for granular development tracking:

## Version Format: `0.XX.Y`

- **Major (0)**: Pre-1.0 development phase
- **Minor (XX)**: Feature releases with significant functionality (0.01 increments)
- **Patch (Y)**: Bug fixes and minor improvements within a feature release

## Development Phases

- **v0.01.0 - v0.10.0**: Foundation and Core Protocol Implementation
- **v0.11.0 - v0.20.0**: Advanced Features and Enterprise Capabilities  
- **v0.21.0 - v0.30.0**: Cloud Integration and Scalability
- **v0.31.0 - v0.99.0**: Production Hardening and Ecosystem
- **v1.00.0+**: Production Release and Long-term Support

## Release Criteria for 0.01 Increments

Each 0.01 version must include:
1. **Functional Completeness**: All advertised features fully implemented
2. **Test Coverage**: Comprehensive integration and unit tests
3. **Documentation**: Updated README, architecture docs, and examples
4. **Performance Validation**: Benchmarking and optimization verification
5. **Code Quality**: Zero compiler warnings and clean code standards

---

## 🚀 Migration Guides

## Upgrading from v0.14.0 to v0.15.0

### New Dependencies

Add to your `Cargo.toml`:
```toml
ratatui = "0.27"
crossterm = "0.27"
tui-input = "0.8"
```

### API Changes

- New `CanaryDeploymentManager` for traffic management
- Dashboard integration through `run_dashboard()` function
- Enhanced event system with `CanaryEvent` types

### Configuration Updates

No breaking changes to existing policy configuration files.

---

## 📞 Support & Feedback

For questions about specific versions or upgrade assistance:
- **GitHub Issues**: [mcp-rs/issues](https://github.com/n-takatsu/mcp-rs/issues)
- **Discussions**: [mcp-rs/discussions](https://github.com/n-takatsu/mcp-rs/discussions)
- **Documentation**: [project-docs/](project-docs/)

---

*Last Updated: 2025-11-05*  
*Current Version: v0.15.0*