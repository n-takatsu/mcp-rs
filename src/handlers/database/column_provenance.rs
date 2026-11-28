//! SQLの文を、実行する前に`sqlparser`で読んで確かめる。データベースには
//! つながず、文だけを見る。
//!
//! - [`validate_single_query_statement`]・[`validate_single_write_statement`]:
//!   `execute_query`・`execute_command`が受け付ける文か（1つのSELECT、1つの
//!   INSERT・UPDATE・DELETE）
//! - [`resolve_query_tables`]・[`resolve_write_tables`]: 文が触れる表
//!   （マイクロセグメンテーションが、表ごとの許可を確かめる）
//!
//! 確かに読めない形は、[`ProvenanceError`]で断る。呼ぶ側は、そのとき文を
//! 実行しない（安全の境界なので、分からないものは通さない）。
//!
//! 以前は、SELECTの結果の列がどの表の列かを調べる`resolve_column_provenance`も
//! あった（列の暗号化が使った）。列の暗号化と一緒に外した（#768）。

use super::types::DatabaseType;
use sqlparser::ast::{
    AssignmentTarget, Expr, FromTable, Ident, ObjectName, ObjectNamePart, Query, SetExpr,
    Statement, TableFactor, TableObject, TableWithJoins,
};
use sqlparser::dialect::{
    ClickHouseDialect, Dialect, GenericDialect, MySqlDialect, PostgreSqlDialect, SQLiteDialect,
};
use sqlparser::parser::Parser;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};
use std::fmt;

/// Picks the `sqlparser` dialect matching a `DatabaseType`, so SQL-shape
/// validation actually understands the syntax of the engine it's guarding.
/// Parsing every engine's SQL as if it were PostgreSQL would misparse (or
/// wrongly reject) valid MySQL/SQLite/etc syntax.
pub fn dialect_for(database_type: DatabaseType) -> Box<dyn Dialect + Send> {
    match database_type {
        DatabaseType::PostgreSQL => Box::new(PostgreSqlDialect {}),
        DatabaseType::MySQL | DatabaseType::MariaDB => Box::new(MySqlDialect {}),
        DatabaseType::SQLite => Box::new(SQLiteDialect {}),
        DatabaseType::ClickHouse => Box::new(ClickHouseDialect {}),
        // MongoDB/Redis aren't SQL engines at all; execute_query's SQL-shape
        // validation doesn't meaningfully apply, but a generic dialect is a
        // safe fallback rather than assuming Postgres syntax.
        DatabaseType::MongoDB | DatabaseType::Redis => Box::new(GenericDialect {}),
    }
}

/// The query shape wasn't one this module is willing to reason about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProvenanceError(String);

impl fmt::Display for ProvenanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot determine column provenance: {}", self.0)
    }
}

impl std::error::Error for ProvenanceError {}

fn unsupported(msg: impl Into<String>) -> ProvenanceError {
    ProvenanceError(msg.into())
}

/// `sql`を文に分ける。MySQLの方言では、MariaDBだけが実行するコメント
/// （`/*M! … */`・`/*M!100100 … */`）があれば拒否する（PR #573のレビュー）。
/// sqlparserは、`/*! … */`の中身は読むが、`/*M! … */`はただのコメントと
/// して捨てる。そのため、`SELECT 1 /*M! INTO OUTFILE '/tmp/x' */`が検査を
/// 通り、MariaDBではファイルに書き出してしまう。`engine = "mysql"`のまま
/// MariaDBにつなげるので、MySQLの方言では、いつも拒否する。
fn parse_statements(sql: &str, dialect: &dyn Dialect) -> Result<Vec<Statement>, ProvenanceError> {
    let parse_error = |e: &dyn fmt::Display| unsupported(format!("SQL parse error: {e}"));
    if dialect.is::<MySqlDialect>() {
        let tokens = Tokenizer::new(dialect, sql)
            .tokenize()
            .map_err(|e| parse_error(&e))?;
        let mariadb_code = tokens.iter().any(|token| {
            matches!(
                token,
                Token::Whitespace(Whitespace::MultiLineComment(text))
                    if text.get(..2).is_some_and(|start| start.eq_ignore_ascii_case("m!"))
            )
        });
        if mariadb_code {
            return Err(unsupported(
                "MariaDB executable comments (/*M! ... */) are not allowed",
            ));
        }
        // MySQLのヒント`SET_VAR(変数=値)`は、その文の間だけセッションの
        // 変数を変える（`foreign_key_checks=0`で外部キーの確認を外す等）。
        // 検査はヒントをコメントとして読むので、断る（PR #610のレビュー）
        let set_var = tokens.iter().any(|token| {
            matches!(
                token,
                Token::Whitespace(Whitespace::MultiLineComment(text))
                    if text.starts_with('+') && text.to_ascii_lowercase().contains("set_var")
            )
        });
        if set_var {
            return Err(unsupported(
                "the optimizer hint SET_VAR is not allowed: it changes session settings for the statement",
            ));
        }
    }
    if dialect.is::<PostgreSqlDialect>() {
        // PostgreSQLの`U&"…"`（Unicodeのエスケープで書いた名前）は、
        // `U&"pg\005Fread_file"`が`pg_read_file`になる。sqlparserは、これを
        // 列`U`と名前の`&`と読むので、関数の検査を通ってしまう。ふつうの
        // SQLでは使わないので、断る（PR #610のレビュー）
        let tokens = Tokenizer::new(dialect, sql)
            .tokenize()
            .map_err(|e| parse_error(&e))?;
        let unicode_identifier = tokens.windows(3).any(|window| {
            matches!(
                window,
                [Token::Word(u), Token::Ampersand, Token::Word(quoted)]
                    if u.quote_style.is_none()
                        && u.value.eq_ignore_ascii_case("u")
                        && quoted.quote_style == Some('"')
            )
        });
        if unicode_identifier {
            return Err(unsupported(
                "Unicode escape identifiers (U&\"...\") are not allowed",
            ));
        }
    }
    Parser::parse_sql(dialect, sql).map_err(|e| parse_error(&e))
}

/// Validates that `sql` is exactly one read-only query statement - a plain
/// `SELECT` or any SELECT-family construct (`UNION`, a CTE, etc) - rejecting
/// multiple statements or any non-query statement (`INSERT`/`UPDATE`/
/// `DELETE`/DDL/etc). This function enforces the `execute_query` tool's own
/// "SELECT statements only" contract unconditionally.
pub fn validate_single_query_statement(
    sql: &str,
    dialect: &dyn Dialect,
) -> Result<(), ProvenanceError> {
    let statements = parse_statements(sql, dialect)?;

    match statements.as_slice() {
        [statement @ Statement::Query(query)] if is_read_only_query(query) => {
            refuse_side_effects(statement, true)
        }
        [_] => Err(unsupported("execute_query only supports SELECT statements")),
        _ => Err(unsupported("expected exactly one SQL statement")),
    }
}

/// `execute_command`の文が、1つのINSERT・UPDATE・DELETEか（#453）。表の
/// 作成・削除（DDL）、権限の変更、`COPY`等は受け付けない。
pub fn validate_single_write_statement(
    sql: &str,
    dialect: &dyn Dialect,
) -> Result<(), ProvenanceError> {
    let statements = parse_statements(sql, dialect)?;
    match statements.as_slice() {
        [statement @ (Statement::Insert(_) | Statement::Update { .. } | Statement::Delete(_))] => {
            refuse_side_effects(statement, false)
        }
        [_] => Err(unsupported(
            "execute_command only supports INSERT, UPDATE, and DELETE statements",
        )),
        _ => Err(unsupported("expected exactly one SQL statement")),
    }
}

/// データベースのサーバーのファイル・ほかの接続に触れる関数（#597）。
/// 読み取り専用のトランザクションでは止まらない（表への書き込みではない
/// ため）。権限（MySQLの`FILE`、PostgreSQLの`pg_read_server_files`等）を
/// 与えないのが本来の守りで、これは念のための2つ目の守り
const SERVER_SIDE_FUNCTIONS: &[&str] = &[
    // MySQL・MariaDB: サーバーのファイルを読む
    "load_file",
    // MySQL・MariaDB: よく入れられるUDF（lib_mysqludf_sys）。OSのコマンドを動かす
    "sys_exec",
    "sys_eval",
    // PostgreSQL: サーバーのファイル・フォルダを読む
    "pg_read_file",
    "pg_read_binary_file",
    "pg_ls_dir",
    "pg_stat_file",
    // PostgreSQL: サーバーのファイルと大きなオブジェクトの間で写す
    "lo_import",
    "lo_export",
    // PostgreSQL: ほかの接続を切る・止める。同じロールの接続なら、特別な
    // 権限が無くても切れる（PR #610のレビュー）
    "pg_terminate_backend",
    "pg_cancel_backend",
    // SQLite: 共有ライブラリを読み込む（#528）。SQLiteの既定では使えない
    // （mcp-rsは有効にしない）が、念のため
    "load_extension",
    // SQLite: FTS3の分割器の関数の場所を読み書きする（#528。古い版の
    // 脆弱性の入り口。今の既定では無効）
    "fts3_tokenizer",
];

/// 名前がこれで始まる関数も断る（#597、PR #610のレビュー）
const SERVER_SIDE_FUNCTION_PREFIXES: &[&str] = &[
    // PostgreSQL: dblinkは別の接続を開くので、読み取り専用のトランザクションの
    // 外で書き込める（dblink・dblink_exec・dblink_connect・dblink_open等）
    "dblink",
    // PostgreSQL: サーバーのフォルダの一覧（pg_ls_dir・pg_ls_logdir・
    // pg_ls_waldir等）
    "pg_ls_",
];

/// 文の中の、副作用のあるもの（#597）。`execute_query`（`read_only`）では、
/// 行の鍵（`FOR UPDATE`・`FOR SHARE`等）も断る。読み取りの道具なのに、
/// ほかの書き込みを待たせられるため（PostgreSQLは読み取り専用の
/// トランザクションで拒否するが、MySQLは通すことがある）。どちらの道具でも、
/// `SERVER_SIDE_FUNCTIONS`の関数を断る。副問い合わせの中も見る
fn refuse_side_effects(statement: &Statement, read_only: bool) -> Result<(), ProvenanceError> {
    use sqlparser::ast::{Visit, Visitor};
    use std::ops::ControlFlow;

    struct Finder {
        read_only: bool,
    }
    impl Visitor for Finder {
        type Break = String;
        fn pre_visit_query(&mut self, query: &Query) -> ControlFlow<String> {
            if self.read_only && !query.locks.is_empty() {
                return ControlFlow::Break(
                    "execute_query does not take row locks (FOR UPDATE, FOR SHARE)".to_string(),
                );
            }
            ControlFlow::Continue(())
        }
        fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<String> {
            match expr {
                Expr::Function(function) => check_function(&function.name),
                _ => ControlFlow::Continue(()),
            }
        }
        // `FROM dblink(...)`のように、表として呼ぶ関数
        fn pre_visit_table_factor(&mut self, table: &TableFactor) -> ControlFlow<String> {
            match table {
                TableFactor::Table {
                    name,
                    args: Some(_),
                    ..
                }
                | TableFactor::Function { name, .. } => check_function(name),
                _ => ControlFlow::Continue(()),
            }
        }
    }
    fn check_function(name: &ObjectName) -> ControlFlow<String> {
        let name = name
            .0
            .last()
            .and_then(|part| part.as_ident())
            .map(|ident| ident.value.to_ascii_lowercase());
        let refused = |n: &String| {
            SERVER_SIDE_FUNCTIONS.contains(&n.as_str())
                || SERVER_SIDE_FUNCTION_PREFIXES
                    .iter()
                    .any(|prefix| n.starts_with(prefix))
        };
        match name.filter(refused) {
            Some(name) => ControlFlow::Break(format!(
                "the function {name}() is not allowed: it reaches files or connections on the database server"
            )),
            None => ControlFlow::Continue(()),
        }
    }
    match statement.visit(&mut Finder { read_only }) {
        ControlFlow::Break(reason) => Err(unsupported(reason)),
        ControlFlow::Continue(()) => Ok(()),
    }
}

/// `query`が、`WITH`の中も含めてSELECTだけか（#453）。PostgreSQLでは
/// `WITH d AS (DELETE FROM t RETURNING *) SELECT * FROM d`のように、`WITH`の
/// 中に書き込む文を書ける。本体（`body`）だけを見ると、これをSELECTとして
/// 通してしまう。`SELECT … INTO 表`（表を作る）も拒否する。
fn is_read_only_query(query: &Query) -> bool {
    is_read_only_with(query) && is_select_family(&query.body) && !selects_into(&query.body)
}

/// `query`の`WITH`の中が、読むだけか
fn is_read_only_with(query: &Query) -> bool {
    query.with.as_ref().is_none_or(|with| {
        with.cte_tables
            .iter()
            .all(|cte| is_read_only_cte(&cte.query))
    })
}

/// `WITH`の中の1つが、読むだけか。`WITH ids(id) AS (VALUES (1), (2))`の
/// ように、`WITH`の中では`VALUES`も許す（#737。単独の`VALUES`は、
/// `is_read_only_query`が今までどおり断る）
fn is_read_only_cte(query: &Query) -> bool {
    fn select_or_values(body: &SetExpr) -> bool {
        match body {
            SetExpr::Values(_) => true,
            SetExpr::Query(query) => is_read_only_cte(query),
            SetExpr::SetOperation { left, right, .. } => {
                select_or_values(left) && select_or_values(right)
            }
            body => is_select_family(body),
        }
    }
    is_read_only_with(query) && select_or_values(&query.body) && !selects_into(&query.body)
}

/// `SELECT … INTO 表`か
fn selects_into(body: &SetExpr) -> bool {
    match body {
        SetExpr::Select(select) => select.into.is_some(),
        SetExpr::Query(query) => selects_into(&query.body),
        SetExpr::SetOperation { left, right, .. } => selects_into(left) || selects_into(right),
        _ => false,
    }
}

/// `sqlparser` represents `VALUES (...)`, `INSERT`/`UPDATE`/`DELETE`/`MERGE`
/// (when used as a query body, e.g. inside a CTE) and the `TABLE` shorthand
/// as a `Statement::Query` too, alongside genuine `SELECT`s and the set
/// operations (`UNION`/`EXCEPT`/`INTERSECT`) and parenthesized subqueries
/// built from them. `Statement::Query(_)` alone can't tell these apart, so
/// this walks the actual body to confirm it's SELECT-family before
/// `validate_single_query_statement` accepts it.
fn is_select_family(body: &SetExpr) -> bool {
    match body {
        SetExpr::Select(_) => true,
        SetExpr::Query(query) => is_select_family(&query.body),
        SetExpr::SetOperation { left, right, .. } => {
            is_select_family(left) && is_select_family(right)
        }
        SetExpr::Values(_)
        | SetExpr::Insert(_)
        | SetExpr::Update(_)
        | SetExpr::Delete(_)
        | SetExpr::Merge(_)
        | SetExpr::Table(_) => false,
    }
}

/// Parses `sql` as a single `SELECT` statement and returns the distinct
/// table names referenced *anywhere* in it - not just the top-level
/// `FROM`/`JOIN` clauses, but also any table reached only through a nested
/// subquery (a scalar subquery in the projection list, an `EXISTS`/`IN`
/// subquery in `WHERE`/`HAVING`, a derived table in `FROM`, etc). Useful for
/// callers (e.g. table-level access control) that only need to know which
/// tables a query touches, not which table each result column came from.
///
/// Walking only `select.from` (as an earlier version of this function did)
/// would miss a table referenced solely through such a subquery - e.g.
/// `SELECT id, (SELECT ssn FROM secret WHERE secret.uid = users.id) FROM
/// users` touches `secret` too, and a caller checking access per table must
/// see it or a policy gating `secret` could be silently bypassed. This uses
/// `sqlparser`'s recursive AST visitor (`visit_relations`) instead of a
/// hand-rolled walk, so every table reference is found regardless of how
/// deeply it's nested, rather than relying on this function correctly
/// enumerating every AST position a subquery could appear in.
///
/// Table names are returned exactly as qualified in the SQL (e.g.
/// `"public.users"` stays qualified, a bare `"users"` stays bare) rather
/// than collapsed to the bare table name - collapsing `schema_a.users` and
/// `schema_b.users` to the same `"users"` string here would let an access
/// policy intended for one schema's table also silently grant access to a
/// same-named table in a different, unintended schema. Callers configuring
/// policies must therefore match however their queries actually qualify
/// table names; an unqualified and a qualified reference to the same
/// physical table are treated as different resources (a query attributed to
/// the "wrong" form fails closed via "no matching policy", never open).
///
/// CTEs (`WITH`), set operations (`UNION` etc.) and multiple statements are
/// refused: a query shape this module can't confidently reason about must
/// not be treated as "safe."
pub fn resolve_query_tables(
    sql: &str,
    dialect: &dyn Dialect,
) -> Result<Vec<String>, ProvenanceError> {
    let statements = parse_statements(sql, dialect)?;

    let [statement] = statements.as_slice() else {
        return Err(unsupported("expected exactly one SQL statement"));
    };

    let Statement::Query(query) = statement else {
        return Err(unsupported("expected a SELECT statement"));
    };

    if query.with.is_some() {
        return Err(unsupported(
            "common table expressions (WITH) are not supported",
        ));
    }

    let SetExpr::Select(select) = query.body.as_ref() else {
        return Err(unsupported(
            "expected a plain SELECT (no UNION/EXCEPT/INTERSECT)",
        ));
    };
    // `TABLE name`・文字列のSQLを実行する関数は、下の`visit_relations`で
    // 拾えない
    refuse_hidden_reads(statement)?;
    let fold = name_fold(dialect);

    let mut tables = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let outcome = sqlparser::ast::visit_relations(select, |name| {
        match object_name_to_qualified_string(name, fold) {
            Ok(table) => {
                if seen.insert(table.clone()) {
                    tables.push(table);
                }
                std::ops::ControlFlow::Continue(())
            }
            Err(e) => std::ops::ControlFlow::Break(e),
        }
    });
    if let std::ops::ControlFlow::Break(e) = outcome {
        return Err(e);
    }

    Ok(tables)
}

/// 引数の文字列のSQL・表の名前で、表を読む関数（PostgreSQL）。読む表が
/// 文字列の中にあるので、`visit_relations`では拾えない（PR #802のレビュー）。
/// 名前がこれで始まる関数を、表ごとの許可を確かめるときは断る:
///
/// - `query_to_xml`・`table_to_xml`・`schema_to_xml`・`database_to_xml`・
///   `cursor_to_xml`（`…_xmlschema`・`…_xml_and_xmlschema`も）
/// - `ts_stat`（全文検索の統計。引数のSQLを実行する）
/// - `crosstab`・`connectby`（tablefunc）・`xpath_table`（xml2）: 拡張の関数
///
/// 網羅はできない: ユーザーが作った関数は、どの表でも読める。本来の守りは、
/// データベースのロールの権限
const TEXT_SQL_FUNCTION_PREFIXES: &[&str] = &[
    "query_to_xml",
    "table_to_xml",
    "schema_to_xml",
    "database_to_xml",
    "cursor_to_xml",
    "ts_stat",
    "crosstab",
    "xpath_table",
    "connectby",
];

/// 表を拾えない読み方を探す: `TABLE name`（`SELECT * FROM name`と同じ。
/// `sqlparser`は、この表の名前を文字列で持つ）と、文字列のSQLを実行する関数
struct HiddenReadFinder;

impl HiddenReadFinder {
    fn check_function(name: &ObjectName) -> std::ops::ControlFlow<String> {
        let name = name
            .0
            .last()
            .and_then(|part| part.as_ident())
            .map(|ident| ident.value.to_ascii_lowercase());
        match name.filter(|n| {
            TEXT_SQL_FUNCTION_PREFIXES
                .iter()
                .any(|prefix| n.starts_with(prefix))
        }) {
            Some(name) => std::ops::ControlFlow::Break(format!(
                "the function {name}() is not supported: it reads tables that are named in a string"
            )),
            None => std::ops::ControlFlow::Continue(()),
        }
    }
}

impl sqlparser::ast::Visitor for HiddenReadFinder {
    type Break = String;

    fn pre_visit_query(&mut self, query: &Query) -> std::ops::ControlFlow<String> {
        fn has_table_command(body: &SetExpr) -> bool {
            match body {
                SetExpr::Table(_) => true,
                SetExpr::SetOperation { left, right, .. } => {
                    has_table_command(left) || has_table_command(right)
                }
                // 入れ子の問い合わせは、それぞれ`pre_visit_query`に来る
                _ => false,
            }
        }
        if has_table_command(&query.body) {
            std::ops::ControlFlow::Break(
                "TABLE name is not supported: write SELECT * FROM name".to_string(),
            )
        } else {
            std::ops::ControlFlow::Continue(())
        }
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> std::ops::ControlFlow<String> {
        match expr {
            Expr::Function(function) => Self::check_function(&function.name),
            _ => std::ops::ControlFlow::Continue(()),
        }
    }

    // `FROM crosstab(...)`のように、表として呼ぶ関数
    fn pre_visit_table_factor(&mut self, table: &TableFactor) -> std::ops::ControlFlow<String> {
        match table {
            TableFactor::Table {
                name,
                args: Some(_),
                ..
            }
            | TableFactor::Function { name, .. } => Self::check_function(name),
            _ => std::ops::ControlFlow::Continue(()),
        }
    }
}

/// 文のどこか（副問い合わせ・`UNION`の中も）に、表を拾えない読み方があれば
/// 断る。拾えないまま通すと、その表の許可を確かめずに読める（PR #802の
/// レビュー）
fn refuse_hidden_reads(statement: &Statement) -> Result<(), ProvenanceError> {
    use sqlparser::ast::Visit;
    match statement.visit(&mut HiddenReadFinder) {
        std::ops::ControlFlow::Break(reason) => Err(unsupported(reason)),
        std::ops::ControlFlow::Continue(()) => Ok(()),
    }
}

/// 表の名前を許可と比べる前に、データベースと同じように小文字にそろえる
/// 範囲（PR #802のレビュー）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameFold {
    /// 書いたとおり。MySQL・MariaDBは、表の名前の大文字・小文字を区別するかが
    /// サーバーの設定（`lower_case_table_names`）とOSで変わるので、そろえない
    /// （区別しないサーバーで、許可と違う書き方をすれば、断る側になる）
    AsWritten,
    /// 引用符で囲まない名前だけ。PostgreSQLは、`Users`を`users`にし、
    /// `"Users"`は別の表とする
    Unquoted,
    /// すべて。SQLiteは、引用符で囲んでも、表の名前の大文字・小文字を
    /// 区別しない（`Notes`・`"NOTES"`・`notes`は同じ表）
    All,
}

fn name_fold(dialect: &dyn Dialect) -> NameFold {
    if dialect.is::<PostgreSqlDialect>() {
        NameFold::Unquoted
    } else if dialect.is::<SQLiteDialect>() {
        NameFold::All
    } else {
        NameFold::AsWritten
    }
}

/// 書き込みの文（`execute_command`）が触れる表（#609）。
/// [`resolve_write_tables`]が返す
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteTables {
    /// 書き込む表に要る許可: `INSERT`・`UPDATE`は`"write"`、`DELETE`は`"delete"`
    pub action: &'static str,
    /// 書き込む表に、`"delete"`の許可も要るか。ぶつかった今の行を消してから
    /// 入れる書き方（`REPLACE INTO`、SQLiteの`INSERT OR REPLACE`・
    /// `UPDATE OR REPLACE`）のとき。消すときは、削除のトリガーや
    /// `ON DELETE CASCADE`も動く（PR #802のレビュー）。文に書いた分だけを
    /// 見る。表の定義の`ON CONFLICT REPLACE`は、文からは分からない
    pub also_deletes: bool,
    /// 書き込む表（重複は除く）。名前は[`resolve_query_tables`]と同じく、
    /// PostgreSQLでは引用符なしの名前を、SQLiteではすべての名前を、小文字に
    /// そろえる（MySQL・MariaDBは、文に書いたとおり）
    pub written: Vec<String>,
    /// 読むだけの表（`INSERT … SELECT`の元、副問い合わせ、`UPDATE … FROM`・
    /// `DELETE … USING`の表）。書き込む表を、副問い合わせ等でも読むなら、
    /// ここにも入る
    pub read: Vec<String>,
}

/// 1つの`INSERT`・`UPDATE`・`DELETE`が触れる表を、書き込む表と読む表に
/// 分けて返す（#609。[`resolve_query_tables`]の書き込み版）。マイクロ
/// セグメンテーションが、表ごとに許可を確かめるために使う。
///
/// - `INSERT`: 書き込むのは、`INTO`の表
/// - `UPDATE`: 書き込むのは、`UPDATE`の直後の表。MySQLの、表を`JOIN`した
///   `UPDATE`では、`SET`の代入先（`u.note`の`u`）が指す表だけを書き込む表
///   とし、ほかの`JOIN`した表は読む表とする（#807。`JOIN`しただけの表の
///   値を、`read`の許可なしに写せないように）。代入先の表を見分けられない
///   とき（`SET note = …`のように表を書いていない等）は、`JOIN`した表を
///   すべて、書き込む表にも読む表にもする。`FROM`の表は読むだけ
/// - `DELETE`: 書き込むのは、`FROM`の表。MySQLの`DELETE a, b FROM a JOIN b`
///   では、`DELETE`の直後に書いた表（か別名）だけを消す表とし、ほかの
///   `JOIN`した表は読む表とする（#807）。`USING`の表は読むだけ。
///   ただしMySQL・MariaDBの`DELETE FROM a USING …`は、`FROM`の名前が
///   `USING`の表（か、その別名）を指すので、それが指す表を消す表とし、
///   ほかの`USING`の表は読む表とする（`DELETE FROM notes USING secret AS
///   notes`は、`secret`から消す）。消す表を見分けられない文は、断る
///
/// それ以外の場所に出る表（副問い合わせ・`INSERT … SELECT`の元）は、
/// `sqlparser`の`visit_relations`で、深さに関わらず拾う。
///
/// 書き込む表を`WHERE`で絞ること・`SET`でその列を使うことは、書き込みの
/// 許可に含める（読む許可は求めない）。名前は、[`resolve_query_tables`]と
/// 同じく、スキーマまで含めて比べ（`public.users`と`users`は別の名前）、
/// PostgreSQLでは引用符なしの名前を、SQLiteではすべての名前を小文字に
/// そろえる。マイクロセグメンテーションの許可のリソースは、そろえずに
/// 書いたとおりに比べるので、PostgreSQLの引用符なしの表とSQLiteの表は、
/// 小文字で書く（PostgreSQLの`"Users"`は`Users`のまま）。
///
/// 確かに分けられない形（`WITH`、表の関数や副問い合わせへの`INSERT`等）は
/// エラーにする。呼ぶ側は、そのとき文を断る。
///
/// 確かめるのは、文に書いたことだけ。その先でデータベースがすること
/// （PostgreSQLの継承・パーティションの子の表、ビューの元の表、トリガー、
/// 外部キーの`ON DELETE CASCADE`）は、文からは分からないので、見ない。
/// 表の定義で決めた動きも同じ: SQLiteの`UNIQUE … ON CONFLICT REPLACE`の
/// 表では、`OR REPLACE`を書かない`INSERT`・`UPDATE`でも、ぶつかった同じ表の
/// 行が消える。これは`also_deletes`にならず、`write`の許可だけで通る。親の
/// 表・ビューへの許可は、それを通して届く行への許可になる（PostgreSQL自身の
/// 権限も、親の表を通した操作では、子の表の権限を確かめない）。
pub fn resolve_write_tables(
    sql: &str,
    dialect: &dyn Dialect,
) -> Result<WriteTables, ProvenanceError> {
    let statements = parse_statements(sql, dialect)?;
    let [statement] = statements.as_slice() else {
        return Err(unsupported("expected exactly one SQL statement"));
    };

    refuse_hidden_reads(statement)?;
    let fold = name_fold(dialect);

    let replaces = |or: &Option<sqlparser::ast::SqliteOnConflict>| {
        matches!(or, Some(sqlparser::ast::SqliteOnConflict::Replace))
    };
    let mut also_deletes = false;
    let mut written: Vec<String> = Vec::new();
    // 表としては数えない名前（MySQLの`DELETE FROM a USING …`の`a`）
    let mut not_tables: Vec<String> = Vec::new();
    // 書き込む表のうち、読む表にも入れるもの（どれに書くかを見分けられない
    // とき。#807）
    let mut also_read: Vec<String> = Vec::new();
    let action = match statement {
        Statement::Insert(insert) => {
            let TableObject::TableName(name) = &insert.table else {
                return Err(unsupported("expected INSERT INTO a table name"));
            };
            if !insert.multi_table_into_clauses.is_empty() {
                return Err(unsupported("multi-table INSERT is not supported"));
            }
            if insert.source.as_ref().is_some_and(|q| q.with.is_some()) {
                return Err(unsupported(
                    "common table expressions (WITH) are not supported",
                ));
            }
            written.push(object_name_to_qualified_string(name, fold)?);
            also_deletes = insert.replace_into || replaces(&insert.or);
            "write"
        }
        Statement::Update(update) => {
            let joined = collect_joined(std::slice::from_ref(&update.table), fold)?;
            // `SET`の代入先が指す表（#807）。1つでも見分けられなければ`None`
            let targets: Option<Vec<String>> = if joined.len() == 1 {
                Some(vec![joined[0].table.clone()])
            } else {
                update
                    .assignments
                    .iter()
                    .flat_map(|assignment| match &assignment.target {
                        AssignmentTarget::ColumnName(column) => vec![column],
                        AssignmentTarget::Tuple(columns) => columns.iter().collect(),
                    })
                    .map(|column| {
                        let parts = ident_parts(column)?;
                        let (_, qualifier) = parts.split_last()?;
                        resolve_joined(qualifier, &joined, fold)
                    })
                    .collect()
            };
            match targets {
                Some(targets) => {
                    written.extend(
                        joined
                            .iter()
                            .filter(|entry| targets.contains(&entry.table))
                            .map(|entry| entry.table.clone()),
                    );
                }
                None => {
                    written.extend(joined.iter().map(|entry| entry.table.clone()));
                    also_read.extend(joined.iter().map(|entry| entry.table.clone()));
                }
            }
            also_deletes = replaces(&update.or);
            "write"
        }
        Statement::Delete(delete) => {
            let (FromTable::WithFromKeyword(from) | FromTable::WithoutKeyword(from)) = &delete.from;
            // 消す表を指す名前（表の名前か別名）と、その名前で探す表
            let (target_names, joined): (Vec<&ObjectName>, Vec<Joined>) = match &delete.using {
                // MySQL・MariaDB: `FROM`の名前は、`USING`の表か別名を指す
                // だけ。消す表は`USING`の中にある（PR #802のレビュー）
                Some(using) if dialect.is::<MySqlDialect>() => {
                    if !delete.tables.is_empty() {
                        return Err(unsupported("unsupported DELETE form"));
                    }
                    let mut names = Vec::new();
                    for table in from {
                        let TableFactor::Table { name, .. } = &table.relation else {
                            return Err(unsupported("expected a table name to write to"));
                        };
                        if !table.joins.is_empty() {
                            return Err(unsupported("unsupported table form in a write"));
                        }
                        // `visit_relations`は、この名前も表として渡すので、除く
                        not_tables.push(object_name_to_qualified_string(name, fold)?);
                        names.push(name);
                    }
                    (names, collect_joined(using, fold)?)
                }
                // `DELETE a, b FROM a JOIN b …`の`a`・`b`（`visit_relations`は、
                // これらを表として渡さない）
                _ => (delete.tables.iter().collect(), collect_joined(from, fold)?),
            };
            if target_names.is_empty() {
                if joined.len() == 1 {
                    written.push(joined[0].table.clone());
                } else {
                    // 消す表を書かない、複数の表の`DELETE`（MySQLでは
                    // 構文エラー）。すべてを、消す表にも読む表にもする
                    written.extend(joined.iter().map(|entry| entry.table.clone()));
                    also_read.extend(joined.iter().map(|entry| entry.table.clone()));
                }
            } else {
                // 消す表を見分けられない文は、断る（#807）
                let mut targets: Vec<String> = Vec::new();
                for name in target_names {
                    let target = ident_parts(name)
                        .and_then(|parts| resolve_joined(&parts, &joined, fold))
                        .ok_or_else(|| {
                            unsupported("cannot tell which table a DELETE target names")
                        })?;
                    targets.push(target);
                }
                written.extend(
                    joined
                        .iter()
                        .filter(|entry| targets.contains(&entry.table))
                        .map(|entry| entry.table.clone()),
                );
            }
            "delete"
        }
        _ => {
            return Err(unsupported(
                "expected an INSERT, UPDATE, or DELETE statement",
            ))
        }
    };

    // 文の全体に出る表を、出た回数だけ集める
    let mut seen: Vec<String> = Vec::new();
    let outcome = sqlparser::ast::visit_relations(statement, |name| {
        match object_name_to_qualified_string(name, fold) {
            Ok(table) => {
                seen.push(table);
                std::ops::ControlFlow::Continue(())
            }
            Err(e) => std::ops::ControlFlow::Break(e),
        }
    });
    if let std::ops::ControlFlow::Break(e) = outcome {
        return Err(e);
    }
    // 書き込む場所の分を1回ずつ除く。残りは読む表。除く分が見つからなければ、
    // 数え方が合っていないので、断る
    for name in written.iter().chain(&not_tables) {
        let Some(index) = seen.iter().position(|table| table == name) else {
            return Err(unsupported(
                "cannot tell the written tables from the tables that are read",
            ));
        };
        seen.remove(index);
    }
    let mut read: Vec<String> = Vec::new();
    for table in seen.into_iter().chain(also_read) {
        if !read.contains(&table) {
            read.push(table);
        }
    }
    // 書き込む表の重複（自分自身との`JOIN`）は、除いた後でまとめる
    let mut targets: Vec<String> = Vec::new();
    for table in written {
        if !targets.contains(&table) {
            targets.push(table);
        }
    }
    Ok(WriteTables {
        action,
        also_deletes,
        written: targets,
        read,
    })
}

/// `UPDATE`の直後・`DELETE FROM`（か`USING`）の表と、それに`JOIN`した表の
/// 1つ
#[derive(Debug)]
struct Joined {
    /// 表の名前（[`object_name_to_qualified_string`]の形）
    table: String,
    /// 別名（`users u`の`u`）
    alias: Option<String>,
    /// 名前の最後の部分（`db.users`の`users`）
    last: String,
}

/// `UPDATE`の直後・`DELETE FROM`の表と、それに`JOIN`した表を、文に出た順に
/// 集める。最初の表は、名前で書いた表でなければならない。`JOIN`した
/// 副問い合わせは、書き込めないので、ここでは集めない（中の表は、読む表
/// として拾われる）
fn collect_joined(
    tables: &[TableWithJoins],
    fold: NameFold,
) -> Result<Vec<Joined>, ProvenanceError> {
    if tables.is_empty() {
        return Err(unsupported("expected a table to write to"));
    }
    let entry = |name: &ObjectName, alias: &Option<sqlparser::ast::TableAlias>| {
        let last = match name.0.last() {
            Some(ObjectNamePart::Identifier(ident)) => ident.value.clone(),
            _ => return Err(unsupported("unsupported table name form")),
        };
        Ok(Joined {
            table: object_name_to_qualified_string(name, fold)?,
            alias: alias.as_ref().map(|alias| alias.name.value.clone()),
            last,
        })
    };
    let mut joined = Vec::new();
    for table in tables {
        let TableFactor::Table {
            name, args, alias, ..
        } = &table.relation
        else {
            return Err(unsupported("expected a table name to write to"));
        };
        if args.is_some() {
            return Err(unsupported("table functions are not supported"));
        }
        joined.push(entry(name, alias)?);
        for join in &table.joins {
            match &join.relation {
                TableFactor::Table {
                    name,
                    args: None,
                    alias,
                    ..
                } => joined.push(entry(name, alias)?),
                TableFactor::Derived { .. } => {}
                _ => return Err(unsupported("unsupported table form in a write")),
            }
        }
    }
    Ok(joined)
}

/// 名前の部分（`u.note`の`u`・`note`）。識別子でない部分があれば`None`
fn ident_parts(name: &ObjectName) -> Option<Vec<&Ident>> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => Some(ident),
            _ => None,
        })
        .collect()
}

/// `SET u.note`の`u`・`DELETE o FROM …`の`o`のような名前が指す、`JOIN`した
/// 表（#807）。1つの部分なら、別名か、別名の無い表の名前の最後の部分と
/// 比べる。2つ以上の部分（`db.users`）なら、別名の無い表の名前と比べる。
/// 大文字・小文字は区別しない（MySQLは、OSによって区別しない）。当たる表が
/// ちょうど1つ（自分自身との`JOIN`で、同じ表に何度か当たるのは1つと数える）
/// のときだけ返し、当たらない・2つ以上に当たるときは`None`
fn resolve_joined(qualifier: &[&Ident], joined: &[Joined], fold: NameFold) -> Option<String> {
    let mut found: Option<&str> = None;
    for entry in joined {
        let hit = match qualifier {
            [] => false,
            [name] => match &entry.alias {
                Some(alias) => alias.eq_ignore_ascii_case(&name.value),
                None => entry.last.eq_ignore_ascii_case(&name.value),
            },
            parts => {
                entry.alias.is_none() && {
                    let name = ObjectName(
                        parts
                            .iter()
                            .map(|ident| ObjectNamePart::Identifier((*ident).clone()))
                            .collect(),
                    );
                    object_name_to_qualified_string(&name, fold)
                        .is_ok_and(|name| name.eq_ignore_ascii_case(&entry.table))
                }
            }
        };
        if hit {
            match found {
                Some(table) if table != entry.table => return None,
                _ => found = Some(&entry.table),
            }
        }
    }
    found.map(str::to_string)
}

/// Fully schema-qualified table name (all parts joined with `.`), keeping
/// every part that appeared in the SQL - see [`resolve_query_tables`]'s doc
/// comment for why this must not collapse to the bare table name.
///
/// `fold`に従って、小文字にそろえる（[`NameFold`]。PostgreSQL・SQLiteと
/// 同じく、ASCIIの文字だけ）
fn object_name_to_qualified_string(
    name: &ObjectName,
    fold: NameFold,
) -> Result<String, ProvenanceError> {
    let mut parts = Vec::with_capacity(name.0.len());
    for part in &name.0 {
        match part {
            // 引用符で囲んだ名前は、`.`を含められる（`"public.users"`は、
            // `public.users`とは別の、1つの名前の表）。`.`でつなぐと区別
            // できなくなるので、断る（PR #802のレビュー）
            ObjectNamePart::Identifier(ident) if ident.value.contains('.') => {
                return Err(unsupported(
                    "table names that contain a dot inside quotes are not supported",
                ))
            }
            ObjectNamePart::Identifier(ident)
                if fold == NameFold::All
                    || (fold == NameFold::Unquoted && ident.quote_style.is_none()) =>
            {
                parts.push(ident.value.to_ascii_lowercase())
            }
            ObjectNamePart::Identifier(ident) => parts.push(ident.value.clone()),
            _ => return Err(unsupported("unsupported table name form")),
        }
    }
    if parts.is_empty() {
        return Err(unsupported("unsupported table name form"));
    }
    Ok(parts.join("."))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validate(sql: &str) -> Result<(), ProvenanceError> {
        validate_single_query_statement(sql, &PostgreSqlDialect {})
    }

    /// MariaDBだけが実行するコメント（`/*M! … */`）を、MySQLの方言では
    /// 拒否する。中身を検査できないため（PR #573のレビュー）
    #[test]
    fn mariadb_executable_comments_are_rejected() {
        let mysql = MySqlDialect {};
        for sql in [
            "SELECT 1 /*M! INTO OUTFILE '/tmp/x' */",
            "SELECT 1 /*M!100100 INTO OUTFILE '/tmp/x' */",
            "SELECT id FROM t /*M! INTO DUMPFILE '/tmp/x' */",
            "SELECT id FROM t /*M! UNION SELECT secret FROM other */",
            "SELECT 1 /*m! INTO OUTFILE '/tmp/x' */",
        ] {
            let err = validate_single_query_statement(sql, &mysql).unwrap_err();
            assert!(err.to_string().contains("/*M!"), "{sql}: {err}");
            assert!(resolve_query_tables(sql, &mysql).is_err(), "{sql}");
        }
        assert!(validate_single_write_statement(
            "UPDATE t SET x = 1 /*M! , y = (SELECT secret FROM other) */",
            &mysql
        )
        .is_err());
        // MySQLの`/*! … */`は、sqlparserが中身を読むので、今までどおり検査する
        assert!(
            validate_single_query_statement("SELECT 1 /*! INTO OUTFILE '/tmp/x' */", &mysql)
                .is_err()
        );
        // ただのコメントや、文字列の中の`/*M!`は、拒否しない
        for sql in [
            "SELECT 1 /* M! note */",
            "SELECT 1 /* note */",
            "SELECT '/*M! not a comment */' AS s",
            "SELECT 1 -- /*M! line comment */",
        ] {
            assert!(
                validate_single_query_statement(sql, &mysql).is_ok(),
                "{sql}"
            );
        }
    }

    /// 行の鍵と、サーバーのファイル・接続に触れる関数を断る（#597）
    #[test]
    fn side_effects_are_refused() {
        let mysql = MySqlDialect {};
        let pg = PostgreSqlDialect {};
        for sql in [
            "SELECT id FROM users FOR UPDATE",
            "SELECT id FROM users FOR SHARE",
            "SELECT * FROM (SELECT id FROM users FOR UPDATE) AS t",
            "SELECT LOAD_FILE('/etc/passwd')",
            "SELECT id FROM users WHERE email = load_file('/etc/passwd')",
            "SELECT sys_exec('id')",
        ] {
            let err = validate_single_query_statement(sql, &mysql).unwrap_err();
            assert!(
                err.to_string().contains("FOR UPDATE") || err.to_string().contains("()"),
                "{sql}: {err}"
            );
        }
        for sql in [
            "SELECT id FROM users FOR UPDATE",
            "SELECT pg_read_file('/etc/passwd')",
            "SELECT lo_export(1, '/tmp/x')",
            "SELECT * FROM dblink('dbname=app', 'DELETE FROM users') AS t(x int)",
            "SELECT dblink_exec('dbname=app', 'DELETE FROM users')",
            "SELECT dblink_open('c', 'SELECT 1')",
            "SELECT pg_ls_logdir()",
            "SELECT pg_terminate_backend(pid) FROM pg_stat_activity",
            "SELECT pg_cancel_backend(1)",
            // Unicodeのエスケープで書いた名前（PR #610のレビュー）
            r#"SELECT U&"pg\005Fread_file"('/etc/passwd')"#,
            r#"SELECT u&"dblink\005Fexec"('x', 'y')"#,
        ] {
            assert!(validate_single_query_statement(sql, &pg).is_err(), "{sql}");
        }
        // SQLite（#528）。ATTACH・PRAGMA・VACUUMは、SELECTではないので断る
        let sqlite = SQLiteDialect {};
        for sql in [
            "SELECT load_extension('/tmp/evil.so')",
            "SELECT fts3_tokenizer('simple')",
            "ATTACH DATABASE '/tmp/other.db' AS other",
            "PRAGMA writable_schema = ON",
            "VACUUM INTO '/tmp/copy.db'",
        ] {
            assert!(
                validate_single_query_statement(sql, &sqlite).is_err(),
                "{sql}"
            );
            assert!(
                validate_single_write_statement(sql, &sqlite).is_err(),
                "{sql}"
            );
        }
        // 書き込みの道具でも、関数の名前で断る（文の種類ではなく。PR #798のレビュー）
        for sql in [
            "INSERT INTO users (id) VALUES (load_extension('/tmp/evil.so'))",
            "UPDATE users SET name = fts3_tokenizer('simple')",
        ] {
            let err = validate_single_write_statement(sql, &sqlite).unwrap_err();
            assert!(err.to_string().contains("()"), "{sql}: {err}");
        }
        assert!(validate_single_query_statement("SELECT sqlite_version()", &sqlite).is_ok());
        // ふつうの引用符つきの名前・U&の文字列は通す
        assert!(validate_single_query_statement(r#"SELECT "id" FROM users"#, &pg).is_ok());
        assert!(validate_single_query_statement("SELECT 1 & 2", &pg).is_ok());
        // MySQLのヒントSET_VARは断る。ほかのヒントは通す（PR #610のレビュー）
        assert!(validate_single_write_statement(
            "INSERT /*+ SET_VAR(foreign_key_checks=0) */ INTO users (id) VALUES (1)",
            &mysql
        )
        .is_err());
        assert!(validate_single_query_statement(
            "SELECT /*+ set_var(sort_buffer_size = 16M) */ id FROM users",
            &mysql
        )
        .is_err());
        assert!(validate_single_query_statement(
            "SELECT /*+ MAX_EXECUTION_TIME(1000) */ 1",
            &mysql
        )
        .is_ok());
        // 書き込みの道具でも、関数は断る。行の鍵は書き込みの文には要らない
        assert!(validate_single_write_statement(
            "UPDATE users SET email = LOAD_FILE('/etc/passwd') WHERE id = 1",
            &mysql
        )
        .is_err());
        // ふつうの関数・列名は通す
        for sql in [
            "SELECT COUNT(*), MAX(id) FROM users",
            "SELECT load_file_name FROM users",
            "SELECT GET_LOCK('x', 0)",
        ] {
            assert!(
                validate_single_query_statement(sql, &mysql).is_ok(),
                "{sql}"
            );
        }
        assert!(validate_single_write_statement(
            "UPDATE users SET email = LOWER(email) WHERE id = 1",
            &mysql
        )
        .is_ok());
    }

    /// MySQLのコメント・文字列の書き方は、サーバーと同じに読む（#597の点検）。
    /// 検査では文字列・コメントに見えて、サーバーでは書き出しを実行する、
    /// という食い違いが無いこと
    #[test]
    fn mysql_comments_and_strings_are_read_like_the_server() {
        let mysql = MySqlDialect {};
        // サーバーでINTO OUTFILEを実行する書き方は、どれも断る
        for sql in [
            "SELECT 1 INTO OUTFILE '/tmp/x'",
            "SELECT 1 INTO DUMPFILE '/tmp/x'",
            "SELECT 1 FROM t INTO OUTFILE '/tmp/x'",
            "SELECT 1 --1 INTO OUTFILE '/tmp/x'",
            "SELECT 1 # x\n INTO OUTFILE '/tmp/x'",
            "SELECT 1 /*!50000 INTO OUTFILE '/tmp/x' */",
            "SELECT 1 /* a /* b */ INTO OUTFILE '/tmp/x' /* */",
            "SELECT 1 AS `a\\` INTO OUTFILE '/tmp/x' -- `",
            "SELECT * FROM t WHERE a = 'x' INTO @y",
        ] {
            assert!(
                validate_single_query_statement(sql, &mysql).is_err(),
                "{sql}"
            );
        }
        // サーバーでもコメント・文字列なので、通してよい
        for sql in [
            "SELECT 1 # INTO OUTFILE '/tmp/x'",
            "SELECT _utf8'a\\' INTO OUTFILE ''/tmp/x'' -- ' AS s",
            "SELECT 'a' 'b'",
            "SELECT /*+ MAX_EXECUTION_TIME(1000) */ 1",
        ] {
            assert!(
                validate_single_query_statement(sql, &mysql).is_ok(),
                "{sql}"
            );
        }
    }

    #[test]
    fn validate_single_query_statement_accepts_plain_select() {
        assert!(validate("SELECT email FROM users").is_ok());
    }

    #[test]
    fn validate_single_query_statement_accepts_cte_and_union() {
        // These are legitimate read-only SELECT-family constructs, not
        // statements that violate execute_query's "SELECT only" contract.
        assert!(validate("WITH t AS (SELECT 1 AS x) SELECT x FROM t").is_ok());
        assert!(validate("SELECT a FROM t1 UNION SELECT b FROM t2").is_ok());
    }

    #[test]
    fn validate_single_query_statement_rejects_non_select() {
        assert!(validate("INSERT INTO users (email) VALUES ('a@b.com')").is_err());
        assert!(validate("DROP TABLE users").is_err());
    }

    #[test]
    fn validate_single_query_statement_rejects_values_and_table() {
        // `VALUES (...)` and `TABLE t` parse as `Statement::Query` too (the
        // same statement variant a real SELECT does), but neither is a
        // SELECT - execute_query's "SELECT statements only" contract must
        // still reject them.
        assert!(validate("VALUES (1)").is_err());
        assert!(validate("TABLE users").is_err());
    }

    #[test]
    fn validate_single_write_statement_accepts_only_dml() {
        let check = |sql: &str| validate_single_write_statement(sql, &PostgreSqlDialect {});
        for sql in [
            "INSERT INTO users (email) VALUES ($1)",
            "UPDATE users SET email = $1 WHERE id = $2",
            "DELETE FROM users WHERE id = $1",
        ] {
            assert!(check(sql).is_ok(), "{sql}");
        }
        for sql in [
            "SELECT 1",
            "DROP TABLE users",
            "CREATE TABLE t (id int)",
            "TRUNCATE users",
            "GRANT ALL ON users TO public",
            "ALTER TABLE users ADD COLUMN x int",
            "DELETE FROM users; DROP TABLE users",
        ] {
            assert!(check(sql).is_err(), "{sql}");
        }
    }

    /// `WITH`の中の書き込みと、`SELECT … INTO`を拒否する（#453）
    #[test]
    fn validate_single_query_statement_rejects_writes_hidden_in_select() {
        for sql in [
            "WITH d AS (DELETE FROM users RETURNING *) SELECT * FROM d",
            "WITH u AS (UPDATE users SET email = '' RETURNING id) SELECT id FROM u",
            "WITH i AS (INSERT INTO users (email) VALUES ('x') RETURNING id) SELECT id FROM i",
            "WITH a AS (SELECT 1), d AS (DELETE FROM users RETURNING *) SELECT * FROM a",
            "WITH outer_cte AS (WITH d AS (DELETE FROM users RETURNING *) SELECT * FROM d) SELECT * FROM outer_cte",
            "SELECT * INTO copied_users FROM users",
        ] {
            assert!(validate(sql).is_err(), "{sql}");
        }
        // 読むだけのWITHは通す
        assert!(
            validate("WITH a AS (SELECT 1 AS x), b AS (SELECT x FROM a) SELECT * FROM b").is_ok()
        );
        // WITHの中のVALUESも通す（#737）。単独のVALUESは断る
        for sql in [
            "WITH ids(id) AS (VALUES (1), (2)) SELECT id FROM ids",
            "WITH ids(id) AS (SELECT 1 UNION ALL VALUES (2)) SELECT id FROM ids",
        ] {
            assert!(validate(sql).is_ok(), "{sql}");
        }
        assert!(validate("VALUES (1), (2)").is_err());
    }

    #[test]
    fn validate_single_query_statement_rejects_multiple_statements() {
        assert!(validate("SELECT 1; DROP TABLE users;").is_err());
    }

    fn tables(sql: &str) -> Result<Vec<String>, ProvenanceError> {
        resolve_query_tables(sql, &PostgreSqlDialect {})
    }

    #[test]
    fn resolve_query_tables_single_table() {
        assert_eq!(
            tables("SELECT email FROM users").unwrap(),
            vec!["users".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_join_returns_all_distinct_tables() {
        assert_eq!(
            tables("SELECT u.email, o.total FROM users u JOIN orders o ON u.id = o.user_id")
                .unwrap(),
            vec!["users".to_string(), "orders".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_self_join_deduplicates() {
        assert_eq!(
            tables("SELECT a.email FROM users a JOIN users b ON a.manager_id = b.id").unwrap(),
            vec!["users".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_wildcard_still_resolves_tables() {
        // resolve_query_tables only needs FROM/JOIN tables, not projection
        // shape, so a bare `SELECT *` works the same as any other projection.
        assert_eq!(
            tables("SELECT * FROM users").unwrap(),
            vec!["users".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_rejects_cte_and_union() {
        assert!(tables("WITH t AS (SELECT 1 AS x) SELECT x FROM t").is_err());
        assert!(tables("SELECT a FROM t1 UNION SELECT b FROM t2").is_err());
    }

    #[test]
    fn resolve_query_tables_rejects_multiple_statements() {
        assert!(tables("SELECT 1; DROP TABLE users;").is_err());
    }

    #[test]
    fn resolve_query_tables_preserves_schema_qualification() {
        // resolve_query_tables must not collapse "public.users" and
        // "other_schema.users" to the same "users" string - doing so would
        // let a policy meant for one schema's table also grant access to a
        // same-named table in a different, unintended schema.
        assert_eq!(
            tables("SELECT email FROM public.users").unwrap(),
            vec!["public.users".to_string()]
        );
    }

    fn write_tables(sql: &str) -> Result<WriteTables, ProvenanceError> {
        resolve_write_tables(sql, &PostgreSqlDialect {})
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    /// 書き込む表と、読むだけの表を分ける（#609）
    #[test]
    fn resolve_write_tables_splits_written_and_read() {
        for (sql, action, written, read) in [
            ("INSERT INTO users (email) VALUES ($1)", "write", vec!["users"], vec![]),
            (
                "INSERT INTO archive SELECT * FROM users WHERE id IN (SELECT user_id FROM orders)",
                "write",
                vec!["archive"],
                vec!["users", "orders"],
            ),
            ("UPDATE public.users SET email = $1 WHERE id = $2", "write", vec!["public.users"], vec![]),
            (
                "UPDATE users u SET email = (SELECT email FROM secret s WHERE s.uid = u.id)",
                "write",
                vec!["users"],
                vec!["secret"],
            ),
            (
                "UPDATE users SET email = o.email FROM orders o WHERE o.user_id = users.id",
                "write",
                vec!["users"],
                vec!["orders"],
            ),
            ("DELETE FROM users WHERE id = $1", "delete", vec!["users"], vec![]),
            (
                "DELETE FROM users USING orders WHERE orders.user_id = users.id",
                "delete",
                vec!["users"],
                vec!["orders"],
            ),
            (
                "DELETE FROM users WHERE id IN (SELECT uid FROM secret)",
                "delete",
                vec!["users"],
                vec!["secret"],
            ),
            // 書き込む表を、副問い合わせでも読む
            (
                "UPDATE users SET rank = (SELECT count(*) FROM users)",
                "write",
                vec!["users"],
                vec!["users"],
            ),
            (
                "INSERT INTO users (email) SELECT email FROM users",
                "write",
                vec!["users"],
                vec!["users"],
            ),
            // RETURNINGの中の副問い合わせ
            (
                "DELETE FROM users WHERE id = 1 RETURNING (SELECT ssn FROM secret LIMIT 1)",
                "delete",
                vec!["users"],
                vec!["secret"],
            ),
            // ON CONFLICTの中の副問い合わせ
            (
                "INSERT INTO users (id) VALUES (1) ON CONFLICT (id) DO UPDATE SET email = (SELECT e FROM secret LIMIT 1)",
                "write",
                vec!["users"],
                vec!["secret"],
            ),
        ] {
            let tables = write_tables(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
            assert_eq!(tables.action, action, "{sql}");
            assert_eq!(tables.written, names(&written), "{sql}");
            assert_eq!(tables.read, names(&read), "{sql}");
        }
    }

    /// MySQLの、複数の表のUPDATE・DELETE（#609・#807）。`SET`の代入先・
    /// `DELETE`の直後に書いた表だけを書き込む表とし、ほかのJOINした表は
    /// 読む表とする
    #[test]
    fn resolve_write_tables_splits_joined_tables_by_target() {
        let mysql = |sql: &str| resolve_write_tables(sql, &MySqlDialect {});
        let tables =
            mysql("UPDATE users u JOIN orders o ON o.user_id = u.id SET o.total = 0").unwrap();
        assert_eq!(tables.written, names(&["orders"]));
        assert_eq!(tables.read, names(&["users"]));

        let tables = mysql("DELETE o FROM users u JOIN orders o ON o.user_id = u.id").unwrap();
        assert_eq!(tables.action, "delete");
        assert_eq!(tables.written, names(&["orders"]));
        assert_eq!(tables.read, names(&["users"]));

        // JOINしただけの表の値を写すには、その表のreadが要る（#807の例）
        let tables =
            mysql("UPDATE users u JOIN secret s ON s.uid = u.id SET u.note = s.ssn").unwrap();
        assert_eq!(tables.written, names(&["users"]));
        assert_eq!(tables.read, names(&["secret"]));
        // 探すのに使うだけの表には、deleteではなくreadを求める
        let tables =
            mysql("DELETE u FROM users u JOIN secret s ON s.uid = u.id WHERE s.flag = 1").unwrap();
        assert_eq!(tables.written, names(&["users"]));
        assert_eq!(tables.read, names(&["secret"]));

        // 両方を変える・両方から消す
        let tables =
            mysql("UPDATE users u JOIN orders o ON o.user_id = u.id SET u.a = 1, o.b = 2").unwrap();
        assert_eq!(tables.written, names(&["users", "orders"]));
        assert!(tables.read.is_empty(), "{:?}", tables.read);
        let tables = mysql("DELETE u, o FROM users u JOIN orders o ON o.user_id = u.id").unwrap();
        assert_eq!(tables.written, names(&["users", "orders"]));
        assert!(tables.read.is_empty(), "{:?}", tables.read);

        // 別名の無い表は名前で、`db.table`はそのまま指す。大文字・小文字は
        // 区別しない
        let tables = mysql(
            "UPDATE users JOIN shop.orders ON orders.user_id = users.id SET shop.orders.total = 0",
        )
        .unwrap();
        assert_eq!(tables.written, names(&["shop.orders"]));
        assert_eq!(tables.read, names(&["users"]));
        let tables = mysql("DELETE O FROM users u JOIN orders o ON o.user_id = u.id").unwrap();
        assert_eq!(tables.written, names(&["orders"]));
        assert_eq!(tables.read, names(&["users"]));

        // 代入先の表を見分けられないときは、すべてを書き込む表にも読む表にも
        // する（表を書いていない列・同じ名前の2つの表）
        for sql in [
            "UPDATE users u JOIN secret s ON s.uid = u.id SET note = 1",
            "UPDATE a.users JOIN b.users ON a.users.id = b.users.id SET users.x = 1",
        ] {
            let tables = mysql(sql).unwrap_or_else(|e| panic!("{sql}: {e}"));
            assert_eq!(tables.written.len(), 2, "{sql}: {tables:?}");
            assert_eq!(tables.read, tables.written, "{sql}");
        }

        // 消す表を見分けられないDELETEは断る
        for sql in [
            "DELETE x FROM users u JOIN orders o ON o.user_id = u.id",
            "DELETE users FROM users u JOIN orders o ON o.user_id = u.id",
            "DELETE FROM x USING users u JOIN orders o ON o.user_id = u.id",
        ] {
            let error = mysql(sql).unwrap_err().to_string();
            assert!(error.contains("DELETE target"), "{sql}: {error}");
        }

        // JOINした副問い合わせの中の表は、読むだけ
        let tables = mysql(
            "UPDATE users u JOIN (SELECT uid, ssn FROM secret) s ON s.uid = u.id SET u.note = s.ssn",
        )
        .unwrap();
        assert_eq!(tables.written, names(&["users"]));
        assert_eq!(tables.read, names(&["secret"]));

        // 自分自身とのJOIN
        let tables =
            mysql("UPDATE users a JOIN users b ON a.boss = b.id SET a.rank = b.rank").unwrap();
        assert_eq!(tables.written, names(&["users"]));
        assert!(tables.read.is_empty());
    }

    /// MySQL・MariaDBの`DELETE FROM a USING …`は、`USING`の表から消す
    /// （PR #802のレビュー）。`FROM`の名前は、別名かもしれない
    #[test]
    fn resolve_write_tables_mysql_delete_using_writes_the_using_tables() {
        let mysql = |sql: &str| resolve_write_tables(sql, &MySqlDialect {});
        let tables = mysql("DELETE FROM notes USING secret AS notes").unwrap();
        assert_eq!(tables.action, "delete");
        assert_eq!(tables.written, names(&["secret"]));
        assert!(tables.read.is_empty(), "{:?}", tables.read);

        // `FROM`の名前が指す表だけを消す。ほかの`USING`の表は読むだけ（#807）
        let tables = mysql(
            "DELETE FROM users USING users JOIN orders ON orders.user_id = users.id WHERE users.id IN (SELECT uid FROM secret)",
        )
        .unwrap();
        assert_eq!(tables.written, names(&["users"]));
        assert_eq!(tables.read, names(&["orders", "secret"]));

        // PostgreSQLでは、`USING`の表は読むだけ
        let tables =
            write_tables("DELETE FROM users USING orders WHERE orders.user_id = users.id").unwrap();
        assert_eq!(tables.written, names(&["users"]));
        assert_eq!(tables.read, names(&["orders"]));
    }

    /// `TABLE name`は、表を拾えないので断る（PR #802のレビュー）。`UNION`の
    /// 中でも断る。副問い合わせの中の`TABLE name`は、今の`sqlparser`では
    /// 構文エラーになる（どちらにしても通さない）
    #[test]
    fn table_commands_are_refused_wherever_they_are() {
        for sql in [
            "INSERT INTO archive (ssn) TABLE secret",
            "INSERT INTO archive (ssn) SELECT ssn FROM users UNION ALL TABLE secret",
        ] {
            let error = write_tables(sql).unwrap_err().to_string();
            assert!(
                error.contains("TABLE name is not supported"),
                "{sql}: {error}"
            );
        }
        for sql in [
            "INSERT INTO archive (ssn) TABLE secret UNION ALL SELECT ssn FROM users",
            "INSERT INTO archive (ssn) SELECT ssn FROM (TABLE secret) s",
            "UPDATE users SET n = (SELECT count(*) FROM (TABLE secret) s)",
            "DELETE FROM users WHERE id IN (TABLE secret)",
        ] {
            assert!(write_tables(sql).is_err(), "{sql}");
        }
        for sql in [
            "SELECT * FROM users WHERE id IN (TABLE secret)",
            "SELECT * FROM (TABLE secret) s",
            "SELECT * FROM users WHERE id IN (SELECT id FROM users UNION TABLE secret)",
        ] {
            assert!(tables(sql).is_err(), "{sql}");
        }
    }

    /// 引用符の中に`.`のある名前は、`schema.table`と区別できないので断る
    /// （PR #802のレビュー）
    #[test]
    fn quoted_names_with_a_dot_are_refused() {
        for sql in [
            r#"DELETE FROM "public.users""#,
            r#"INSERT INTO notes (body) SELECT ssn FROM "public.secret""#,
            r#"UPDATE public."a.b" SET x = 1"#,
        ] {
            assert!(write_tables(sql).is_err(), "{sql}");
        }
        assert!(tables(r#"SELECT * FROM "public.users""#).is_err());
        assert!(resolve_write_tables("DELETE FROM `app.users`", &MySqlDialect {}).is_err());
        // 普通の`schema.table`は、今までどおり
        assert_eq!(
            write_tables(r#"DELETE FROM "public"."users""#)
                .unwrap()
                .written,
            names(&["public.users"])
        );
    }

    /// PostgreSQLでは、引用符で囲まない名前を小文字にそろえる（PR #802の
    /// レビュー）。`Users`は`users`、`"Users"`は別の表
    #[test]
    fn unquoted_names_are_folded_like_postgresql() {
        assert_eq!(
            write_tables("DELETE FROM Public.Users").unwrap().written,
            names(&["public.users"])
        );
        assert_eq!(
            write_tables(r#"DELETE FROM public."Users""#)
                .unwrap()
                .written,
            names(&["public.Users"])
        );
        let both = write_tables(r#"INSERT INTO Notes SELECT * FROM "Notes" JOIN NOTES n ON true"#)
            .unwrap();
        assert_eq!(both.written, names(&["notes"]));
        assert_eq!(both.read, names(&["Notes", "notes"]));
        assert_eq!(
            tables(r#"SELECT * FROM Users JOIN "Users" u ON true"#).unwrap(),
            names(&["users", "Users"])
        );
        // MySQLは、書いたとおり
        assert_eq!(
            resolve_write_tables("DELETE FROM Users", &MySqlDialect {})
                .unwrap()
                .written,
            names(&["Users"])
        );
        // SQLiteは、引用符で囲んでも、大文字・小文字を区別しない
        assert_eq!(
            resolve_query_tables(
                r#"SELECT * FROM Users JOIN "USERS" u ON true"#,
                &SQLiteDialect {}
            )
            .unwrap(),
            names(&["users"])
        );
        let sqlite = resolve_write_tables(
            r#"INSERT INTO "Notes" SELECT * FROM NOTES"#,
            &SQLiteDialect {},
        )
        .unwrap();
        assert_eq!(sqlite.written, names(&["notes"]));
        assert_eq!(sqlite.read, names(&["notes"]));
    }

    /// 文字列のSQL・表の名前で表を読む関数は、表を拾えないので断る
    /// （PR #802のレビュー）
    #[test]
    fn functions_that_read_tables_named_in_text_are_refused() {
        for sql in [
            "INSERT INTO notes (body) VALUES (query_to_xml('SELECT ssn FROM secret', true, false, '')::text)",
            "INSERT INTO notes (body) SELECT table_to_xml('secret', true, false, '')::text",
            "UPDATE notes SET body = (SELECT pg_catalog.QUERY_TO_XML_AND_XMLSCHEMA('TABLE secret', true, false, '')::text)",
            "INSERT INTO notes (body) SELECT word FROM ts_stat('SELECT v FROM secret')",
            "INSERT INTO notes (body) SELECT * FROM crosstab('SELECT a, b, c FROM secret') AS t(a text, b text)",
            "DELETE FROM notes WHERE body = schema_to_xml('private', true, false, '')::text",
            "INSERT INTO notes (body) SELECT keyid FROM connectby('secret', 'id', 'parent_id', '1', 0) AS t(keyid text, parent_keyid text, level int)",
        ] {
            let error = write_tables(sql).unwrap_err().to_string();
            assert!(error.contains("named in a string"), "{sql}: {error}");
        }
        for sql in [
            "SELECT query_to_xml('SELECT ssn FROM secret', true, false, '') FROM notes",
            "SELECT * FROM notes, ts_stat('SELECT v FROM secret')",
        ] {
            let error = tables(sql).unwrap_err().to_string();
            assert!(error.contains("named in a string"), "{sql}: {error}");
        }
        // ほかの関数・普通のVALUESは、今までどおり
        assert!(write_tables("INSERT INTO notes (body) VALUES (upper($1)), (now()::text)").is_ok());
        assert!(tables("SELECT count(*), max(length(body)) FROM notes").is_ok());
    }

    /// ぶつかった今の行を消してから入れる書き方には、`delete`の許可も要る
    /// （PR #802のレビュー）
    #[test]
    fn replacing_writes_also_need_delete() {
        let mysql = |sql: &str| resolve_write_tables(sql, &MySqlDialect {}).unwrap();
        let sqlite = |sql: &str| resolve_write_tables(sql, &SQLiteDialect {}).unwrap();
        for tables in [
            mysql("REPLACE INTO notes (id, body) VALUES (1, 'x')"),
            sqlite("REPLACE INTO notes (id, body) VALUES (1, 'x')"),
            sqlite("INSERT OR REPLACE INTO notes (id, body) VALUES (1, 'x')"),
            sqlite("UPDATE OR REPLACE notes SET id = 1 WHERE id = 2"),
        ] {
            assert_eq!(tables.action, "write");
            assert!(tables.also_deletes, "{tables:?}");
            assert_eq!(tables.written, names(&["notes"]));
        }
        for tables in [
            mysql("INSERT INTO notes (id, body) VALUES (1, 'x')"),
            mysql("INSERT INTO notes (id) VALUES (1) ON DUPLICATE KEY UPDATE body = 'x'"),
            sqlite("INSERT OR IGNORE INTO notes (id, body) VALUES (1, 'x')"),
            sqlite("UPDATE OR IGNORE notes SET id = 1 WHERE id = 2"),
            sqlite("UPDATE notes SET body = 'x'"),
            sqlite("DELETE FROM notes"),
        ] {
            assert!(!tables.also_deletes, "{tables:?}");
        }
    }

    /// 分けられない形は、断る（#609）
    #[test]
    fn resolve_write_tables_rejects_what_it_cannot_split() {
        for sql in [
            "SELECT 1",
            "DROP TABLE users",
            "DELETE FROM users; DELETE FROM orders",
            "INSERT INTO archive WITH t AS (SELECT * FROM users) SELECT * FROM t",
            "WITH t AS (SELECT 1) DELETE FROM users",
        ] {
            assert!(write_tables(sql).is_err(), "{sql}");
        }
    }

    fn sorted_tables(sql: &str) -> Vec<String> {
        let mut result = tables(sql).unwrap();
        result.sort();
        result
    }

    #[test]
    fn resolve_query_tables_finds_table_referenced_via_subquery_in_from() {
        // A derived table in FROM is walked into (not rejected outright)
        // now, so the real table behind it is still found.
        assert_eq!(
            sorted_tables("SELECT x FROM (SELECT id AS x FROM secret_table) t"),
            vec!["secret_table".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_finds_table_referenced_via_scalar_subquery_in_projection() {
        // A previous version of this function only walked the top-level
        // FROM/JOIN clause and would have returned just ["users"] here,
        // silently missing "secret" - a caller checking per-table access
        // would never know this query also reads from "secret", letting a
        // policy gating it be bypassed entirely.
        assert_eq!(
            sorted_tables(
                "SELECT id, (SELECT ssn FROM secret WHERE secret.uid = users.id) AS ssn FROM users"
            ),
            vec!["secret".to_string(), "users".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_finds_table_referenced_via_where_exists_subquery() {
        assert_eq!(
            sorted_tables(
                "SELECT id FROM users WHERE EXISTS (SELECT 1 FROM secret WHERE secret.uid = users.id)"
            ),
            vec!["secret".to_string(), "users".to_string()]
        );
    }

    #[test]
    fn resolve_query_tables_finds_table_referenced_via_in_subquery() {
        assert_eq!(
            sorted_tables("SELECT id FROM users WHERE id IN (SELECT uid FROM banned)"),
            vec!["banned".to_string(), "users".to_string()]
        );
    }

    #[test]
    fn dialect_for_maps_each_database_type() {
        // Just confirm each variant maps to a dialect that can parse basic
        // SQL without panicking - the specific dialect choice per engine is
        // documented on `dialect_for` itself.
        for db_type in [
            DatabaseType::PostgreSQL,
            DatabaseType::MySQL,
            DatabaseType::MariaDB,
            DatabaseType::SQLite,
            DatabaseType::ClickHouse,
            DatabaseType::MongoDB,
            DatabaseType::Redis,
        ] {
            let dialect = dialect_for(db_type);
            assert!(validate_single_query_statement("SELECT 1", dialect.as_ref()).is_ok());
        }
    }
}
