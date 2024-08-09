//! ## Query and DML
//!
//! A subset of `SQL:2016` standard that supports general queries and `DML` statements.
//!
//! ### Statements
//!
//! - [`SQL`](SqlAst)
//!
//! ### `SELECT`
//!
//! - [`SELECT`](SqlProject)
//! - [`DISTINCT`](SqlSelect)
//! - [`FROM`](SqlFrom)
//! - [`WHERE`](SqlSelection)
//! - [`JOIN`](SqlJoin)
//! - [`ON`](SqlOn)
//! - [`ORDER BY`](SqlOrder)
//! - [`LIMIT`](SqlSelect)
//! - [`Aggregates`](SqlAgg)
//!
//! ### `DML`
//!
//! - [`INSERT`](SqlInsert)
//! - [`DELETE`](SqlDelete)
//! - [`UPDATE`](SqlUpdate)
//!
//! ### Others
//!
//! - [`SET`](SqlSetVar)
//! - [`SHOW`](SqlReadVar)
//!
//! Notable unsupported features include
//!
//! - Literals in `SELECT` clause
//! - DDL
//! - Subqueries
//! - Arithmetic expressions
//! - Group by aggregation
//!
//! ### Query Grammar
//!
//! > **Warning**: This grammar does not guarantee the semantic validity of its queries.
//! > That is the responsibility of the type system.
//! > For example, while the following query is syntactically valid
//! >
//! > ```sql
//! >  SELECT * FROM TABLE WHERE a = 'str'
//! > ```
//! > it cannot be evaluated when `a` is any type other than `string`.
//!
//! ```ebnf
//! statement
//!     = select
//!     | insert
//!     | delete
//!     | update
//!     | set
//!     | show
//!     ;
//!
//! insert
//!     = INSERT INTO table [ '(' column { ',' column } ')' ] VALUES '(' literal { ',' literal } ')'
//!     ;
//!
//! delete
//!     = DELETE FROM table [ WHERE predicate ]
//!     ;
//!
//! update
//!     = UPDATE table SET [ '(' assignment { ',' assignment } ')' ] [ WHERE predicate ]
//!     ;
//!
//! assignment
//!     = column '=' expr
//!     ;
//!
//! set
//!     = SET var ( TO | '=' ) literal
//!     ;
//!
//! show
//!     = SHOW var
//!     ;
//!
//! var
//!     = ident
//!     ;
//!
//! select
//!     = SELECT [ DISTINCT ] projection FROM relation [ [ WHERE predicate ] [ ORDER BY order ] [ LIMIT limit ] ]
//!     ;
//!
//! projection
//!     = listExpr
//!     | projExpr { ',' projExpr }
//!     | aggrExpr { ',' aggrExpr }
//!     ;
//!
//! listExpr
//!     = STAR
//!     | ident '.' STAR
//!     ;
//!
//! projExpr
//!     = columnExpr [ [ AS ] ident ]
//!     ;
//!
//! columnExpr
//!     = column
//!     | qualfied
//!     ;
//!
//! aggrExpr
//!     = COUNT '(' STAR ')' [AS] ident
//!     | COUNT '(' DISTINCT columnExpr ')' [AS] ident
//!     | SUM   '(' columnExpr ')' [AS] ident
//!     ;
//!
//! relation
//!     = table [ [AS] ident ] { [INNER] JOIN table [ [AS] ident ] ON predicate }
//!     ;
//!
//! predicate
//!     = expr
//!     | predicate AND predicate
//!     | predicate OR  predicate
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
//!     | '!' '='
//!     | '<' '>'
//!     ;
//!
//! order
//!     = columnExpr [ ASC | DESC ] { ',' columnExpr [ ASC | DESC ] }
//!     ;
//!
//! limit
//!     = INTEGER
//!     ;
//!
//! table
//!     = ident
//!     ;
//!
//! column
//!     = ident
//!     ;
//!
//! literal
//!     = INTEGER
//!     | FLOAT
//!     | STRING
//!     | HEX
//!     | TRUE
//!     | FALSE
//!     ;
//! ```
use crate::ast::*;

/// Arguments for `SQL` projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlArg {
    Ident(SqlIdentifier),
    Wildcard,
}

/// The `SUM` and `COUNT` expressions for `SQL` projection.
///
/// ```ebnf
/// aggrExpr
///    = COUNT '(' STAR ')' [AS] ident
///    | COUNT '(' DISTINCT columnExpr ')' [AS] ident
///    | SUM   '(' columnExpr ')' [AS] ident
/// ;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlAgg {
    Count {
        arg: SqlArg,
        distinct: bool,
        alias: Identifier,
    },
    Sum {
        ident: SqlIdentifier,
        alias: Identifier,
    },
}

/// The `SELECT` clause for `SQL` queries.
///
/// ```ebnf
/// listExpr
///    = STAR
///   | ident '.' STAR
///  ;
///
/// projection
///     = listExpr
///     | projExpr { ',' projExpr }
///     | aggrExpr { ',' aggrExpr }
///     ;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlProject {
    /// An qualified `columnExpr`
    Ident {
        column: SqlIdentifier,
        alias: Option<Identifier>,
    },
    /// A `SUM(columnExpr)` or `COUNT(* | columnExpr)` expression
    Summarize(SqlAgg),
    /// An qualified `table.*`
    QualifiedWildcard { table: Identifier },
    /// An unqualified `SELECT *`
    Wildcard,
}

/// The list of expressions for determining what data to extract.
///
/// ```ebnf
/// predicate
///     = expr
///     | predicate AND predicate
///     | predicate OR predicate
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

/// The `ON` clause of `JOIN` for `SQL` queries.
///
/// ```ebnf
/// predicate
///     = expr
///    | predicate AND predicate
///    | predicate OR predicate
///    ;
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum SqlOn {
    Logic {
        op: OpLogic,
        lhs: Box<SqlOn>,
        rhs: Box<SqlOn>,
    },
    Cmp {
        op: OpCmp,
        lhs: SqlIdentifier,
        rhs: SqlIdentifier,
    },
}

/// The `[INNER] JOIN` clause for `SQL` queries.
///
/// ```ebnf
/// [INNER] JOIN table [ [AS] alias ] ON
/// ```
#[derive(Debug, PartialEq, Eq)]
pub enum SqlJoin {
    Inner { rhs: SqlTable, on: SqlOn },
}

///  The `FROM` & `JOIN` clauses for `SQL` queries.
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

/// A valid expression for `SQL` queries.
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
    pub fn or(lhs: SqlExpr, rhs: SqlExpr) -> Self {
        SqlExpr::BinOp {
            op: OpQuery::Logic(OpLogic::Or),
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

/// The `ORDER BY` clause for `SQL` queries.
///
/// ```ebnf
/// ORDER BY columnExpr [ ASC | DESC ]
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlOrder {
    pub ident: SqlIdentifier,
    pub asc: bool,
}

/// Defines the portions of the `SQL` standard that we support for 'general' queries.
///
/// See the [module-level documentation](ad_hoc) for more information.
#[derive(Debug)]
pub struct SqlSelect {
    pub from: SqlFrom,
    pub selection: Option<SqlSelection>,
    pub project: Box<[SqlProject]>,
    pub order_by: Box<[SqlOrder]>,
    pub limit: Option<u64>,
    pub distinct: bool,
}

/// The `INSERT INTO` clause for `SQL` queries.
///
/// ```ebnf
/// INSERT INTO table [ '(' column { ',' column } ')' ] VALUES '(' literal { ',' literal } ')'
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SqlInsert {
    pub table: SqlTable,
    // We allow disjunct columns and values, need to be validated by the type system later.
    pub idents: Box<[SqlIdentifier]>,
    pub values: Box<[Box<[SqlLiteral]>]>,
}

/// The `SET` assigment clause for the `UPDATE` statements.
///
/// ```ebnf
/// assignment
///     = column '=' expr
///     ;
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SqlAssignment {
    pub ident: SqlIdentifier,
    pub value: SqlLiteral,
}

/// The `UPDATE` clause for `SQL` queries.
///
/// ```ebnf
/// UPDATE table SET [ '(' assignment { ',' assignment } ')' ] [ WHERE predicate ]
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SqlUpdate {
    pub table: SqlTable,
    pub assignments: Box<[SqlAssignment]>,
    pub selection: Option<SqlSelection>,
}

/// The `DELETE` clause for `SQL` queries.
///
/// ```ebnf
/// DELETE FROM table [ WHERE predicate ]
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SqlDelete {
    pub table: SqlTable,
    pub selection: Option<SqlSelection>,
}

/// The `SET` clause for the `SQL` queries.
///
/// ```ebnf
/// SET var ( TO | '=' ) literal
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SqlSetVar {
    pub name: Identifier,
    pub value: SqlLiteral,
}

/// The `SHOW` clause for the `SQL` queries.
///
/// ```ebnf
/// SHOW var
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct SqlReadVar {
    pub name: Identifier,
}

/// Defines the portions of the `SQL` standard that we support for 'general' queries.
///
/// See the [module-level documentation](ad_hoc) for more information.
#[derive(Debug)]
pub enum SqlAst {
    Select(SqlSelect),
    Insert(SqlInsert),
    Update(SqlUpdate),
    Delete(SqlDelete),
    SetVar(SqlSetVar),
    ReadVar(SqlReadVar),
}
