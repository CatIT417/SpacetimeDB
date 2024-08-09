//! This module provides the implementation of the [`ParserAdHoc`] struct and its associated methods.
//! It is used to parse `SQL` queries and compile them into an abstract syntax tree
//! defined in  [crate::ast::ad_hoc].
//!

use crate::ast::ad_hoc::*;
use crate::ast::{Identifier, SqlIdentifier, SqlSource, SqlTable};
use crate::errors::{SqlError, SqlResult, SqlUnsupported, Suggestion};
use crate::parser::{
    compile_plain_expr_value, compile_plain_table, compile_table_factor, compile_value, parse_sql, SqlParser,
    Unsupported,
};
use crate::unsupported;
use error_stack::{Report, ResultExt};

use spacetimedb_lib::operator::{OpCmp, OpLogic, OpQuery};
use sqlparser::ast::{
    Assignment, BinaryOperator, Distinct, Expr, Function, FunctionArg, FunctionArgExpr, Ident, ObjectName, OrderByExpr,
    Query, Select, SetExpr, Statement, TableFactor, Value, Values,
};

/// Parser for the [crate::ast::ad_hoc] `SQL` queries.
pub struct ParserAdHoc<'a> {
    sql: &'a str,
}

impl<'a> ParserAdHoc<'a> {
    pub fn new(sql: &'a str) -> Self {
        Self { sql }
    }

    pub fn parse(&self) -> SqlResult<Vec<SqlAst>> {
        let ast = parse_sql(SqlSource::General, self.sql)?;
        ast.into_iter()
            .map(|stmt| {
                self.compile_statement(stmt)
                    .attach_printable_lazy(|| self.sql.to_string())
            })
            .collect()
    }

    fn compile_limit(&self, limit: Option<Expr>) -> SqlResult<Option<u64>> {
        limit
            .map(|x| match x {
                Expr::Value(Value::Number(num, _)) => Ok(num.parse::<u64>().unwrap()),
                x => Err(SqlUnsupported::LimitExpr(x).into()),
            })
            .transpose()
    }

    fn compile_order_by(&self, order_by: Vec<OrderByExpr>) -> SqlResult<Vec<SqlOrder>> {
        order_by
            .into_iter()
            .map(|order_by| {
                unsupported!("ORDER BY", order_by.nulls_first);
                let column = match order_by.expr {
                    Expr::Identifier(ident) => SqlIdentifier::new(ident.into()),
                    _ => return Err(SqlUnsupported::OrderByExpr(order_by.expr).into()),
                };
                let asc = !matches!(order_by.asc, Some(false));
                Ok(SqlOrder { ident: column, asc })
            })
            .collect()
    }

    fn compile_argument(&self, arg: &FunctionArg) -> SqlResult<SqlArg> {
        match arg {
            FunctionArg::Named { .. } => Err(SqlUnsupported::NamedArguments(arg.clone()).into()),
            FunctionArg::Unnamed(arg) => Ok(match arg {
                FunctionArgExpr::Expr(Expr::Identifier(name)) => SqlArg::Ident(name.clone().into()),
                FunctionArgExpr::Expr(Expr::CompoundIdentifier(name)) => SqlArg::Ident(name.clone().into()),
                FunctionArgExpr::Wildcard => SqlArg::Wildcard,
                _ => {
                    return Err(SqlUnsupported::Argument(arg.clone()).into());
                }
            }),
        }
    }
    fn compile_aggregate(&self, f: Function, alias: Option<Identifier>) -> SqlResult<SqlAgg> {
        unsupported!("Function", f.over, f.order_by);
        let name = f.name.to_string().to_lowercase();

        let alias = if let Some(alias) = alias {
            alias
        } else {
            return Err(SqlUnsupported::AdHocFunctionAlias { name }.into());
        };

        let arg = match &f.args[..] {
            [arg] => self.compile_argument(arg)?,
            _ => {
                return Err(SqlUnsupported::MultipleArgumentsAggregate(f).into());
            }
        };

        match name.as_str() {
            "count" => Ok(SqlAgg::Count {
                arg,
                distinct: f.distinct,
                alias,
            }),
            "sum" => match arg {
                SqlArg::Ident(ident) => Ok(SqlAgg::Sum { ident, alias }),
                _ => Err(SqlUnsupported::ArgumentAggregate.into()),
            },
            _ => Err(SqlUnsupported::Function(f).into()),
        }
    }

    fn compile_insert(&self, table_name: ObjectName, cols: Vec<Ident>, data: Values) -> SqlResult<SqlAst> {
        let table = compile_plain_table(table_name.try_into()?)?;

        let columns: Vec<SqlIdentifier> = cols.into_iter().map(Into::into).collect::<_>();

        let values: Vec<_> = data
            .rows
            .into_iter()
            .map(|row| row.into_iter().map(compile_plain_expr_value).collect::<Result<_, _>>())
            .collect::<Result<_, _>>()?;

        Ok(SqlAst::Insert(SqlInsert {
            table,
            idents: columns.into(),
            values: values.into(),
        }))
    }

    fn compile_update(
        &self,
        table: TableFactor,
        assignments: Vec<Assignment>,
        selection: Option<Expr>,
    ) -> SqlResult<SqlAst> {
        let table = compile_plain_table(compile_table_factor(table)?)?;

        let mut assigns = Vec::with_capacity(assignments.len());
        for set in assignments {
            let col = set.id.into();
            let value = compile_plain_expr_value(set.value)?;
            assigns.push(SqlAssignment { ident: col, value });
        }

        let selection = self.compile_where(selection)?;

        Ok(SqlAst::Update(SqlUpdate {
            table,
            assignments: assigns.into(),
            selection,
        }))
    }

    fn compile_delete(&self, table: TableFactor, selection: Option<Expr>) -> SqlResult<SqlAst> {
        let table = compile_plain_table(compile_table_factor(table)?)?;
        let selection = self.compile_where(selection)?;

        Ok(SqlAst::Delete(SqlDelete { table, selection }))
    }

    fn compile_set_config(&self, name: ObjectName, value: Vec<Expr>) -> SqlResult<SqlAst> {
        let name = name.try_into()?;

        let value = match value.as_slice() {
            [first] => first.clone(),
            _ => {
                return Err(SqlUnsupported::SetValues(value).into());
            }
        };

        let value = match value {
            Expr::Value(value) => compile_value(value)?,
            x => {
                return Err(SqlUnsupported::Expression(x).into());
            }
        };

        Ok(SqlAst::SetVar(SqlSetVar { name, value }))
    }

    fn compile_read_config(&self, names: Vec<Ident>) -> SqlResult<SqlAst> {
        let name = match names.as_slice() {
            [first] => first.clone().into(),
            _ => {
                return Err(SqlUnsupported::ShowVariable(names).into());
            }
        };
        Ok(SqlAst::ReadVar(SqlReadVar { name }))
    }
}

impl SqlParser for ParserAdHoc<'_> {
    type Ast = SqlAst;
    type Expr = SqlExpr;
    type Selection = SqlSelection;
    type Projection = SqlProject;
    type InnerJoin = SqlJoin;
    type JoinOn = SqlOn;
    type From = SqlFrom;

    fn compile_other_statement(&self, statement: Statement) -> SqlResult<Self::Ast> {
        match statement {
            Statement::Insert {
                or,
                into,
                table_name,
                columns,
                overwrite,
                source,
                partitioned,
                after_columns,
                table,
                on,
                returning,
            } => {
                unsupported!(
                    "INSERT",
                    or,
                    overwrite,
                    partitioned,
                    after_columns,
                    table,
                    on,
                    returning
                );
                if into {
                    let values = match *source.body {
                        SetExpr::Values(values) => values,
                        x => {
                            return Err(SqlUnsupported::InsertWithValues(x).into());
                        }
                    };

                    self.compile_insert(table_name, columns, values)
                } else {
                    Err(SqlError::InsertNoValues.into())
                }
            }
            Statement::Update {
                table,
                assignments,
                from,
                selection,
                returning,
            } => {
                unsupported!("UPDATE", from, returning, table.joins);
                self.compile_update(table.relation, assignments, selection)
            }
            Statement::Delete {
                tables,
                from,
                using,
                selection,
                returning,
            } => {
                unsupported!("DELETE", using, returning, tables);
                if from.len() != 1 {
                    unsupported!("DELETE (multiple tables)", tables);
                }

                let table = from.first().unwrap().clone();
                unsupported!("DELETE", table.joins);

                self.compile_delete(table.relation, selection)
            }
            Statement::SetVariable {
                local,
                hivevar,
                variable,
                value,
            } => {
                unsupported!("SET", local, hivevar);
                self.compile_set_config(variable, value)
            }
            Statement::ShowVariable { variable } => self.compile_read_config(variable),
            x => Err(SqlUnsupported::Feature(format!("Statement: `{x}`")).into()),
        }
    }

    fn compile_query(&self, query: Query) -> SqlResult<Self::Ast> {
        unsupported!("QUERY", query.fetch, query.offset, query.locks, query.with);

        match *query.body {
            SetExpr::Select(select) => {
                unsupported!(
                    "SELECT",
                    select.into,
                    select.lateral_views,
                    select.group_by,
                    select.having,
                    select.sort_by,
                    select.top,
                    select.cluster_by,
                    select.distribute_by,
                    select.named_window,
                    select.qualify,
                );

                self.compile_select(*select, query.limit, query.order_by)
            }
            x => Err(SqlUnsupported::Feature(format!("Query statement `{x:?}`")).into()),
        }
    }

    fn compile_select(&self, select: Select, limit: Option<Expr>, order_by: Vec<OrderByExpr>) -> SqlResult<Self::Ast> {
        let distinct = if let Some(distinct) = select.distinct {
            match distinct {
                Distinct::On(_) => return Err(SqlUnsupported::DistinctOn.into()),
                Distinct::Distinct => true,
            }
        } else {
            false
        };

        let limit = self.compile_limit(limit)?;

        let from = self.compile_from(&select.from)?.into();

        let project: Vec<_> = select
            .projection
            .into_iter()
            .map(|item| self.compile_projection(item))
            .collect::<Result<_, _>>()?;

        let project = project.into();

        let selection = self.compile_where(select.selection)?;

        let order_by = self.compile_order_by(order_by)?.into();

        Ok(SqlAst::Select(SqlSelect {
            distinct,
            from,
            project,
            order_by,
            selection,
            limit,
        }))
    }

    fn make_projection_wildcard(&self, name: Option<ObjectName>) -> SqlResult<Self::Projection> {
        if let Some(name) = name {
            Ok(SqlProject::QualifiedWildcard {
                table: name.try_into()?,
            })
        } else {
            Ok(SqlProject::Wildcard)
        }
    }

    fn compile_projection_expr(&self, expr: Expr, alias: Option<Identifier>) -> SqlResult<Self::Projection> {
        Ok(match expr {
            Expr::Identifier(name) => SqlProject::Ident {
                column: name.into(),
                alias,
            },
            Expr::CompoundIdentifier(ident) => SqlProject::Ident {
                column: ident.into(),
                alias,
            },
            Expr::Function(f) => SqlProject::Summarize(self.compile_aggregate(f, alias)?),
            x => {
                return Err(
                    Report::from(SqlUnsupported::AdHocProjection(x)).attach_printable(Suggestion::ProjectionAdHoc)
                )
            }
        })
    }

    fn make_selection(&self, predicate: Expr) -> SqlResult<Self::Selection> {
        Ok(SqlSelection::new(self.compile_expr(predicate)?))
    }

    fn extract_column_join(&self, expr: Self::Expr) -> SqlResult<SqlIdentifier> {
        match expr {
            SqlExpr::Ident(column) => Ok(column),
            _ => Err(SqlUnsupported::AdHocJoinOnExp.into()),
        }
    }

    fn compile_join_on(&self, expr: &Expr) -> SqlResult<Self::JoinOn> {
        match expr {
            Expr::BinaryOp { left, op, right } => {
                let op = self.compile_op(op.clone())?;
                let lhs = self.compile_expr(*left.clone())?;
                let rhs = self.compile_expr(*right.clone())?;

                match op {
                    OpQuery::Cmp(op) => {
                        let lhs_col = self.extract_column_join(lhs)?;
                        let rhs_col = self.extract_column_join(rhs)?;
                        Ok(SqlOn::Cmp {
                            op,
                            lhs: lhs_col,
                            rhs: rhs_col,
                        })
                    }
                    OpQuery::Logic(op) => {
                        let lhs_on = self.compile_join_on(left)?;
                        let rhs_on = self.compile_join_on(right)?;
                        Ok(SqlOn::Logic {
                            op,
                            lhs: Box::new(lhs_on),
                            rhs: Box::new(rhs_on),
                        })
                    }
                }
            }
            _ => Err(SqlUnsupported::JoinConstraint(format!("{expr:?}")).into()),
        }
    }

    fn compile_join_inner(&self, table: SqlTable, on: &Expr) -> SqlResult<Self::InnerJoin> {
        let on_clause = self.compile_join_on(on)?;
        Ok(SqlJoin::Inner {
            rhs: table,
            on: on_clause,
        })
    }

    fn compile_op(&self, op: BinaryOperator) -> SqlResult<OpQuery> {
        Ok(match op {
            BinaryOperator::Gt => OpCmp::Gt.into(),
            BinaryOperator::Lt => OpCmp::Lt.into(),
            BinaryOperator::GtEq => OpCmp::GtEq.into(),
            BinaryOperator::LtEq => OpCmp::LtEq.into(),
            BinaryOperator::Eq => OpCmp::Eq.into(),
            BinaryOperator::NotEq => OpCmp::NotEq.into(),
            BinaryOperator::And => OpLogic::And.into(),
            BinaryOperator::Or => OpLogic::Or.into(),
            op => {
                return Err(SqlUnsupported::BinaryOp(op).into());
            }
        })
    }

    fn compile_expr(&self, of: Expr) -> SqlResult<Self::Expr> {
        Ok(match of {
            Expr::Identifier(name) => SqlExpr::Ident(SqlIdentifier::new(name.into())),
            Expr::CompoundIdentifier(ident) => SqlExpr::Ident(ident.into()),
            Expr::Value(x) => SqlExpr::Literal(compile_value(x)?),
            Expr::BinaryOp { left, op, right } => {
                let (op, lhs, rhs) = self.compile_bin_op(op, *left, *right)?;

                SqlExpr::BinOp {
                    op,
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                }
            }
            Expr::Nested(x) => {
                return self.compile_expr(*x);
            }
            x => {
                return Err(SqlUnsupported::Expression(x).into());
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::ad_hoc::*;
    use crate::ast::{PrintOperator, SqlLiteral, SqlTable};
    use crate::errors::{SqlResult, SqlUnsupported};
    use crate::matches_unsupported;
    use crate::parser::ad_hoc::ParserAdHoc;
    use spacetimedb_lib::operator::{OpCmp, OpQuery};
    use sqlparser::ast::{Expr, FunctionArgExpr};

    fn parse(sql: &str) -> SqlResult<Vec<SqlAst>> {
        ParserAdHoc::new(sql).parse()
    }
    fn parse_one(sql: &str) -> SqlResult<SqlAst> {
        let ast = parse(sql)?;
        assert_eq!(ast.len(), 1);
        Ok(ast.into_iter().next().unwrap())
    }

    fn parse_select(sql: &str) -> SqlResult<SqlSelect> {
        match parse_one(sql)? {
            SqlAst::Select(x) => Ok(x),
            _ => {
                unreachable!()
            }
        }
    }

    fn parse_insert(sql: &str) -> SqlResult<SqlInsert> {
        match parse_one(sql)? {
            SqlAst::Insert(x) => Ok(x),
            _ => {
                unreachable!()
            }
        }
    }

    fn parse_update(sql: &str) -> SqlResult<SqlUpdate> {
        match parse_one(sql)? {
            SqlAst::Update(x) => Ok(x),
            _ => {
                unreachable!()
            }
        }
    }

    fn parse_delete(sql: &str) -> SqlResult<SqlDelete> {
        match parse_one(sql)? {
            SqlAst::Delete(x) => Ok(x),
            _ => {
                unreachable!()
            }
        }
    }

    fn parse_set_var(sql: &str) -> SqlResult<SqlSetVar> {
        match parse_one(sql)? {
            SqlAst::SetVar(x) => Ok(x),
            _ => {
                unreachable!()
            }
        }
    }

    fn parse_read_var(sql: &str) -> SqlResult<SqlReadVar> {
        match parse_one(sql)? {
            SqlAst::ReadVar(x) => Ok(x),
            _ => {
                unreachable!()
            }
        }
    }

    /// Verify we can project
    #[test]
    fn compile_project() -> SqlResult<()> {
        let ast = parse_select("SELECT * FROM a")?;
        assert_eq!(ast.from.root, "a".into());
        assert_eq!(&*ast.project, &[SqlProject::Wildcard]);

        let ast = parse_select("SELECT a.* FROM a")?;
        assert_eq!(ast.from.root, "a".into());
        assert_eq!(&*ast.project, [SqlProject::QualifiedWildcard { table: "a".into() }]);

        let ast = parse_select("SELECT a FROM b")?;
        assert_eq!(
            &*ast.project,
            [SqlProject::Ident {
                column: "a".into(),
                alias: None
            }]
        );

        // Verify we don't support literals
        let ast = parse("SELECT 1 FROM a");
        matches_unsupported!(ast, SqlUnsupported::AdHocProjection(Expr::Value(_)));

        // Verify we don't support nulls
        let ast = parse("SELECT null FROM a");
        matches_unsupported!(ast, SqlUnsupported::AdHocProjection(Expr::Value(_)));

        Ok(())
    }

    /// Verify we can compile aliases
    #[test]
    fn compile_alias() -> SqlResult<()> {
        let ast = parse_select("SELECT a AS b FROM t")?;
        assert_eq!(
            ast.project[0],
            SqlProject::Ident {
                column: "a".into(),
                alias: Some("b".into())
            }
        );

        let ast = parse_select("SELECT a.d AS b FROM t")?;
        assert_eq!(
            ast.project[0],
            SqlProject::Ident {
                column: "a.d".into(),
                alias: Some("b".into())
            }
        );

        let ast = parse_select("SELECT * FROM t AS a")?;
        assert_eq!(ast.from.root, SqlTable::new("t".into(), Some("a".into())));

        let ast = parse_select("SELECT * FROM t JOIN b AS c ON t.a = c.b")?;
        assert_eq!(ast.from.root, "t".into());
        assert_eq!(
            ast.from.joins[0],
            SqlJoin::Inner {
                rhs: SqlTable::new("b".into(), Some("c".into())),
                on: SqlOn::Cmp {
                    op: OpCmp::Eq,
                    lhs: "t.a".into(),
                    rhs: "c.b".into(),
                }
            }
        );

        let ast = parse_select("SELECT COUNT(*) AS b FROM t")?;
        assert_eq!(
            ast.project[0],
            SqlProject::Summarize(SqlAgg::Count {
                arg: SqlArg::Wildcard,
                distinct: false,
                alias: "b".into()
            })
        );

        let ast = parse_select("SELECT Sum(a) AS b FROM t")?;

        assert_eq!(
            ast.project[0],
            SqlProject::Summarize(SqlAgg::Sum {
                ident: "a".into(),
                alias: "b".into()
            })
        );

        // Verify we don't support SUM/COUNT without an alias
        let ast = parse_select("SELECT COUNT(*) FROM t");
        matches_unsupported!(ast, SqlUnsupported::AdHocFunctionAlias { .. });

        let ast = parse_select("SELECT Sum(a)  FROM t");
        matches_unsupported!(ast, SqlUnsupported::AdHocFunctionAlias { .. });

        Ok(())
    }

    #[test]
    fn compile_where() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlExpr) -> SqlResult<()> {
            let ast = parse_select(sql)?;
            assert_eq!(ast.selection, Some(SqlSelection::new(expected)));
            Ok(())
        }
        for op in [OpCmp::Eq, OpCmp::NotEq, OpCmp::Gt, OpCmp::GtEq, OpCmp::Lt, OpCmp::LtEq] {
            check(
                &format!("SELECT * FROM t WHERE a {} 1", PrintOperator { op: op.into() }),
                SqlExpr::cmp(
                    op,
                    SqlExpr::Ident("a".into()),
                    SqlExpr::Literal(SqlLiteral::Number("1".into())),
                ),
            )?;
        }

        Ok(())
    }

    #[test]
    fn compile_ands_ors() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlExpr) -> SqlResult<()> {
            let ast = parse_select(sql)?;
            assert_eq!(ast.selection, Some(SqlSelection::new(expected)));
            Ok(())
        }

        check("SELECT * FROM t WHERE a", SqlExpr::Ident("a".into()))?;

        check(
            "SELECT * FROM t WHERE a AND c AND d",
            SqlExpr::and(
                SqlExpr::and(SqlExpr::Ident("a".into()), SqlExpr::Ident("c".into())),
                SqlExpr::Ident("d".into()),
            ),
        )?;

        check(
            "SELECT * FROM t WHERE a AND t.b = 1",
            SqlExpr::and(
                SqlExpr::Ident("a".into()),
                SqlExpr::cmp(
                    OpCmp::Eq,
                    SqlExpr::Ident("t.b".into()),
                    SqlExpr::Literal(SqlLiteral::Number("1".into())),
                ),
            ),
        )?;

        check(
            "SELECT * FROM t WHERE a OR b",
            SqlExpr::or(SqlExpr::Ident("a".into()), SqlExpr::Ident("b".into())),
        )?;

        check(
            "SELECT * FROM t WHERE a OR (b and c)",
            SqlExpr::or(
                SqlExpr::Ident("a".into()),
                SqlExpr::and(SqlExpr::Ident("b".into()), SqlExpr::Ident("c".into())),
            ),
        )?;

        Ok(())
    }

    #[test]
    fn compile_join() -> SqlResult<()> {
        fn check(sql: &str, expected: Vec<SqlJoin>) -> SqlResult<()> {
            let ast = parse_select(sql)?;
            assert_eq!(ast.from.joins, expected);
            Ok(())
        }

        for op in [OpCmp::Eq, OpCmp::NotEq, OpCmp::Gt, OpCmp::GtEq, OpCmp::Lt, OpCmp::LtEq] {
            check(
                &format!(
                    "SELECT * FROM t JOIN t2 ON t.a {} t2.b",
                    PrintOperator { op: op.into() }
                ),
                vec![SqlJoin::Inner {
                    rhs: "t2".into(),
                    on: SqlOn::Cmp {
                        op,
                        lhs: "t.a".into(),
                        rhs: "t2.b".into(),
                    },
                }],
            )?;
        }

        // We can join more than [MAX_JOINS] tables
        check(
            "SELECT * FROM t JOIN t2 ON t.a = t2.b INNER JOIN t3 ON t2.c = t3.d",
            vec![
                SqlJoin::Inner {
                    rhs: "t2".into(),
                    on: SqlOn::Cmp {
                        op: OpCmp::Eq,
                        lhs: "t.a".into(),
                        rhs: "t2.b".into(),
                    },
                },
                SqlJoin::Inner {
                    rhs: "t3".into(),
                    on: SqlOn::Cmp {
                        op: OpCmp::Eq,
                        lhs: "t2.c".into(),
                        rhs: "t3.d".into(),
                    },
                },
            ],
        )?;

        Ok(())
    }

    /// Verify we can do SUM, COUNT.
    #[test]
    fn compile_summarize() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlAgg) -> SqlResult<()> {
            let ast = parse_select(sql)?;
            assert_eq!(&*ast.project, [SqlProject::Summarize(expected)]);

            Ok(())
        }

        check(
            "SELECT COUNT(*) AS a FROM a",
            SqlAgg::Count {
                arg: SqlArg::Wildcard,
                distinct: false,
                alias: "a".into(),
            },
        )?;

        check(
            "SELECT COUNT(b) AS a FROM a",
            SqlAgg::Count {
                arg: SqlArg::Ident("b".into()),
                distinct: false,
                alias: "a".into(),
            },
        )?;

        check(
            "SELECT COUNT(a.b) AS a FROM a",
            SqlAgg::Count {
                arg: SqlArg::Ident("a.b".into()),
                distinct: false,
                alias: "a".into(),
            },
        )?;

        check(
            "SELECT COUNT(DISTINCT b) AS b FROM a",
            SqlAgg::Count {
                arg: SqlArg::Ident("b".into()),
                distinct: true,
                alias: "b".into(),
            },
        )?;

        check(
            "SELECT SUM(a) AS a FROM a",
            SqlAgg::Sum {
                ident: "a".into(),
                alias: "a".into(),
            },
        )?;
        check(
            "SELECT SUM(a.b) AS b FROM a",
            SqlAgg::Sum {
                ident: "a.b".into(),
                alias: "b".into(),
            },
        )?;

        // Verify we don't allow qualified wildcards
        let ast = parse_select("SELECT COUNT(a.*) AS b FROM a");
        matches_unsupported!(ast, SqlUnsupported::Argument(FunctionArgExpr::QualifiedWildcard { .. }));

        Ok(())
    }

    /// Verify we can do LIMIT
    #[test]
    fn compile_limit() -> SqlResult<()> {
        let ast = parse_select("SELECT * FROM a")?;
        assert_eq!(ast.limit, None);

        let ast = parse_select("SELECT * FROM a LIMIT 10")?;
        assert_eq!(ast.limit, Some(10));

        // Verify we don't allow negative numbers
        let ast = parse_select("SELECT * FROM a LIMIT -10");
        matches_unsupported!(ast, SqlUnsupported::LimitExpr(_));

        Ok(())
    }

    /// Verify we can do DISTINCT
    #[test]
    fn compile_distinct() -> SqlResult<()> {
        let ast = parse_select("SELECT * FROM a")?;
        assert!(!ast.distinct);
        let ast = parse_select("SELECT DISTINCT * FROM a")?;
        assert!(ast.distinct);

        Ok(())
    }

    /// Verify we can do ORDER BY
    #[test]
    fn compile_order_by() -> SqlResult<()> {
        fn check(sql: &str, expected: &[SqlOrder]) -> SqlResult<()> {
            let ast = parse_select(sql)?;
            assert_eq!(&*ast.order_by, expected);

            Ok(())
        }

        check("SELECT * FROM a", &[])?;

        check(
            "SELECT * FROM a ORDER BY a",
            &[SqlOrder {
                ident: "a".into(),
                asc: true,
            }],
        )?;

        check(
            "SELECT * FROM a ORDER BY a DESC",
            &[SqlOrder {
                ident: "a".into(),
                asc: false,
            }],
        )?;

        check(
            "SELECT * FROM a ORDER BY a, b DESC",
            &[
                SqlOrder {
                    ident: "a".into(),
                    asc: true,
                },
                SqlOrder {
                    ident: "b".into(),
                    asc: false,
                },
            ],
        )?;

        // Verify we don't support aliases
        let ast = parse_select("SELECT * FROM a ORDER BY a AS b");
        assert!(matches!(ast, Err(err) if err.to_string().contains("General ParsingError")));

        Ok(())
    }

    #[test]
    fn compile_insert() -> SqlResult<()> {
        let ast = parse_insert("INSERT INTO t (a, b) VALUES (1, 2.0, true, false, 'hello', 0x0000FCAB)")?;

        assert_eq!(
            ast,
            SqlInsert {
                table: "t".into(),
                idents: ["a".into(), "b".into()].into(),
                values: [[
                    SqlLiteral::Number("1".into()),
                    SqlLiteral::Number("2.0".into()),
                    SqlLiteral::Bool(true),
                    SqlLiteral::Bool(false),
                    SqlLiteral::String("hello".into()),
                    SqlLiteral::Hex("0000FCAB".into()),
                ]
                .into()]
                .into(),
            }
        );

        Ok(())
    }

    #[test]
    fn compile_update() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlUpdate) -> SqlResult<()> {
            let ast = parse_update(sql)?;

            assert_eq!(ast, expected);

            Ok(())
        }

        check(
            "UPDATE t SET a = 1",
            SqlUpdate {
                table: "t".into(),
                assignments: [SqlAssignment {
                    ident: "a".into(),
                    value: SqlLiteral::Number("1".into()),
                }]
                .into(),
                selection: None,
            },
        )?;

        check(
            "UPDATE t SET a = 1, b = 2 WHERE c = 3",
            SqlUpdate {
                table: "t".into(),
                assignments: [
                    SqlAssignment {
                        ident: "a".into(),
                        value: SqlLiteral::Number("1".into()),
                    },
                    SqlAssignment {
                        ident: "b".into(),
                        value: SqlLiteral::Number("2".into()),
                    },
                ]
                .into(),
                selection: Some(SqlSelection::new(SqlExpr::BinOp {
                    op: OpQuery::Cmp(OpCmp::Eq),
                    lhs: SqlExpr::Ident("c".into()).into(),
                    rhs: SqlExpr::Literal(SqlLiteral::Number("3".into())).into(),
                })),
            },
        )?;

        Ok(())
    }

    #[test]
    fn compile_delete() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlDelete) -> SqlResult<()> {
            let ast = parse_delete(sql)?;

            assert_eq!(ast, expected);

            Ok(())
        }

        check(
            "DELETE FROM t",
            SqlDelete {
                table: "t".into(),
                selection: None,
            },
        )?;

        check(
            "DELETE FROM t WHERE a = 1",
            SqlDelete {
                table: "t".into(),
                selection: Some(SqlSelection::new(SqlExpr::BinOp {
                    op: OpQuery::Cmp(OpCmp::Eq),
                    lhs: SqlExpr::Ident("a".into()).into(),
                    rhs: SqlExpr::Literal(SqlLiteral::Number("1".into())).into(),
                })),
            },
        )?;

        Ok(())
    }

    #[test]
    fn compile_set_var() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlSetVar) -> SqlResult<()> {
            let ast = parse_set_var(sql)?;

            assert_eq!(ast, expected);

            Ok(())
        }
        check(
            "SET a = 1",
            SqlSetVar {
                name: "a".into(),
                value: SqlLiteral::Number("1".into()),
            },
        )?;

        check(
            "SET a TO 1",
            SqlSetVar {
                name: "a".into(),
                value: SqlLiteral::Number("1".into()),
            },
        )?;
        Ok(())
    }

    #[test]
    fn compile_read_var() -> SqlResult<()> {
        let ast = parse_read_var("SHOW a")?;

        assert_eq!(ast, SqlReadVar { name: "a".into() });

        Ok(())
    }

    /// Verify we can do capture hex and binary literals
    #[test]
    fn test_hex_bin() -> SqlResult<()> {
        let ast = parse_select("SELECT * FROM a WHERE a = 0x0000FCAB AND a = x'93dda09' AND a = X'93dda09'")?;

        assert_eq!(
            ast.selection.unwrap(),
            SqlSelection::new(SqlExpr::and(
                SqlExpr::and(
                    SqlExpr::cmp(
                        OpCmp::Eq,
                        SqlExpr::Ident("a".into()),
                        SqlExpr::Literal(SqlLiteral::Hex("0000FCAB".into()))
                    ),
                    SqlExpr::cmp(
                        OpCmp::Eq,
                        SqlExpr::Ident("a".into()),
                        SqlExpr::Literal(SqlLiteral::Hex("93dda09".into()))
                    ),
                ),
                SqlExpr::cmp(
                    OpCmp::Eq,
                    SqlExpr::Ident("a".into()),
                    SqlExpr::Literal(SqlLiteral::Hex("93dda09".into()))
                )
            ))
        );

        Ok(())
    }
}
