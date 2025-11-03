# mcp-rs Documentation

## 📚 Documentation Structure

### 📖 Project Documentation (`project-docs/`)

#### [`architecture.md`](architecture.md)
- **Purpose**: System architecture and design patterns
- **Audience**: Developers, contributors, system architects
- **Content**: Layered architecture, plugin system, error handling, security, performance

#### [`wordpress-guide.md`](wordpress-guide.md) 
- **Purpose**: Complete WordPress integration guide
- **Audience**: Users, integrators, AI agents
- **Content**: Setup, features, examples, troubleshooting, workflows

#### [`api-reference.md`](api-reference.md)
- **Purpose**: Quick API reference for all 27 tools
- **Audience**: Developers, API consumers, AI agents
- **Content**: Tool parameters, response formats, examples, error codes

#### [`index.md`](index.md) *(This file)*
- **Purpose**: Documentation navigation and structure
- **Audience**: All users
- **Content**: Documentation overview and maintenance guidelines

## 🎯 Recommended Reading Order

### For New Users
1. **[README.md](../README.md)** - Project overview and features
2. **[wordpress-guide.md](wordpress-guide.md)** - Complete setup and usage
3. **[api-reference.md](api-reference.md)** - Quick API lookup

### For Developers & Contributors
1. **[architecture.md](architecture.md)** - System design and patterns
2. **[wordpress-guide.md](wordpress-guide.md)** - Implementation details and examples
3. **[Contributing Guidelines](../README.md#contributing)** - Development process

### For AI Agents
1. **[api-reference.md](api-reference.md)** - Tool parameters and examples
2. **[wordpress-guide.md](wordpress-guide.md)** - Advanced features and workflows
3. **[architecture.md](architecture.md)** - System understanding for complex tasks

## 🧹 Cleanup Status

### ✅ Completed Restructuring
- [x] `docs/wordpress-tools.md` - **REMOVED** (Content merged into wordpress-guide.md)
- [x] `docs/wordpress-advanced.md` - **REMOVED** (Content merged into wordpress-guide.md)
- [x] `project-docs/architecture.md` (duplicate) - **REMOVED** 
- [x] `docs/architecture.md` - **MOVED** to `project-docs/architecture.md`
- [x] `docs/` folder - **REMOVED** (empty after consolidation)

### Final Simplified Structure
```
mcp-rs/
├── README.md                    # Main project introduction
├── project-docs/               # All documentation in one place
│   ├── index.md                # Documentation navigation (this file)
│   ├── architecture.md         # System design and patterns
│   ├── wordpress-guide.md      # Complete WordPress integration
│   └── api-reference.md        # Quick API reference
└── examples/                   # Working code examples
    ├── wordpress_test.rs
    ├── wordpress_post_crud_test.rs
    ├── wordpress_media_crud_test.rs
    ├── wordpress_embed_test.rs
    ├── wordpress_advanced_post_test.rs
    ├── wordpress_categories_tags_test.rs
    └── wordpress_posts_with_taxonomy_test.rs
```

### Benefits of New Structure
- **🎯 Single Documentation Location**: All docs in `project-docs/`
- **📚 Simpler Navigation**: No need to jump between `docs/` and `project-docs/`
- **🎨 Clear Purpose**: Each document has distinct, focused content
- **🔍 Better Discoverability**: Related documents grouped together
- **🛠️ Easier Maintenance**: Single location for all documentation updates

## 📝 Documentation Maintenance

### Update Triggers
- **New WordPress tools**: Update api-reference.md and wordpress-guide.md
- **Architecture changes**: Update architecture.md
- **New examples**: Reference in wordpress-guide.md
- **Configuration changes**: Update wordpress-guide.md setup section

### Quality Standards
- **Consistency**: Use same terminology across all docs
- **Examples**: Include working JSON examples for all tools
- **Accessibility**: Document accessibility features prominently
- **Error Handling**: Include error scenarios and solutions
- **Performance**: Document performance considerations

### Review Checklist
- [ ] All 27 tools documented
- [ ] Examples are current and working
- [ ] Configuration is up-to-date
- [ ] Links are valid
- [ ] Content is not duplicated
- [ ] Accessibility features highlighted
- [ ] Security best practices included

## 🔗 External References

- [WordPress REST API Documentation](https://developer.wordpress.org/rest-api/)
- [Model Context Protocol Specification](https://spec.modelcontextprotocol.io/)
- [JSON-RPC 2.0 Specification](https://www.jsonrpc.org/specification)
- [Rust Documentation](https://doc.rust-lang.org/)
- [WCAG Accessibility Guidelines](https://www.w3.org/WAI/WCAG21/quickref/)

## 📊 Documentation Metrics

### Coverage
- **WordPress Tools**: 27/27 documented ✅
- **Examples**: 7 comprehensive examples ✅
- **Configuration**: Complete setup guide ✅
- **Troubleshooting**: Common issues covered ✅

### Completeness
- **Setup Instructions**: ✅ Complete
- **API Reference**: ✅ All tools covered
- **Error Handling**: ✅ Comprehensive
- **Performance**: ✅ Best practices documented
- **Security**: ✅ Guidelines included
- **Accessibility**: ✅ Features highlighted

---

*Last updated: 2024-11-03*  
*Documentation version: 1.0.0*  
*mcp-rs version: 0.1.0-alpha*