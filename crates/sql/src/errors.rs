use crate::ast::{PrintOperator, SqlSource};
use derive_more::Display;
use error_stack::Report;
use sqlparser::ast::{BinaryOperator, Expr, Function, FunctionArg, FunctionArgExpr, Ident, SetExpr, Statement, Value};
use thiserror::Error;

#[derive(Debug, Display)]
pub enum Suggestion {
    #[display(fmt = "Change it to an INNER JOIN")]
    ChangeJoinToInner,
    #[display(fmt = "Change to a valid comparison operator: `=, <, >, <=, >=`")]
    ChangeOpToCmp,
    #[display(fmt = "Change to several queries")]
    ChangeOrToManyQueries,
    #[display(fmt = "Only `column` names & `COUNT`, `SUM` allowed")]
    ProjectionAdHoc,
}

#[derive(Error, Debug)]
pub enum SqlUnsupported {
    #[error("Unsupported Limit expression `{0}`")]
    LimitExpr(Expr),
    #[error("Unsupported Binary Operator `{0}`")]
    BinaryOp(BinaryOperator),
    #[error("Unsupported JOIN kind `{0}`")]
    JoinKind(String),
    #[error("JOIN constrain `{0}` is not valid, can be only on the form Table.Field [Cmp] Table.Field")]
    JoinConstraint(String),
    #[error("Join operator `{0}`")]
    JoinOp(PrintOperator),
    #[error("Unsupported value `{0}`")]
    Value(Value),
    #[error("Multiple tables in `FROM`")]
    MultipleTables,
    #[error("Unsupported `{name}` with `{it}` feature")]
    FeatureNamed { name: &'static str, it: &'static str },
    #[error("Unsupported `{0}` feature")]
    Feature(String),
    #[error("Unsupported INSERT `VALUES` expression: `{0}`")]
    InsertWithValues(SetExpr),
    #[error("Unsupported ORDER BY expression: `{0}`")]
    OrderByExpr(Expr),
    #[error("Unsupported named arguments: `{0}`")]
    NamedArguments(FunctionArg),
    #[error("Unsupported function argument: `{0}`")]
    Argument(FunctionArgExpr),
    #[error("Unsupported multiple arguments in aggregate function: `{0}`")]
    MultipleArgumentsAggregate(Function),
    #[error("Unsupported function argument in aggregate function: Only `column` names allowed")]
    ArgumentAggregate,
    #[error("Unsupported function: `{0}`")]
    Function(Function),
    #[error("Unsupported identifier: `{0}`")]
    MultipleIdentifier(String),
    #[error("Unsupported DISTINCT ON")]
    DistinctOn,
    #[error("Unsupported expression: `{0}`")]
    Expression(Expr),
    #[error("Unsupported table alias")]
    TableAlias,
    #[error("Unsupported column alias")]
    ColumnAlias,
    #[error("Unsupported expression in SELECT: `{0}`")]
    AdHocProjection(Expr),
    #[error("Unsupported expression in JOIN ON: Only `column` names allowed")]
    AdHocJoinOnExp,
    #[error("Unsupported `{name}` without alias")]
    AdHocFunctionAlias { name: String },
    #[error("Unsupported expression in SELECT: `{0}`")]
    SubscriptionProjection(Expr),
    #[error("Subscriptions not allow multiple statements")]
    SubscriptionMultipleStatements,
    #[error("Subscriptions not allow the `<>` operator")]
    SubscriptionNotEq,
    #[error("Subscriptions not allow `{0}`")]
    SubscriptionStatement(Statement),
    #[error("Subscriptions only allow `*` or `Table.*` in SELECT")]
    SubscriptionSelect,
    #[error("Unsupported expression in JOIN ON: Only `column` names allowed")]
    SubscriptionJoinOnExpr,
    #[error("Unsupported `SET` values different to a single one: `{0:?}`")]
    SetValues(Vec<Expr>),
    #[error("Unsupported `SHOW` values different to a single one: `{0:?}`")]
    ShowVariable(Vec<Ident>),
}

#[derive(Error, Debug)]
pub enum SqlError {
    #[error("{0} ParsingError")]
    ParsingError(SqlSource),
    #[error("ParseError: {0}")]
    ParseError(#[from] sqlparser::parser::ParserError),
    #[error("{0}")]
    Unsupported(#[from] SqlUnsupported),
    #[error("Missing FROM clause")]
    MissingFrom,
    #[error("Max joins exceeded: `{max_joins}` on subscriptions, but the query has `{given}` joins")]
    MaxJoinsSubscription { given: usize, max_joins: usize },
    #[error("Insert without `VALUES`")]
    InsertNoValues,
}

pub type SqlResult<T> = error_stack::Result<T, SqlError>;

impl From<SqlUnsupported> for Report<SqlError> {
    fn from(value: SqlUnsupported) -> Self {
        Report::from(SqlError::Unsupported(value))
    }
}
