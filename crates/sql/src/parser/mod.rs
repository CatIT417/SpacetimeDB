pub mod ad_hoc;
pub mod subscription;

use crate::ast::{Identifier, PrintOperator, SqlIdentifier, SqlLiteral, SqlSource, SqlTable};
use crate::errors::{SqlError, SqlResult, SqlUnsupported, Suggestion};
use crate::unsupported;
use error_stack::{Report, ResultExt};
use spacetimedb_lib::operator::{OpCmp, OpLogic, OpQuery};
use sqlparser::ast::{
    BinaryOperator, Expr, HiveDistributionStyle, JoinConstraint, JoinOperator, ObjectName, OrderByExpr, Query, Select,
    SelectItem, Statement, TableFactor, TableWithJoins, Value,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

/// Simplify to detect features of the syntax we don't support yet
/// Because we use [PostgreSqlDialect] in the compiler step it already protect against features
/// that are not in the standard SQL-92 but still need to check for completeness
pub(crate) trait Unsupported {
    fn unsupported(&self) -> bool;
}

impl Unsupported for bool {
    fn unsupported(&self) -> bool {
        *self
    }
}

impl<T> Unsupported for Option<T> {
    fn unsupported(&self) -> bool {
        self.is_some()
    }
}

impl<T> Unsupported for Vec<T> {
    fn unsupported(&self) -> bool {
        !self.is_empty()
    }
}

impl Unsupported for HiveDistributionStyle {
    fn unsupported(&self) -> bool {
        !matches!(self, HiveDistributionStyle::NONE)
    }
}

impl Unsupported for sqlparser::ast::GroupByExpr {
    fn unsupported(&self) -> bool {
        match self {
            sqlparser::ast::GroupByExpr::All => true,
            sqlparser::ast::GroupByExpr::Expressions(v) => v.unsupported(),
        }
    }
}

impl Unsupported for sqlparser::ast::WildcardAdditionalOptions {
    fn unsupported(&self) -> bool {
        self.opt_except.is_some()
            || self.opt_exclude.is_some()
            || self.opt_rename.is_some()
            || self.opt_replace.is_some()
    }
}

impl Unsupported for sqlparser::ast::TableAlias {
    fn unsupported(&self) -> bool {
        !self.columns.is_empty()
    }
}

#[macro_export]
macro_rules! unsupported {
    ($name:literal,$a:expr)=>{{
        if $a.unsupported() {
            return Err(SqlError::Unsupported(SqlUnsupported::FeatureNamed {
                name: stringify!($name),
                it: stringify!($a),
            }).into());
        }
    }};
    ($name:literal,$($a:expr),+$(,)?)=> {{
        $(unsupported!($name,$a);)+
    }};
}

/// Parse the `sql` using the `PostgreSqlDialect`.
fn parse_sql(source: SqlSource, sql: &str) -> SqlResult<Vec<Statement>> {
    Parser::parse_sql(&PostgreSqlDialect {}, sql)
        .change_context(SqlError::ParsingError(source))
        .attach_printable_lazy(|| sql.to_string())
}

/// Compiles the [Identifier] from a section of `SQL` that describes a table clause.
fn compile_table_factor(table: TableFactor) -> SqlResult<SqlTable> {
    match table {
        TableFactor::Table {
            name,
            alias,
            args,
            with_hints,
            version,
            partitions,
        } => {
            unsupported!("TableFactor", args, with_hints, version, partitions);

            let alias = if let Some(alias) = alias {
                unsupported!("TableFactor", alias);
                Some(alias.name.clone().into())
            } else {
                None
            };

            Ok(SqlTable::new(name.clone().try_into()?, alias))
        }
        x => Err(SqlUnsupported::Feature(format!("TableFactor: `{x}`")).into()),
    }
}

/// Check the [SqlTable] is not aliased, to avoid invalid constructs like `INSERT INTO t AS a`
fn compile_plain_table(table: SqlTable) -> SqlResult<SqlTable> {
    if table.alias.is_some() {
        Err(SqlUnsupported::TableAlias.into())
    } else {
        Ok(table)
    }
}

/// Compile a plain expression value
fn compile_plain_expr_value(value: Expr) -> SqlResult<SqlLiteral> {
    match value {
        Expr::Value(value) => compile_value(value),
        x => Err(SqlUnsupported::Expression(x).into()),
    }
}

fn compile_value(value: Value) -> SqlResult<SqlLiteral> {
    Ok(match value {
        Value::Number(num, _) => SqlLiteral::Number(num),
        Value::SingleQuotedString(s) => SqlLiteral::String(s),
        Value::DoubleQuotedString(s) => SqlLiteral::Identifier(Identifier::new(s)),
        Value::HexStringLiteral(s) => SqlLiteral::Hex(s),
        Value::Boolean(x) => SqlLiteral::Bool(x),
        x => {
            return Err(SqlUnsupported::Value(x).into());
        }
    })
}

/// Unified AST evaluation for the SQL parser
pub trait SqlParser {
    type Ast;
    type Expr;
    type Selection;
    type Projection;
    type InnerJoin;
    type JoinOn;
    type From;

    /// Compile a `SQL` `crud` statement into the target [Self::Ast].
    fn compile_statement(&self, statement: Statement) -> SqlResult<Self::Ast> {
        match statement {
            Statement::Query(query) => self.compile_query(*query),
            x => self.compile_other_statement(x),
        }
    }
    /// Compile a `SQL` `crud` statement other than `SELECT`.
    fn compile_other_statement(&self, statement: Statement) -> SqlResult<Self::Ast>;
    /// Compile a `SQL` `query` statement into the target [Self::Ast].
    fn compile_query(&self, query: Query) -> SqlResult<Self::Ast>;
    /// Compile a `SQL` `SELECT` statement into the target [Self::Ast].
    fn compile_select(&self, select: Select, limit: Option<Expr>, order_by: Vec<OrderByExpr>) -> SqlResult<Self::Ast>;
    /// Create the [Self::Projection] for a wildcard fragment.
    fn make_projection_wildcard(&self, name: Option<ObjectName>) -> SqlResult<Self::Projection>;
    /// Compile a `SQL` `SELECT` projection expression into the target [Self::Projection].
    fn compile_projection_expr(&self, expr: Expr, alias: Option<Identifier>) -> SqlResult<Self::Projection>;

    /// Compile a `SQL` `SELECT` projection into the target [Self::Projection].
    fn compile_projection(&self, expr: SelectItem) -> SqlResult<Self::Projection> {
        match expr {
            SelectItem::QualifiedWildcard(name, opt) => {
                unsupported!("SELECT", opt);
                self.make_projection_wildcard(Some(name))
            }
            SelectItem::Wildcard(opt) => {
                unsupported!("SELECT", opt);
                self.make_projection_wildcard(None)
            }
            SelectItem::UnnamedExpr(x) => self.compile_projection_expr(x, None),
            SelectItem::ExprWithAlias { expr, alias } => self.compile_projection_expr(expr, Some(alias.into())),
        }
    }

    /// Compile a `SQL` `FROM` clause into the target [Self::From] components.
    fn compile_from(&self, from: &[TableWithJoins]) -> SqlResult<(SqlTable, Vec<Self::InnerJoin>)> {
        if from.len() > 1 {
            return Err(SqlUnsupported::MultipleTables.into());
        }
        let root_table = from.first().ok_or(SqlError::MissingFrom)?;
        let joins = self.compile_joins(root_table)?;
        Ok((compile_table_factor(root_table.relation.clone())?, joins))
    }

    /// Create the [Self::Selection].
    fn make_selection(&self, predicate: Expr) -> SqlResult<Self::Selection>;

    /// Compile a `SQL` `WHERE` clause into the target [Self::Selection].
    fn compile_where(&self, predicate: Option<Expr>) -> SqlResult<Option<Self::Selection>> {
        predicate.map_or(Ok(None), |p| self.make_selection(p).map(Some))
    }
    /// Extract the column for a join expression.
    fn extract_column_join(&self, expr: Self::Expr) -> SqlResult<SqlIdentifier>;

    /// Compile a `SQL` `JOIN ON` clause into the target [Self::InnerJoin] components.
    fn compile_join_on(&self, expr: &Expr) -> SqlResult<Self::JoinOn>;

    /// Compile a `SQL` `INNER JOIN` clause into the target [Self::InnerJoin] component.
    fn compile_join_inner(&self, table: SqlTable, on: &Expr) -> SqlResult<Self::InnerJoin>;
    /// Compile a `SQL` `JOINs` clauses.
    fn compile_joins(&self, root_table: &TableWithJoins) -> SqlResult<Vec<Self::InnerJoin>> {
        let mut result = Vec::new();
        for join in &root_table.joins {
            let table = compile_table_factor(join.relation.clone())?;
            match &join.join_operator {
                JoinOperator::Inner(constraint) => match constraint {
                    JoinConstraint::On(on) => {
                        result.push(self.compile_join_inner(table, on)?);
                    }
                    x => {
                        return Err(SqlUnsupported::JoinConstraint(format!("{x:?}")).into());
                    }
                },
                x => {
                    return Err(Report::from(SqlUnsupported::JoinKind(format!("{x:?}")))
                        .attach_printable(Suggestion::ChangeJoinToInner));
                }
            }
        }

        Ok(result)
    }

    /// Compile the valid binary operators
    fn compile_op(&self, op: BinaryOperator) -> SqlResult<OpQuery>;

    /// Compile a `SQL` binary operation.
    fn compile_bin_op(&self, op: BinaryOperator, lhs: Expr, rhs: Expr) -> SqlResult<(OpQuery, Self::Expr, Self::Expr)> {
        let op = self.compile_op(op)?;
        let lhs = self.compile_expr(lhs)?;
        let rhs = self.compile_expr(rhs)?;

        Ok((op, lhs, rhs))
    }

    /// Compile a `SQL` expression into the target [Self::Expr].
    fn compile_expr(&self, of: Expr) -> SqlResult<Self::Expr>;
}

#[cfg(test)]
mod tests {
    /// Utility for matching a `SqlError` with an `Unsupported` variant.
    #[macro_export]
    macro_rules! matches_unsupported {
        ($expression:expr, $pattern:pat) => {
            match $expression {
                Err(actual) => {
                    assert!(
                        matches!(
                            actual.current_context(),
                            $crate::parser::SqlError::Unsupported($pattern)
                        ),
                        "got: {:?}",
                        actual.current_context()
                    );
                }
                x => panic!("Expected an error, got: {:?}", x),
            }
        };
    }
}
