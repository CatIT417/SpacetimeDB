//! ## Subscriptions AST
//!
//! The subscription language is strictly a query language.
//!
//! There is no context for manually updating this view.
//!
//! > **NOTE:** Because subscriptions are evaluated in realtime,
//! > performance is critical, and as a result,
//! > additional restrictions are applied over [`super::ad_hoc`] queries.
//! > These restrictions are highlighted below.
//!
//! - Only `SELECT *` or `table.*` projections
//! - No `ORDER BY`, `LIMIT`, or aggregates
//! - No `OR` conditions
//! - No `!=` predicates
//! - Only 2-way joins
//!
//! ### Statements
//!
//! - [`SQL`](SqlAst)
//!
//! ### `SELECT`
//!
//! - [`SELECT`](SqlProject)
//! - [`FROM`](SqlFrom)
//! - [`JOIN`](SqlJoin)
//! - [`ON`](SqlOn)
//! - [`WHERE`](SqlSelection)
//!
//! ### Subscription Grammar
//!
//! **WARNING:** This grammar does not guarantee the semantic validity of its queries, only their syntactic correctness.
//!
//! ```ebnf
//! query
//!     = SELECT projection FROM relation [ WHERE predicate ]
//!     ;
//!
//! projection
//!     = STAR
//!     | ident '.' STAR
//!     ;
//!
//! relation
//!     = table [ [AS] ident ] [ [INNER] JOIN table [ [AS] ident ] ON predicate_on ]
//!     ;
//!
//! columnExpr
//!     = column
//!     | qualfied
//!     ;
//!
//! predicate
//!     =  expr
//!     | predicate AND predicate
//!     ;
//!
//! predicate_on
//!     =  columnExpr '=' columnExpr
//!     | predicate_on AND predicate_on
//!     ;
//!
//! expr
//!     = literal
//!     | ident
//!     | qualified
//!     | expr op expr
//!     ;
//!
//! qualified
//!     = ident '.' ident
//!     ;
//!
//! op
//!     = '='
//!     | '<'
//!     | '>'
//!     | '<' '='
//!     | '>' '='
//!     ;
//!
//! literal
//!     = INTEGER
//!     | STRING
//!     | HEX
//!     | TRUE
//!     | FALSE
//!     ;
//! ```
//!
use crate::ast::*;
use crate::errors::{SqlError, SqlUnsupported};

/// The maximum number of `JOIN` clauses allowed in a `SQL` subscription query.
///
/// This is a limitation to avoid complex queries that could be hard to optimize.
pub const MAX_JOINS: usize = 2;

/// The comparison operators supported in `SQL` subscription queries.
///
/// ```ebnf
/// op
///     = '='
///     | '<'
///     | '>'
///     | '<' '='
///     | '>' '='
///     ;
/// ```
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum SqlOpCmp {
    Eq,
    Lt,
    LtEq,
    Gt,
    GtEq,
}

impl TryInto<SqlOpCmp> for OpCmp {
    type Error = SqlError;

    fn try_into(self) -> Result<SqlOpCmp, Self::Error> {
        match self {
            OpCmp::Eq => Ok(SqlOpCmp::Eq),
            OpCmp::Lt => Ok(SqlOpCmp::Lt),
            OpCmp::LtEq => Ok(SqlOpCmp::LtEq),
            OpCmp::Gt => Ok(SqlOpCmp::Gt),
            OpCmp::GtEq => Ok(SqlOpCmp::GtEq),
            _ => Err(SqlUnsupported::SubscriptionNotEq.into()),
        }
    }
}

/// The `SELECT` clause for `SQL` subscription queries.
///
/// ```ebnf
/// SELECT ( '*' | table '.' '*' )
/// ```
#[derive(Debug, Eq, PartialEq)]
pub enum SqlProject {
    /// An qualified `table.*`
    QualifiedWildcard { table: Identifier },
    /// An unqualified `SELECT *`
    Wildcard,
}

/// The `ON` clause of `JOIN` for `SQL` subscription queries.
///
/// ```ebnf
/// predicate_on
///    = columnExpr '=' columnExpr
///   | predicate_on AND predicate_on
///  ;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlOn {
    And { lhs: Box<SqlOn>, rhs: Box<SqlOn> },
    Eq { lhs: SqlIdentifier, rhs: SqlIdentifier },
}

/// The `[INNER] JOIN` clause for `SQL` subscription queries.
///
/// ```ebnf
/// [INNER] JOIN table [ [AS] alias ] ON predicate_on
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlJoin {
    Inner { rhs: SqlTable, on: SqlOn },
}

/// The `FROM` & `JOIN` clauses for `SQL` subscription queries.
///
/// ```ebnf
/// FROM table [ [AS] alias ] [ [INNER] JOIN ]
/// ```
#[derive(Debug)]
pub struct SqlFrom {
    pub root: SqlTable,
    pub joins: Vec<SqlJoin>,
}

impl SqlFrom {
    pub fn new(root: SqlTable, joins: Vec<SqlJoin>) -> Self {
        Self { root, joins }
    }
}

impl From<(SqlTable, Vec<SqlJoin>)> for SqlFrom {
    fn from(value: (SqlTable, Vec<SqlJoin>)) -> Self {
        SqlFrom::new(value.0, value.1)
    }
}

/// A valid expression for `SQL` subscription queries.
///
/// ```ebnf
/// expr
///     = literal
///     | ident
///     | column
///     | expr op expr
///     ;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlExpr {
    Literal(SqlLiteral),
    Ident(SqlIdentifier),
    BinOp {
        op: OpQuery,
        lhs: Box<SqlExpr>,
        rhs: Box<SqlExpr>,
    },
}

impl SqlExpr {
    pub fn and(lhs: SqlExpr, rhs: SqlExpr) -> Self {
        SqlExpr::BinOp {
            op: OpQuery::Logic(OpLogic::And),
            lhs: lhs.into(),
            rhs: rhs.into(),
        }
    }
    pub fn cmp(op: OpCmp, lhs: SqlExpr, rhs: SqlExpr) -> Self {
        SqlExpr::BinOp {
            op: OpQuery::Cmp(op),
            lhs: lhs.into(),
            rhs: rhs.into(),
        }
    }
}

/// The list of expressions determining what data to extract.
///
/// ```ebnf
/// predicate
///     = expr
///     | predicate AND predicate
///     ;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlSelection {
    pub predicate: SqlExpr,
}

impl SqlSelection {
    pub fn new(predicate: SqlExpr) -> Self {
        Self { predicate }
    }
}

/// Defines the portions of the `SQL` standard that we support for 'subscription' queries.
///
/// See the [module-level documentation](subscription) for more information.
#[derive(Debug)]
pub struct SqlAst {
    pub project: SqlProject,
    pub from: SqlFrom,
    pub selection: Option<SqlSelection>,
}
