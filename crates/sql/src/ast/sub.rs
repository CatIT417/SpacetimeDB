use super::{Project, SqlExpr};

/// The AST for the SQL subscription language.
pub enum SqlAst {
    /// SELECT
    Select(SqlSelect),
    /// UNION [ALL]
    Union(Box<SqlAst>, Box<SqlAst>, bool),
    /// EXCEPT [ALL]
    Minus(Box<SqlAst>, Box<SqlAst>, bool),
}

/// A SELECT statement in the SQL subscription language
pub struct SqlSelect {
    /// SELECT ...
    pub project: Project,
    /// FROM ...
    pub from: SqlFrom,
    /// WHERE ...
    pub filter: Option<SqlExpr>,
}

/// A FROM clause in the SQL subscription language
pub type SqlFrom = super::SqlFrom<SqlAst>;

/// A relation expression in the SQL subscription language
pub type RelExpr = super::RelExpr<SqlAst>;
