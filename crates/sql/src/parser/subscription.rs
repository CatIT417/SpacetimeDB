//! This module provides the implementation of the [`ParserSubscription`] struct and its associated methods.
//! It is used to parse `SQL` queries related to subscriptions and compile them into an abstract syntax tree
//! defined in  [crate::ast::subscription].
//!

use crate::ast::subscription::*;
use crate::ast::SqlSource;
use crate::errors::{SqlError, SqlResult, SqlUnsupported};
use crate::parser::*;
use crate::unsupported;
use error_stack::Report;

use sqlparser::ast::{Expr, OrderByExpr, Query, Select, SetExpr, Statement};

/// Parser for the [crate::ast::subscription] `SQL` queries.
pub struct ParserSubscription<'a> {
    sql: &'a str,
}

impl<'a> ParserSubscription<'a> {
    pub fn new(sql: &'a str) -> Self {
        Self { sql }
    }

    pub fn parse(&self) -> SqlResult<SqlAst> {
        let ast = parse_sql(SqlSource::Subscription, self.sql)?;

        if ast.len() != 1 {
            return Err(Report::new(SqlUnsupported::SubscriptionMultipleStatements.into()))
                .attach_printable_lazy(|| self.sql.to_string());
        }

        let ast = self
            .compile_statement(ast[0].clone())
            .attach_printable_lazy(|| self.sql.to_string())?;

        Ok(ast)
    }
}

impl SqlParser for ParserSubscription<'_> {
    type Ast = SqlAst;
    type Expr = SqlExpr;
    type Selection = SqlSelection;
    type Projection = SqlProject;
    type InnerJoin = SqlJoin;
    type JoinOn = SqlOn;
    type From = SqlFrom;

    fn compile_other_statement(&self, statement: Statement) -> SqlResult<Self::Ast> {
        Err(SqlUnsupported::SubscriptionStatement(statement).into())
    }

    fn compile_query(&self, query: Query) -> SqlResult<Self::Ast> {
        unsupported!(
            "SELECT",
            query.order_by,
            query.fetch,
            query.limit,
            query.offset,
            query.locks,
            query.with
        );

        match *query.body {
            SetExpr::Select(select) => {
                unsupported!(
                    "SELECT",
                    select.distinct,
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
            x => Err(SqlUnsupported::Feature(format!("{x:?}")).into()),
        }
    }

    fn compile_select(
        &self,
        select: Select,
        _limit: Option<Expr>,
        _order_by: Vec<OrderByExpr>,
    ) -> SqlResult<Self::Ast> {
        let from: SqlFrom = self.compile_from(&select.from)?.into();
        if from.joins.len() > MAX_JOINS {
            return Err(SqlError::MaxJoinsSubscription {
                given: from.joins.len(),
                max_joins: MAX_JOINS,
            }
            .into());
        }

        let project = match &select.projection[..] {
            [item] => self.compile_projection(item.clone())?,
            _ => {
                return Err(SqlUnsupported::SubscriptionSelect.into());
            }
        };

        let selection = self.compile_where(select.selection)?;

        Ok(SqlAst {
            from,
            project,
            selection,
        })
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

    fn compile_projection_expr(&self, expr: Expr, _alias: Option<Identifier>) -> SqlResult<Self::Projection> {
        Err(SqlUnsupported::SubscriptionProjection(expr).into())
    }

    fn make_selection(&self, predicate: Expr) -> SqlResult<Self::Selection> {
        Ok(SqlSelection::new(self.compile_expr(predicate)?))
    }

    fn extract_column_join(&self, expr: Self::Expr) -> SqlResult<SqlIdentifier> {
        match expr {
            SqlExpr::Ident(column) => Ok(column),
            _ => Err(SqlUnsupported::SubscriptionJoinOnExpr.into()),
        }
    }

    fn compile_join_on(&self, expr: &Expr) -> SqlResult<Self::JoinOn> {
        match expr {
            Expr::BinaryOp { left, op, right } => {
                let op = self.compile_op(op.clone())?;
                match op {
                    OpQuery::Cmp(OpCmp::Eq) => {}
                    OpQuery::Logic(OpLogic::And) => {}
                    op => return Err(SqlUnsupported::JoinOp(PrintOperator { op }).into()),
                }
                let lhs = self.compile_expr(*left.clone())?;
                let rhs = self.compile_expr(*right.clone())?;

                match op {
                    OpQuery::Cmp(_) => {
                        let lhs_col = self.extract_column_join(lhs)?;
                        let rhs_col = self.extract_column_join(rhs)?;
                        Ok(SqlOn::Eq {
                            lhs: lhs_col,
                            rhs: rhs_col,
                        })
                    }
                    OpQuery::Logic(_) => {
                        let lhs_on = self.compile_join_on(left)?;
                        let rhs_on = self.compile_join_on(right)?;
                        Ok(SqlOn::And {
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
            BinaryOperator::And => OpLogic::And.into(),
            BinaryOperator::Or => {
                return Err(
                    Report::from(SqlUnsupported::BinaryOp(op)).attach_printable(Suggestion::ChangeOrToManyQueries)
                );
            }
            op => {
                return Err(Report::from(SqlUnsupported::BinaryOp(op)).attach_printable(Suggestion::ChangeOpToCmp));
            }
        })
    }

    fn compile_expr(&self, of: Expr) -> SqlResult<Self::Expr> {
        Ok(match of {
            Expr::Identifier(name) => SqlExpr::Ident(name.into()),
            Expr::CompoundIdentifier(ident) => SqlExpr::Ident(ident.into()),
            Expr::Value(value) => SqlExpr::Literal(compile_value(value)?),
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
            x => return Err(SqlUnsupported::Expression(x).into()),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ast::subscription::*;
    use crate::ast::{PrintOperator, SqlLiteral, SqlTable};
    use crate::errors::{SqlError, SqlResult, SqlUnsupported};
    use crate::matches_unsupported;
    use crate::parser::subscription::ParserSubscription;
    use spacetimedb_lib::operator::OpCmp;
    use sqlparser::ast::{BinaryOperator, Expr};

    fn parse(sql: &str) -> SqlResult<SqlAst> {
        ParserSubscription::new(sql).parse()
    }

    /// Verify we can only compile `Project::Wildcard` | `Project::QualifiedWildcard`
    #[test]
    fn compile_project() -> SqlResult<()> {
        let ast = parse("SELECT * FROM a")?;
        assert_eq!(ast.from.root, "a".into());
        assert_eq!(ast.project, SqlProject::Wildcard);

        let ast = parse("SELECT a.* FROM a")?;
        assert_eq!(ast.from.root, "a".into());
        assert_eq!(ast.project, SqlProject::QualifiedWildcard { table: "a".into() });

        let ast = parse("SELECT a FROM b");
        matches_unsupported!(ast, SqlUnsupported::SubscriptionProjection(Expr::Identifier(_)));

        Ok(())
    }

    /// Verify we can compile aliases
    #[test]
    fn compile_alias() -> SqlResult<()> {
        let ast = parse("SELECT * FROM t AS a")?;
        assert_eq!(ast.from.root, SqlTable::new("t".into(), Some("a".into())));

        let ast = parse("SELECT * FROM t JOIN b AS c ON t.a = c.b")?;
        assert_eq!(ast.from.root, "t".into());
        assert_eq!(
            ast.from.joins[0],
            SqlJoin::Inner {
                rhs: SqlTable::new("b".into(), Some("c".into())),
                on: SqlOn::Eq {
                    lhs: "t.a".into(),
                    rhs: "c.b".into(),
                },
            }
        );

        Ok(())
    }

    #[test]
    fn compile_where() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlExpr) -> SqlResult<()> {
            let ast = parse(sql)?;
            assert_eq!(ast.selection, Some(SqlSelection::new(expected)));
            Ok(())
        }
        for op in [OpCmp::Eq, OpCmp::Gt, OpCmp::GtEq, OpCmp::Lt, OpCmp::LtEq] {
            check(
                &format!("SELECT * FROM t WHERE a {} 1", PrintOperator { op: op.into() }),
                SqlExpr::cmp(
                    op,
                    SqlExpr::Ident("a".into()),
                    SqlExpr::Literal(SqlLiteral::Number("1".into())),
                ),
            )?;
        }

        // Verify we don't compile `<>`
        let ast = parse("SELECT * FROM t WHERE a <> 1");
        matches_unsupported!(ast, SqlUnsupported::BinaryOp(BinaryOperator::NotEq));

        Ok(())
    }

    #[test]
    fn compile_ands_ors() -> SqlResult<()> {
        fn check(sql: &str, expected: SqlExpr) -> SqlResult<()> {
            let ast = parse(sql)?;
            assert_eq!(ast.selection, Some(SqlSelection::new(expected)));
            Ok(())
        }

        check("SELECT * FROM t WHERE a ", SqlExpr::Ident("a".into()))?;

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

        // Verify we don't compile `OR`
        let ast = parse("SELECT * FROM t WHERE a OR b");
        matches_unsupported!(ast, SqlUnsupported::BinaryOp(BinaryOperator::Or));

        Ok(())
    }

    #[test]
    fn compile_join() -> SqlResult<()> {
        fn check(sql: &str, expected: Vec<SqlJoin>) -> SqlResult<()> {
            let ast = parse(sql)?;
            assert_eq!(ast.from.joins, expected);
            Ok(())
        }

        check(
            "SELECT * FROM t JOIN t2 ON t.a = t2.b",
            vec![SqlJoin::Inner {
                rhs: "t2".into(),
                on: SqlOn::Eq {
                    lhs: "t.a".into(),
                    rhs: "t2.b".into(),
                },
            }],
        )?;

        check(
            "SELECT * FROM t JOIN t2 ON t.a = t2.b INNER JOIN t3 ON t2.c = t3.d",
            vec![
                SqlJoin::Inner {
                    rhs: "t2".into(),
                    on: SqlOn::Eq {
                        lhs: "t.a".into(),
                        rhs: "t2.b".into(),
                    },
                },
                SqlJoin::Inner {
                    rhs: "t3".into(),
                    on: SqlOn::Eq {
                        lhs: "t2.c".into(),
                        rhs: "t3.d".into(),
                    },
                },
            ],
        )?;

        check(
            "SELECT * FROM t JOIN t2 ON t.a = t2.b AND t.c = t2.d",
            vec![SqlJoin::Inner {
                rhs: "t2".into(),
                on: SqlOn::And {
                    lhs: Box::new(SqlOn::Eq {
                        lhs: "t.a".into(),
                        rhs: "t2.b".into(),
                    }),
                    rhs: Box::new(SqlOn::Eq {
                        lhs: "t.c".into(),
                        rhs: "t2.d".into(),
                    }),
                },
            }],
        )?;

        // Verify we don't exceed the max joins
        let ast =
            parse("SELECT * FROM t JOIN t2 ON t.a = t2.b JOIN t3 ON t2.c = t3.d JOIN t4 ON t3.e = t4.f").unwrap_err();
        assert!(matches!(ast.current_context(), &SqlError::MaxJoinsSubscription { .. }));

        // Verify we can't compile a join with `OR`
        let ast = parse("SELECT * FROM t JOIN t2 ON t.a = t2.b OR t.c = t2.d");
        matches_unsupported!(ast, SqlUnsupported::BinaryOp(BinaryOperator::Or));

        Ok(())
    }
}
