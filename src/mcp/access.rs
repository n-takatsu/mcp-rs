//! 呼び出し元ごとに使えるツール（#52）
//!
//! HTTPモードで、APIキー・アクセストークンごとに、呼び出せるツールを
//! 制限する。STDIOモードは制限しない（ローカルのパイプで、使う人は1人）。

use std::sync::Arc;

use crate::mcp::Tool;

/// 読み取り専用のツール（annotationsの`readOnlyHint: true`）を表す指定
pub const READ_ONLY_PRESET: &str = "@read_only";

/// ツールの指定（`tools = [...]`の1項目）
#[derive(Debug, Clone, PartialEq, Eq)]
enum ToolPattern {
    /// `@read_only`
    ReadOnly,
    /// ツール名。`*`は任意の文字列（空も含む）に合う
    Name(String),
}

impl ToolPattern {
    fn parse(item: &str) -> Result<Self, String> {
        let item = item.trim();
        if item.is_empty() {
            return Err("tool patterns must not be empty".to_string());
        }
        if let Some(preset) = item.strip_prefix('@') {
            return match item {
                READ_ONLY_PRESET => Ok(Self::ReadOnly),
                _ => Err(format!(
                    "unknown tool preset '@{}' (available: {})",
                    preset, READ_ONLY_PRESET
                )),
            };
        }
        Ok(Self::Name(item.to_string()))
    }

    fn matches(&self, tool: &Tool) -> bool {
        match self {
            Self::ReadOnly => tool
                .annotations
                .as_ref()
                .and_then(|a| a.read_only_hint)
                .unwrap_or(false),
            Self::Name(pattern) => glob_match(pattern, &tool.name),
        }
    }
}

/// `*`だけを特別に扱う、単純なワイルドカードの照合
fn glob_match(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    // 先に前と後ろを確かめる（下で切り出す位置が、文字の境目になるように）
    if name.len() < first.len() + last.len() || !name.starts_with(first) || !name.ends_with(last) {
        return false;
    }
    let mut rest = &name[first.len()..name.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(i) => rest = &rest[i + part.len()..],
            None => return false,
        }
    }
    true
}

/// 呼び出し元が使えるツール
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum ToolAccess {
    /// すべてのツール（STDIO・制限を設定していないAPIキー等）
    #[default]
    All,
    /// 指定に合うツールだけ（指定が空なら、どのツールも使えない）
    Only(Arc<Vec<ToolPatternSet>>),
}

/// 1つの設定項目（`tools = [...]`）から作った指定の集まり
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolPatternSet(Vec<ToolPattern>);

impl ToolPatternSet {
    /// `tools = [...]`を読む。知らないプリセット・空の項目はエラー
    pub fn parse(items: &[String]) -> Result<Self, String> {
        items
            .iter()
            .map(|item| ToolPattern::parse(item))
            .collect::<Result<Vec<_>, _>>()
            .map(Self)
    }

    fn matches(&self, tool: &Tool) -> bool {
        self.0.iter().any(|p| p.matches(tool))
    }
}

impl ToolAccess {
    /// `tools`の指定から作る。`None`（省略）はすべてのツール
    pub fn from_patterns(items: Option<&[String]>) -> Result<Self, String> {
        match items {
            None => Ok(Self::All),
            Some(items) => Ok(Self::Only(Arc::new(vec![ToolPatternSet::parse(items)?]))),
        }
    }

    /// いくつかの指定のどれかに合うツール（トークンの複数のスコープ等）
    pub fn any_of(sets: Vec<ToolPatternSet>) -> Self {
        Self::Only(Arc::new(sets))
    }

    pub fn allows(&self, tool: &Tool) -> bool {
        match self {
            Self::All => true,
            Self::Only(sets) => sets.iter().any(|set| set.matches(tool)),
        }
    }
}

/// リクエストの呼び出し元
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Caller {
    /// 監査ログ・ログに出す名前（APIキーの名前、トークンの主体等）。
    /// ローカル（STDIO）・認証なしは`None`
    pub name: Option<String>,
    pub access: ToolAccess,
}

impl Caller {
    /// 制限のない呼び出し元（STDIO・認証を設定していないHTTP）
    pub fn unrestricted() -> Self {
        Self::default()
    }

    pub fn new(name: impl Into<String>, access: ToolAccess) -> Self {
        Self {
            name: Some(name.into()),
            access,
        }
    }

    /// ログ・監査ログに出す名前
    pub fn display_name(&self) -> &str {
        self.name.as_deref().unwrap_or("local")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::ToolAnnotations;

    fn tool(name: &str, read_only: Option<bool>) -> Tool {
        Tool {
            name: name.to_string(),
            annotations: read_only.map(|r| ToolAnnotations {
                read_only_hint: Some(r),
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("get_posts", "get_posts"));
        assert!(!glob_match("get_posts", "get_post"));
        assert!(glob_match("*", "anything"));
        assert!(glob_match("*", ""));
        assert!(glob_match("fs__*", "fs__read_file"));
        assert!(!glob_match("fs__*", "other__read_file"));
        assert!(glob_match("get_*", "get_"));
        assert!(glob_match("*_post", "delete_post"));
        assert!(glob_match("fs__*_file", "fs__read_text_file"));
        assert!(!glob_match("fs__*_file", "fs__list_directory"));
        assert!(glob_match("a*b*c", "abc"));
        assert!(!glob_match("a*b*c", "acb"));
        // 前と後ろが重なる場合
        assert!(!glob_match("ab*ba", "aba"));
    }

    #[test]
    fn test_tool_access() {
        let read_only = tool("get_posts", Some(true));
        let write = tool("delete_post", Some(false));
        let plain = tool("fs__read_file", None);

        assert!(ToolAccess::All.allows(&write));

        let access = ToolAccess::from_patterns(Some(&["@read_only".to_string()])).unwrap();
        assert!(access.allows(&read_only));
        assert!(!access.allows(&write));
        // annotationsの無いツールは、読み取り専用として扱わない
        assert!(!access.allows(&plain));

        let access =
            ToolAccess::from_patterns(Some(&["fs__*".to_string(), "delete_post".to_string()]))
                .unwrap();
        assert!(access.allows(&plain));
        assert!(access.allows(&write));
        assert!(!access.allows(&read_only));

        let none = ToolAccess::from_patterns(Some(&[])).unwrap();
        assert!(!none.allows(&read_only));

        assert_eq!(ToolAccess::from_patterns(None).unwrap(), ToolAccess::All);
        assert!(ToolAccess::from_patterns(Some(&["@admin".to_string()])).is_err());
        assert!(ToolAccess::from_patterns(Some(&[" ".to_string()])).is_err());

        let union = ToolAccess::any_of(vec![
            ToolPatternSet::parse(&["@read_only".to_string()]).unwrap(),
            ToolPatternSet::parse(&["delete_*".to_string()]).unwrap(),
        ]);
        assert!(union.allows(&read_only));
        assert!(union.allows(&write));
        assert!(!union.allows(&plain));
        assert!(!ToolAccess::any_of(Vec::new()).allows(&read_only));
    }
}
