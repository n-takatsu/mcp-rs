---
name: Bug Report
about: Create a report to help us improve
title: '[BUG] Brief description of the issue'
labels: ['bug', 'needs-triage']
assignees: ''

---

## 🐛 Bug Description
**Affected Version:** v0.XX.0 (specify the version where you found the bug)
**Severity:** [Low/Medium/High/Critical]

### Summary
A clear and concise description of what the bug is.

### Expected Behavior
A clear and concise description of what you expected to happen.

### Actual Behavior
A clear and concise description of what actually happened.

## 🔄 Reproduction Steps

### Minimal Reproduction Example
```rust
// Provide a minimal code example that reproduces the issue
```

### Step-by-Step Instructions
1. Go to '...'
2. Click on '...'
3. Execute '...'
4. See error

### Configuration
```toml
# Include relevant configuration files (mcp-config.toml, etc.)
```

## 🖥️ Environment Information

### System Details
- **OS:** [e.g., Windows 11, Ubuntu 22.04, macOS 13.0]
- **Architecture:** [e.g., x86_64, ARM64]
- **Rust Version:** [e.g., 1.75.0]
- **Shell:** [e.g., PowerShell 5.1, bash 5.1]

### Project Setup
- **Cargo.toml version:** [e.g., v0.15.0]
- **Feature flags enabled:** [e.g., default, dashboard]
- **Build mode:** [e.g., debug, release]

## 📝 Logs and Output

### Error Messages
```
Paste any error messages here
```

### Console Output
```
Paste relevant console output here
```

### Log Files
```
Paste relevant log entries here (with sensitive data removed)
```

## 🧪 Troubleshooting Attempted

### What I've Tried
- [ ] Restarted the application
- [ ] Cleared cache/temporary files
- [ ] Verified configuration files
- [ ] Checked for dependency conflicts
- [ ] Tested with minimal configuration

### Workarounds Found
Describe any temporary workarounds you've discovered.

## 💥 Impact Assessment

### Affected Features
- [ ] Canary deployment functionality
- [ ] Dashboard display
- [ ] Policy hot-reload
- [ ] Configuration management
- [ ] Other: [specify]

### User Impact
- [ ] Blocks normal operation
- [ ] Degrades performance
- [ ] Causes data loss
- [ ] Cosmetic issue only
- [ ] Other: [specify]

## 🔍 Additional Context

### Related Issues
Link to any related issues:
- Related to #XX
- Similar to #XX

### Screenshots
If applicable, add screenshots to help explain your problem.

### Regression Information
- [ ] This worked in a previous version
- [ ] Last working version: v0.XX.0
- [ ] This is a new issue

## 🚀 Fix Priority

### Urgency Level
- [ ] **Critical:** System unusable, data loss risk
- [ ] **High:** Major feature broken, significant user impact
- [ ] **Medium:** Minor feature affected, workaround available
- [ ] **Low:** Cosmetic issue, no functional impact

### Version Target
Which version should this be fixed in?
- [ ] Next patch (v0.15.1)
- [ ] Next minor (v0.16.0)
- [ ] Future release (v0.XX.0)

---

**Internal Use (Maintainers)**
- [ ] Bug confirmed and reproduced
- [ ] Root cause identified
- [ ] Fix approach determined
- [ ] Test cases created
- [ ] Fix implemented
- [ ] Regression tests added
- [ ] Documentation updated if needed