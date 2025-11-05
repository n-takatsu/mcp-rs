# mcp-rs Release Notes

## Version History (0.01 Increment Versioning)

Our project follows a detailed 0.01 increment versioning strategy to provide granular tracking of development progress and feature implementation.

### 🚀 v0.15.0 - Canary Deployment System (Current)
**Release Date:** 2025-11-05  
**Focus:** Advanced Deployment Management with Real-time Monitoring

#### 🎯 Major Features
- **Canary Deployment Manager**
  - Hash-based traffic splitting algorithm
  - Configurable traffic distribution (10%, 25%, 50%, 75%, 100%)
  - User group management with force-canary capabilities
  - Real-time metrics collection and analysis
  
- **Interactive Dashboard**
  - Terminal-based UI using `ratatui` and `crossterm`
  - 4 main tabs: Overview, Metrics, Events, Control
  - Real-time traffic visualization with success rates
  - Keyboard-driven controls for deployment management
  
- **Event-Driven Architecture**
  - Broadcast channels for real-time event streaming
  - Comprehensive event logging (50 events buffer)
  - Auto-refresh capabilities with manual override
  - Graceful shutdown handling

#### 🔧 Technical Improvements
- **Performance Optimization**
  - Traffic decision processing: <5ms per request
  - Dashboard refresh rate: 500ms for optimal UX
  - Memory-efficient event buffering
  
- **Code Quality**
  - Zero compiler warnings achieved
  - Comprehensive error handling with `Result<T, McpError>`
  - Modular architecture with clear separation of concerns
  - 100% async/await implementation

#### 🧪 Testing & Validation
- **Integration Tests**
  - Keyboard input validation test (`keyboard_test.rs`)
  - Full dashboard demo with traffic simulation
  - External input termination verification
  - Loop prevention and resource management

#### 📦 Dependencies Added
- `ratatui = "0.27"` - Terminal UI framework
- `crossterm = "0.27"` - Cross-platform terminal manipulation
- `tui-input = "0.8"` - Input handling utilities

---

### ✅ v0.14.0 - Policy Hot-Reload System (Epic #15)
**Release Date:** 2025-11-04  
**Focus:** Live Configuration Management

#### 🎯 Major Features
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

#### 📊 Performance Metrics
- **Reload Time**: 15-35ms end-to-end
- **Validation Speed**: 2-8ms per validation level
- **Memory Usage**: <5MB additional overhead
- **File Watch Latency**: <100ms detection time

#### 🧪 Testing Coverage
- 6 comprehensive integration tests
- Hot-reload stress testing with rapid file changes
- Error handling validation for malformed policies
- Performance benchmarking suite

---

## 🗓️ Upcoming Releases

### v0.16.0 - Advanced Dashboard Features (Planned: 2025-11-06)
- Real-time charts and graphs visualization
- Historical metrics with trend analysis
- Alert system integration
- Export capabilities for monitoring data

### v0.17.0 - Auto-scaling & Health Checks (Planned: 2025-11-08)
- Automatic traffic adjustment based on SLA metrics
- Health check integration for validation
- Circuit breaker pattern implementation
- Promotion criteria automation

### v0.18.0 - Multi-Environment Deployment (Planned: 2025-11-10)
- Staging → Production pipeline automation
- Environment-specific policy management
- Cross-environment metrics comparison
- Advanced rollback strategies

---

## 📋 Version Numbering Strategy

We use a **0.01 increment versioning** approach for granular development tracking:

### Version Format: `0.XX.Y`
- **Major (0)**: Pre-1.0 development phase
- **Minor (XX)**: Feature releases with significant functionality (0.01 increments)
- **Patch (Y)**: Bug fixes and minor improvements within a feature release

### Development Phases
- **v0.01.0 - v0.10.0**: Foundation and Core Protocol Implementation
- **v0.11.0 - v0.20.0**: Advanced Features and Enterprise Capabilities  
- **v0.21.0 - v0.30.0**: Cloud Integration and Scalability
- **v0.31.0 - v0.99.0**: Production Hardening and Ecosystem
- **v1.00.0+**: Production Release and Long-term Support

### Release Criteria for 0.01 Increments
Each 0.01 version must include:
1. **Functional Completeness**: All advertised features fully implemented
2. **Test Coverage**: Comprehensive integration and unit tests
3. **Documentation**: Updated README, architecture docs, and examples
4. **Performance Validation**: Benchmarking and optimization verification
5. **Code Quality**: Zero compiler warnings and clean code standards

---

## 🚀 Migration Guides

### Upgrading from v0.14.0 to v0.15.0

#### New Dependencies
Add to your `Cargo.toml`:
```toml
ratatui = "0.27"
crossterm = "0.27"
tui-input = "0.8"
```

#### API Changes
- New `CanaryDeploymentManager` for traffic management
- Dashboard integration through `run_dashboard()` function
- Enhanced event system with `CanaryEvent` types

#### Configuration Updates
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