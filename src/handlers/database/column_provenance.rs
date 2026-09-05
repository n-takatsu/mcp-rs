//! Resolves which `table.column` each column of a `SELECT` result set came
//! from, by parsing the SQL text with `sqlparser` rather than trusting the
//! caller to say which table it queried. This is a pure, DB-independent
//! function: it only reasons about the query text, never about actual
//! returned data.
//!
//! The result is deliberately conservative. Anything this module cannot
//! confidently resolve comes back as [`ColumnProvenance::Unknown`], or as an
//! outright [`ProvenanceError`] for query shapes it refuses to reason about
//! at all (CTEs, set operations, subqueries in `FROM`, multiple statements).
//! Callers that use this to decide whether to expose a column's raw value
//! must treat both cases as "cannot prove this is safe" and fail closed,
//! since this is a security boundary, not a best-effort hint.

use sqlparser::ast::{
    Expr, ObjectName, ObjectNamePart, Query, Select, SelectItem, SelectItemQualifiedWildcardKind,
    SetExpr, Statement, TableFactor,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::HashMap;
use std::fmt;

/// Where a single projected column came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnProvenance {
    /// Confidently resolved to a single source table and column.
    Resolved { table: String, column: String },
    /// Could not be attributed to a single table/column (ambiguous
    /// unqualified reference in a multi-table query, a computed expression,
    /// etc). This is not a "safe to expose" result - the caller has no way
    /// to know whether this came from an encrypted column, so it must be
    /// treated the same as a proven-encrypted one (masked) rather than
    /// passed through.
    Unknown,
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

/// Per-column provenance for a `SELECT`'s result set, or a single marker
/// meaning "every actual result column belongs to this one table" for a bare
/// `SELECT * FROM <table>` / `SELECT t.* FROM <table> t`, whose column count
/// can only be known once the query has actually been run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionResolution {
    PerColumn(Vec<ColumnProvenance>),
    WildcardTable(String),
}

/// Parses `sql` as a single `SELECT` statement and resolves its projected
/// columns' table origins.
pub fn resolve_column_provenance(sql: &str) -> Result<ProjectionResolution, ProvenanceError> {
    let statements = Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .map_err(|e| unsupported(format!("SQL parse error: {e}")))?;

    let [statement] = statements.as_slice() else {
        return Err(unsupported("expected exactly one SQL statement"));
    };

    let Statement::Query(query) = statement else {
        return Err(unsupported("expected a SELECT statement"));
    };

    resolve_query(query)
}

fn resolve_query(query: &Query) -> Result<ProjectionResolution, ProvenanceError> {
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

    resolve_select(select)
}

fn resolve_select(select: &Select) -> Result<ProjectionResolution, ProvenanceError> {
    let mut alias_to_table: HashMap<String, String> = HashMap::new();
    let mut all_tables: Vec<String> = Vec::new();

    for table_with_joins in &select.from {
        collect_table(
            &table_with_joins.relation,
            &mut alias_to_table,
            &mut all_tables,
        )?;
        for join in &table_with_joins.joins {
            collect_table(&join.relation, &mut alias_to_table, &mut all_tables)?;
        }
    }

    // A lone wildcard is the only projection shape whose column count isn't
    // known from the SQL text alone, so it's handled before the general,
    // positional per-column path below.
    if let [only_item] = select.projection.as_slice() {
        match only_item {
            SelectItem::Wildcard(_) => {
                return match all_tables.as_slice() {
                    [single] => Ok(ProjectionResolution::WildcardTable(single.clone())),
                    _ => Err(unsupported(
                        "SELECT * across zero or multiple tables cannot be resolved",
                    )),
                };
            }
            SelectItem::QualifiedWildcard(
                SelectItemQualifiedWildcardKind::ObjectName(qualifier),
                _,
            ) => {
                let qualifier = object_name_to_string(qualifier)?;
                let table = alias_to_table
                    .get(&qualifier)
                    .cloned()
                    .ok_or_else(|| unsupported(format!("unknown table qualifier '{qualifier}'")))?;
                return Ok(ProjectionResolution::WildcardTable(table));
            }
            _ => {}
        }
    }

    let single_table = match all_tables.as_slice() {
        [single] => Some(single.clone()),
        _ => None,
    };

    let mut resolved = Vec::with_capacity(select.projection.len());
    for item in &select.projection {
        let provenance = match item {
            SelectItem::UnnamedExpr(expr)
            | SelectItem::ExprWithAlias { expr, .. }
            | SelectItem::ExprWithAliases { expr, .. } => {
                resolve_expr(expr, &single_table, &alias_to_table)
            }
            SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
                // A wildcard mixed with other projection items (e.g.
                // `SELECT id, * FROM t`) can expand to an unknown number of
                // columns, which would break the 1:1 positional mapping
                // every other branch here relies on.
                return Err(unsupported(
                    "a wildcard mixed with other projection items is not supported",
                ));
            }
        };
        resolved.push(provenance);
    }

    Ok(ProjectionResolution::PerColumn(resolved))
}

fn resolve_expr(
    expr: &Expr,
    single_table: &Option<String>,
    alias_to_table: &HashMap<String, String>,
) -> ColumnProvenance {
    match expr {
        Expr::Identifier(ident) => match single_table {
            Some(table) => ColumnProvenance::Resolved {
                table: table.clone(),
                column: ident.value.clone(),
            },
            None => ColumnProvenance::Unknown,
        },
        Expr::CompoundIdentifier(parts) if parts.len() >= 2 => {
            let column = parts[parts.len() - 1].value.clone();
            let qualifier = &parts[parts.len() - 2].value;
            match alias_to_table.get(qualifier) {
                Some(table) => ColumnProvenance::Resolved {
                    table: table.clone(),
                    column,
                },
                None => ColumnProvenance::Unknown,
            }
        }
        _ => ColumnProvenance::Unknown,
    }
}

fn collect_table(
    factor: &TableFactor,
    alias_to_table: &mut HashMap<String, String>,
    all_tables: &mut Vec<String>,
) -> Result<(), ProvenanceError> {
    match factor {
        TableFactor::Table { name, alias, .. } => {
            let table_name = object_name_to_string(name)?;
            all_tables.push(table_name.clone());
            alias_to_table.insert(table_name.clone(), table_name.clone());
            if let Some(alias) = alias {
                alias_to_table.insert(alias.name.value.clone(), table_name);
            }
            Ok(())
        }
        _ => Err(unsupported(
            "FROM/JOIN must reference plain tables, not subqueries or table-valued functions",
        )),
    }
}

/// Takes the last identifier part of a possibly schema-qualified name
/// (`public.users` -> `users`), matching how table names are configured
/// elsewhere in this module without schema prefixes.
fn object_name_to_string(name: &ObjectName) -> Result<String, ProvenanceError> {
    match name.0.last() {
        Some(ObjectNamePart::Identifier(ident)) => Ok(ident.value.clone()),
        _ => Err(unsupported("unsupported table name form")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(sql: &str) -> Result<ProjectionResolution, ProvenanceError> {
        resolve_column_provenance(sql)
    }

    fn resolved(table: &str, column: &str) -> ColumnProvenance {
        ColumnProvenance::Resolved {
            table: table.to_string(),
            column: column.to_string(),
        }
    }

    #[test]
    fn single_table_unqualified_column() {
        let result = resolve("SELECT email FROM users").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![resolved("users", "email")])
        );
    }

    #[test]
    fn single_table_with_alias_unqualified_column() {
        let result = resolve("SELECT email FROM users u").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![resolved("users", "email")])
        );
    }

    #[test]
    fn single_table_with_alias_qualified_column() {
        let result = resolve("SELECT u.email FROM users u").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![resolved("users", "email")])
        );
    }

    #[test]
    fn join_with_qualified_columns_resolves_both() {
        let result =
            resolve("SELECT u.email, o.total FROM users u JOIN orders o ON u.id = o.user_id")
                .unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![
                resolved("users", "email"),
                resolved("orders", "total"),
            ])
        );
    }

    #[test]
    fn join_with_unqualified_column_is_ambiguous() {
        let result =
            resolve("SELECT email FROM users u JOIN orders o ON u.id = o.user_id").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![ColumnProvenance::Unknown])
        );
    }

    #[test]
    fn wildcard_single_table_resolves_to_wildcard_table() {
        let result = resolve("SELECT * FROM users").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::WildcardTable("users".to_string())
        );
    }

    #[test]
    fn wildcard_across_join_is_unsupported() {
        let err = resolve("SELECT * FROM users u JOIN orders o ON u.id = o.user_id").unwrap_err();
        assert!(err
            .to_string()
            .contains("cannot determine column provenance"));
    }

    #[test]
    fn qualified_wildcard_resolves_to_that_tables_wildcard() {
        let result = resolve("SELECT u.* FROM users u JOIN orders o ON u.id = o.user_id").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::WildcardTable("users".to_string())
        );
    }

    #[test]
    fn mixed_wildcard_and_explicit_column_is_unsupported() {
        assert!(resolve("SELECT id, * FROM users").is_err());
    }

    #[test]
    fn subquery_in_from_is_unsupported() {
        assert!(resolve("SELECT x FROM (SELECT 1 AS x) t").is_err());
    }

    #[test]
    fn cte_is_unsupported() {
        assert!(resolve("WITH t AS (SELECT 1 AS x) SELECT x FROM t").is_err());
    }

    #[test]
    fn union_is_unsupported() {
        assert!(resolve("SELECT a FROM t1 UNION SELECT b FROM t2").is_err());
    }

    #[test]
    fn function_call_column_is_unknown() {
        let result = resolve("SELECT UPPER(email) FROM users").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![ColumnProvenance::Unknown])
        );
    }

    #[test]
    fn schema_qualified_table_name_uses_last_part() {
        let result = resolve("SELECT email FROM public.users").unwrap();
        assert_eq!(
            result,
            ProjectionResolution::PerColumn(vec![resolved("users", "email")])
        );
    }

    #[test]
    fn non_select_statement_is_unsupported() {
        assert!(resolve("INSERT INTO users (email) VALUES ('a@b.com')").is_err());
    }

    #[test]
    fn unparseable_sql_is_an_error() {
        assert!(resolve("this is not valid SQL at all ###").is_err());
    }
}
