use errors::{SqlParseError, SqlUnsupported};
use sqlparser::ast::{BinaryOperator, Expr, Ident, ObjectName, SelectItem, Value, WildcardAdditionalOptions};

use crate::ast::{BinOp, Project, ProjectElem, SqlExpr, SqlIdent, SqlLiteral};

pub mod errors;
pub mod sql;
pub mod sub;

pub type SqlParseResult<T> = core::result::Result<T, SqlParseError>;

/// Parse the items of a SELECT clause
pub(crate) fn parse_projection(mut items: Vec<SelectItem>) -> SqlParseResult<Project> {
    if items.len() == 1 {
        return parse_project(items.swap_remove(0));
    }
    Ok(Project::Exprs(
        items
            .into_iter()
            .map(|item| parse_project_elem(item))
            .collect::<SqlParseResult<_>>()?,
    ))
}

/// Parse a SELECT clause with only a single item
pub(crate) fn parse_project(item: SelectItem) -> SqlParseResult<Project> {
    match item {
        SelectItem::Wildcard(WildcardAdditionalOptions {
            opt_exclude: None,
            opt_except: None,
            opt_rename: None,
            opt_replace: None,
        }) => Ok(Project::Star(None)),
        SelectItem::QualifiedWildcard(
            table_name,
            WildcardAdditionalOptions {
                opt_exclude: None,
                opt_except: None,
                opt_rename: None,
                opt_replace: None,
            },
        ) => Ok(Project::Star(Some(parse_ident(table_name)?))),
        SelectItem::UnnamedExpr(_) | SelectItem::ExprWithAlias { .. } => {
            Ok(Project::Exprs(vec![parse_project_elem(item)?]))
        }
        item => {
            return Err(SqlUnsupported::Projection(item).into());
        }
    }
}

/// Parse an item in a SELECT clause
pub(crate) fn parse_project_elem(item: SelectItem) -> SqlParseResult<ProjectElem> {
    match item {
        SelectItem::Wildcard(_) => Err(SqlUnsupported::MixedWildcardProject.into()),
        SelectItem::QualifiedWildcard(..) => Err(SqlUnsupported::MixedWildcardProject.into()),
        SelectItem::UnnamedExpr(expr) => Ok(ProjectElem(parse_expr(expr)?, None)),
        SelectItem::ExprWithAlias { expr, alias } => Ok(ProjectElem(parse_expr(expr)?, Some(alias.into()))),
    }
}

/// Parse a scalar expression
pub(crate) fn parse_expr(expr: Expr) -> SqlParseResult<SqlExpr> {
    match expr {
        Expr::Value(v) => Ok(SqlExpr::Lit(parse_literal(v)?)),
        Expr::Identifier(ident) => Ok(SqlExpr::Var(ident.into())),
        Expr::CompoundIdentifier(mut idents) if idents.len() == 2 => {
            let table = idents.swap_remove(0).into();
            let field = idents.swap_remove(0).into();
            Ok(SqlExpr::Field(table, field))
        }
        Expr::BinaryOp { left, op, right } => {
            let l = parse_expr(*left)?;
            let r = parse_expr(*right)?;
            Ok(SqlExpr::Bin(Box::new(l), Box::new(r), parse_binop(op)?))
        }
        _ => Err(SqlUnsupported::Expr(expr).into()),
    }
}

/// Parse an optional scalar expression
pub(crate) fn parse_expr_opt(opt: Option<Expr>) -> SqlParseResult<Option<SqlExpr>> {
    opt.map(|expr| parse_expr(expr))
        .transpose()
        .map_err(SqlParseError::from)
}

/// Parse a scalar binary operator
pub(crate) fn parse_binop(op: BinaryOperator) -> SqlParseResult<BinOp> {
    match op {
        BinaryOperator::Eq => Ok(BinOp::Eq),
        BinaryOperator::NotEq => Ok(BinOp::Ne),
        BinaryOperator::Lt => Ok(BinOp::Lt),
        BinaryOperator::LtEq => Ok(BinOp::Lte),
        BinaryOperator::Gt => Ok(BinOp::Gt),
        BinaryOperator::GtEq => Ok(BinOp::Gte),
        BinaryOperator::And => Ok(BinOp::And),
        BinaryOperator::Or => Ok(BinOp::Or),
        _ => Err(SqlUnsupported::BinOp(op).into()),
    }
}

/// Parse a literal expression
pub(crate) fn parse_literal(value: Value) -> SqlParseResult<SqlLiteral> {
    match value {
        Value::Boolean(v) => Ok(SqlLiteral::Bool(v)),
        Value::Number(v, _) => Ok(SqlLiteral::Num(v)),
        Value::SingleQuotedString(s) => Ok(SqlLiteral::Str(s)),
        Value::HexStringLiteral(s) => Ok(SqlLiteral::Hex(s)),
        _ => Err(SqlUnsupported::Literal(value).into()),
    }
}

/// Parse an identifier
pub(crate) fn parse_ident(ObjectName(parts): ObjectName) -> SqlParseResult<SqlIdent> {
    parse_parts(parts)
}

/// Parse an identifier
pub(crate) fn parse_parts(mut parts: Vec<Ident>) -> SqlParseResult<SqlIdent> {
    if parts.len() == 1 {
        return Ok(parts.swap_remove(0).into());
    }
    Err(SqlUnsupported::MultiPartName(ObjectName(parts)).into())
}
