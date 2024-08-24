use sqlparser::{
    ast::{
        GroupByExpr, Query, Select, SetExpr, SetOperator, SetQuantifier, Statement, TableAlias, TableFactor,
        TableWithJoins,
    },
    dialect::PostgreSqlDialect,
    parser::Parser,
};

use crate::ast::{
    sub::{RelExpr, SqlAst, SqlFrom, SqlSelect},
    SqlIdent,
};

use super::{
    errors::{SqlRequired, SqlUnsupported, SubscriptionUnsupported},
    parse_expr_opt, parse_ident, parse_projection, SqlParseResult,
};

/// Parse a SQL string
pub fn parse_subscription(sql: &str) -> SqlParseResult<SqlAst> {
    let mut stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql)?;
    if stmts.len() > 1 {
        return Err(SqlUnsupported::MultiStatement.into());
    }
    parse_statement(stmts.swap_remove(0))
}

/// Parse a SQL query
fn parse_statement(stmt: Statement) -> SqlParseResult<SqlAst> {
    match stmt {
        Statement::Query(query) => parse_query(*query),
        _ => Err(SubscriptionUnsupported::Dml.into()),
    }
}

/// Parse a relation expression
pub(crate) fn parse_query(query: Query) -> SqlParseResult<SqlAst> {
    match query {
        Query {
            with: None,
            body,
            order_by,
            limit: None,
            offset: None,
            fetch: None,
            locks,
        } if order_by.is_empty() && locks.is_empty() => parse_set_op(*body),
        _ => Err(SubscriptionUnsupported::feature(query).into()),
    }
}

/// Parse a set operation
pub(crate) fn parse_set_op(expr: SetExpr) -> SqlParseResult<SqlAst> {
    match expr {
        SetExpr::Query(query) => parse_query(*query),
        SetExpr::Select(select) => Ok(SqlAst::Select(parse_select(*select)?)),
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier: SetQuantifier::All,
            left,
            right,
        } => Ok(SqlAst::Union(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            true,
        )),
        SetExpr::SetOperation {
            op: SetOperator::Union,
            set_quantifier: SetQuantifier::None,
            left,
            right,
        } => Ok(SqlAst::Union(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            false,
        )),
        SetExpr::SetOperation {
            op: SetOperator::Except,
            set_quantifier: SetQuantifier::All,
            left,
            right,
        } => Ok(SqlAst::Minus(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            true,
        )),
        SetExpr::SetOperation {
            op: SetOperator::Except,
            set_quantifier: SetQuantifier::None,
            left,
            right,
        } => Ok(SqlAst::Minus(
            Box::new(parse_set_op(*left)?),
            Box::new(parse_set_op(*right)?),
            false,
        )),
        _ => Err(SqlUnsupported::SetOp(expr).into()),
    }
}

// Parse a SELECT statement
pub(crate) fn parse_select(select: Select) -> SqlParseResult<SqlSelect> {
    match select {
        Select {
            distinct: None,
            top: None,
            projection,
            into: None,
            from,
            lateral_views,
            selection,
            group_by: GroupByExpr::Expressions(exprs),
            cluster_by,
            distribute_by,
            sort_by,
            having: None,
            named_window,
            qualify: None,
        } if lateral_views.is_empty()
            && exprs.is_empty()
            && cluster_by.is_empty()
            && distribute_by.is_empty()
            && sort_by.is_empty()
            && named_window.is_empty() =>
        {
            Ok(SqlSelect {
                from: parse_from(from)?,
                filter: parse_expr_opt(selection)?,
                project: parse_projection(projection)?,
            })
        }
        _ => Err(SubscriptionUnsupported::Select(select).into()),
    }
}

/// Parse a FROM clause
pub(crate) fn parse_from(mut tables: Vec<TableWithJoins>) -> SqlParseResult<SqlFrom> {
    if tables.is_empty() {
        return Err(SqlRequired::From.into());
    }
    if tables.len() > 1 {
        return Err(SqlUnsupported::ImplicitJoins.into());
    }
    let TableWithJoins { relation, joins } = tables.swap_remove(0);
    let (expr, alias) = parse_table_factor(relation)?;
    if joins.is_empty() {
        return Ok(SqlFrom::Expr(expr, alias));
    }
    let Some(alias) = alias else {
        return Err(SqlRequired::JoinAlias.into());
    };
    let mut exprs = vec![(expr, alias)];
    for join in joins {
        let (expr, Some(alias)) = parse_table_factor(join.relation)? else {
            return Err(SqlRequired::JoinAlias.into());
        };
        exprs.push((expr, alias));
    }
    Ok(SqlFrom::Join(exprs))
}

/// Parse a relation expression in a FROM clause
pub(crate) fn parse_table_factor(expr: TableFactor) -> SqlParseResult<(RelExpr, Option<SqlIdent>)> {
    match expr {
        // Relvar no alias
        TableFactor::Table {
            name,
            alias: None,
            args: None,
            with_hints,
            version: None,
            partitions,
        } if with_hints.is_empty() && partitions.is_empty() => Ok((RelExpr::Var(parse_ident(name)?), None)),
        // Relvar with alias
        TableFactor::Table {
            name,
            alias: Some(TableAlias { name: alias, columns }),
            args: None,
            with_hints,
            version: None,
            partitions,
        } if with_hints.is_empty() && partitions.is_empty() && columns.is_empty() => {
            Ok((RelExpr::Var(parse_ident(name)?), Some(alias.into())))
        }
        // RelExpr no alias
        TableFactor::Derived {
            lateral: false,
            subquery,
            alias: None,
        } => Ok((RelExpr::Ast(Box::new(parse_query(*subquery)?)), None)),
        // RelExpr with alias
        TableFactor::Derived {
            lateral: false,
            subquery,
            alias: Some(TableAlias { name, columns }),
        } if columns.is_empty() => Ok((RelExpr::Ast(Box::new(parse_query(*subquery)?)), Some(name.into()))),
        _ => Err(SqlUnsupported::From(expr).into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::sub::parse_subscription;

    #[test]
    fn unsupported() {
        assert!(parse_subscription("SELECT 1").is_err());
        assert!(parse_subscription("SELECT a FROM b.c").is_err());
    }

    #[test]
    fn supported() {
        assert!(parse_subscription("SELECT a FROM b").is_ok());
        assert!(parse_subscription("SELECT a FROM b WHERE c = 1").is_ok());
        assert!(parse_subscription("SELECT a FROM (SELECT a FROM b)").is_ok());
    }
}
