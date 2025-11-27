# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added - RBAC Implementation (Issue #74)

- **Role-Based Access Control (RBAC)** (`src/handlers/database/advanced_security.rs`)
  - `RoleBasedAccessControl` with role hierarchy and inheritance
  - User role management (assign, revoke, get roles)
  - Permission caching for optimized access checks
  - Comprehensive access decision engine

- **Advanced Access Controls**
  - Condition evaluation engine with 6 condition types:
    - TimeOfDay, DayOfWeek, IpAddress
    - UserAttribute, DataSensitivity, QueryComplexity
  - 9 comparison operators (Equals, NotEquals, Contains, GreaterThan, LessThan, Between, In, NotIn, Regex)
  - Time-based access control:
    - Business hours per weekday (Monday-Sunday)
    - Emergency access configuration
    - Break period restrictions
    - Timezone support (defaults to "UTC")
  - IP restrictions:
    - CIDR notation support via `ipnet` crate
    - Role-based IP ranges
    - VPN requirement enforcement
    - Geo-blocking configuration

- **Resource-Level Security**
  - Column-level permissions:
    - Read/Write role assignments
    - Data masking rules integration
    - Encryption requirements
  - Data masking (4 types):
    - **Full**: Complete redaction to "***"
    - **Partial**: Configurable reveal (first/last N characters)
    - **Hash**: SHA-256 hashing via `sha2` crate
    - **Tokenize**: Random token generation via `rand` crate
  - Row-level security:
    - Policy column enforcement (e.g., owner_id)
    - User attribute matching
    - Admin bypass capability

- **IntegratedSecurityManager Integration** (`src/handlers/database/integrated_security.rs`)
  - Enhanced `check_authentication_and_authorization` with RBAC
  - Query type to action mapping (Select→Read, Insert/Update→Write, Delete→Delete, DDL→Admin)
  - Public RBAC APIs:
    - `assign_user_role`, `revoke_user_role`, `update_rbac_config`
    - `check_column_access`, `check_row_level_security`
    - `apply_data_masking`, `get_user_roles`

- **Comprehensive Test Suite** (15 RBAC tests)
  - Basic RBAC operations (role assignment, hierarchy)
  - Condition evaluation (all 6 types + 9 operators)
  - Time-based access control scenarios
  - IP restriction validation with CIDR
  - Column-level permission enforcement
  - Data masking (all 4 types)
  - Row-level security policies

### Security

- **7-Layer Security Architecture**: Now includes RBAC as the primary access control layer
- **Data Masking**: 4 masking strategies for PII/PHI protection
- **Fine-Grained Access Control**: Column and row-level security policies
- **Time-Based Security**: Business hours and emergency access support
- **Network Security**: IP-based access restrictions with CIDR

### Dependencies

- Added `ipnet = "2.10"` for CIDR notation IP range validation
- Added `sha2 = "0.10"` for SHA-256 hashing in data masking
- Added `rand = "0.8"` for cryptographically secure token generation

### Quality Assurance

- 15/15 RBAC tests passing (100%)
- 133 library tests total (100% passing)
- 44 compatibility tests (100% passing)
- Zero Clippy warnings
- Zero compiler errors

### Documentation

- Updated README.md: 6-Layer → 7-Layer Security Architecture
- Enhanced module documentation with comprehensive feature lists
- Added detailed API documentation for RBAC methods

### Added - MySQL Phase 1 Security Enhancement

- **Parameterized Query Support** (`src/handlers/database/engines/mysql/prepared.rs`)
  - `MySqlPreparedStatement` struct for type-safe parameter binding
  - SQL injection prevention through parameter separation
  - Support for all MySQL data types (NULL, Bool, Int, Float, String, DateTime, Binary)
  - Automatic row conversion to internal QueryResult format

- **Transaction Management** (`src/handlers/database/engines/mysql/transaction.rs`)
  - `MySqlTransactionManager` for transaction lifecycle management
  - `MySqlTransaction` context for ACID-compliant operations
  - Full support for transaction isolation levels:
    - READ UNCOMMITTED
    - READ COMMITTED
    - REPEATABLE READ
    - SERIALIZABLE
  - Savepoint functionality:
    - Named savepoint creation
    - Partial rollback to savepoint
    - Savepoint release and cleanup
  - Automatic rollback on transaction drop with warning

- **Trait Extensions** (`src/handlers/database/engine.rs`)
  - `parameter_count()` method for PreparedStatement trait
  - `get_sql()` method for PreparedStatement trait
  - Default implementations for backward compatibility

- **Comprehensive Test Suite** (45 tests - 2,140 lines)
  - Basic functionality tests (21 tests):
    - Parameter counting and validation
    - SQL injection prevention scenarios
    - Complex query handling
    - Isolation level support
    - Savepoint operations
    - Data type handling
    - Unicode and special character support
    - Performance metrics
  - Integration tests (24 tests):
    - Prepared statement lifecycle
    - Transaction workflows
    - Savepoint scenarios
    - 4 SQL injection attack vectors
    - Data integrity validation
    - Failure recovery
    - Concurrent access patterns

### Security

- **SQL Injection Prevention**: All 4 major attack vectors tested and blocked
- **Transaction Isolation**: 4-level isolation support validated
- **Data Type Safety**: Type-safe conversion between Rust and MySQL
- **Error Handling**: Comprehensive error propagation and recovery

### Performance

- Parameter conversion: ~164µs for 1000 SQL statements
- Batch operation handling: Successfully tested with 10,000 operations
- Savepoint management: Successfully tested with 100+ savepoints

### Quality Assurance

- 45/45 tests passing (100%)
- Zero Clippy warnings
- Zero compiler errors
- Full backward compatibility
- No breaking changes

## [0.15.0] - 2025-11-08

## 🚀 Major Release: ユーザーフレンドリーな設定管理システム

### Added

- **対話的設定セットアップシステム** (`--setup-config`)
  - WordPress接続テスト付きの設定ウィザード
  - リアルタイム接続検証とエラー診断
  - crossterm使用のクロスプラットフォーム対応UI
  - スピナーアニメーションとカラー出力
- **動的設定管理システム**
  - `--switch-config`: 実行時設定ファイル切り替え
  - `--config <file>`: カスタム設定ファイル指定
  - `--reload-config`: 動的設定再読み込み
  - 設定ファイル自動検出機能
- **デモンストレーションモード** (`--demo-setup`)
  - 安全なテスト環境での機能体験
  - デモ設定ファイル自動生成
- **包括的ヘルプシステム** (`--help`)
  - 全オプションの詳細説明と使用例
  - 設定ファイル検索順序の明示
- **GitHub Pages統合**
  - 美しいランディングページ (index.html)
  - Jekyll設定の最適化とpermalink構造
  - 404エラーの完全修正

### Enhanced

- **ユーザビリティの大幅向上**
  - 設定ファイル不存在時の自動セットアップ起動
  - 分かりやすい日本語エラーメッセージ
  - 初心者から上級者まで対応の段階的ガイダンス
- **ロバストな入力処理**
  - EOF検出と再試行制限によるパイプ入力対応
  - 非対話環境での自動フォールバック
  - 入力ストリーム終了の適切な処理

### Changed

- **Breaking Changes Resolved**: Complete migration to latest API versions
  - axum 0.8 WebSocket API with `.into()` conversions
  - secrecy 0.10 Secret types (Secret<String> → SecretString)
  - jsonwebtoken 10.0 new crypto backend support
  - criterion 0.7 black_box API migration
  - serde_yaml → serde_yaml_ng transition
- **Performance Improvements**: Significant performance gains across all subsystems
  - 15-20% faster async runtime (tokio 1.48)
  - 20-30% improved HTTP throughput (axum 0.8 + hyper 1.6)
  - 10-20% faster cryptographic operations
  - Enhanced DNS resolution and network performance

### Security Enhancements

- **Enhanced Cryptography**: Latest security algorithms and implementations
  - jsonwebtoken 10.x with improved crypto backends
  - secrecy 0.10 with strengthened Secret management
  - ring 0.17.8 cryptographic optimizations
- **Vulnerability Management**: RUSTSEC-2023-0071 properly managed
  - No actual security impact (unused dependency chain)
  - Alternative secure MySQL implementation via mysql_async
  - Comprehensive audit trail documentation

### Technical Updates

- **Quality Assurance**: 356+ tests passing with zero warnings
  - Complete test suite modernization
  - Strict clippy compliance (-D warnings)
  - Cargo fmt standardization
  - Release build optimization
- **Documentation**: Comprehensive guides and references
  - Updated API documentation
  - Migration guides for breaking changes
  - Production deployment guides
  - Security configuration examples

## [0.1.0-alpha] - 2025-11-04

### Added

- **WordPress Integration**: Complete WordPress REST API integration with 27 tools
  - Advanced post/page management with SEO integration
  - Complete media management with base64 upload support
  - Category and tag management with hierarchical support
  - YouTube and social media embed support
  - Comment management and retrieval
- **Enterprise Security**: 6-layer security architecture (100% implemented)
  - AES-GCM-256 encryption with PBKDF2 key derivation
  - SQL injection protection (11 attack patterns)
  - XSS attack prevention (14 attack patterns)
  - Token bucket rate limiting with DDoS protection
  - TLS 1.2+ enforcement
  - Comprehensive audit logging
- **Core Infrastructure**:
  - JSON-RPC 2.0 server implementation using axum
  - Type-safe TOML configuration with environment variable override
  - Comprehensive error handling with thiserror
  - Async/await support with tokio runtime
  - Production-ready logging with tracing
- **Documentation**:
  - Complete README with usage examples
  - Technical documentation in project-docs/
  - GitHub Pages website preparation
  - Contributing guidelines and code of conduct
- **Testing**: 205+ comprehensive tests with 100% pass rate
- **Security Features**:
  - Zero-panic operations with Result-based error handling
  - Safe environment variable expansion with infinite loop prevention
  - Application password lifecycle management
  - Production monitoring and health checks

### Security

- Implemented military-grade AES-GCM-256 encryption
- Added comprehensive input validation and sanitization
- Enabled zero-trust data validation architecture
- Implemented real-time security monitoring

### Technical

- Built with Rust 2021 edition for memory safety
- Async-first architecture using tokio
- Clean layered architecture with separation of concerns
- Production-optimized build profiles

## Documentation

- Comprehensive API documentation for all 27 WordPress tools
- Security implementation guide with examples
- Architecture documentation with design decisions
- Complete setup and deployment guides

## [0.0.0] - 2025-10-01

### Initial Setup

- Initial project setup
- Basic project structure
- License files (MIT/Apache-2.0)

[Unreleased]: https://github.com/n-takatsu/mcp-rs/compare/v0.1.0-alpha...HEAD
[0.1.0-alpha]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.1.0-alpha
[0.0.0]: https://github.com/n-takatsu/mcp-rs/releases/tag/v0.0.0
