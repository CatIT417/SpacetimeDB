pub mod ad_hoc;
pub mod subscription;

use std::fmt;

use crate::errors::SqlUnsupported;
use derive_more::Display;
use spacetimedb_lib::operator::{OpCmp, OpLogic, OpQuery};
use sqlparser::ast::{Ident, ObjectName};

#[derive(Debug, Clone, PartialEq, Eq, Display)]
#[repr(transparent)]
pub struct Identifier {
    pub name: String,
}

impl Identifier {
    pub fn new(name: String) -> Self {
        Self { name }
    }
}

#[cfg(test)]
impl From<&str> for Identifier {
    fn from(value: &str) -> Self {
        Identifier::new(value.to_string())
    }
}

impl From<Ident> for Identifier {
    fn from(value: Ident) -> Self {
        Identifier::new(value.value)
    }
}

/// Source of the `SQL` query input.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Display)]
pub enum SqlSource {
    Subscription,
    General,
}

/// A `columnExpr` name, with optional `table` and `alias`.
///
/// ```ebnf
/// qualified
///     = ident '.' ident
///     ;
///
/// column
///    = ident
///    ;
///
/// columnExpr
///     = column
///     | qualfied
///     ;
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlIdentifier {
    pub name: Identifier,
    pub qualified: Option<Identifier>,
}

impl SqlIdentifier {
    pub fn new(column: Identifier) -> Self {
        Self {
            name: column,
            qualified: None,
        }
    }

    pub fn table(mut self, qualified: Identifier) -> Self {
        self.qualified = Some(qualified);
        self
    }
}

#[cfg(test)]
impl From<&str> for SqlIdentifier {
    fn from(value: &str) -> Self {
        let mut iter = value.split('.');
        match (iter.next(), iter.next()) {
            (Some(table), Some(column)) => SqlIdentifier {
                name: column.into(),
                qualified: Some(table.into()),
            },
            (Some(column), None) => SqlIdentifier {
                name: column.into(),
                qualified: None,
            },
            _ => panic!("{}", &format!("Invalid column: `{value}`")),
        }
    }
}

impl From<Ident> for SqlIdentifier {
    fn from(value: Ident) -> Self {
        SqlIdentifier {
            name: value.into(),
            qualified: None,
        }
    }
}

impl From<Vec<Ident>> for SqlIdentifier {
    fn from(value: Vec<Ident>) -> Self {
        match &value[..] {
            [table, column] => SqlIdentifier {
                name: column.clone().into(),
                qualified: Some(table.clone().into()),
            },
            [column] => SqlIdentifier {
                name: column.clone().into(),
                qualified: None,
            },
            _ => panic!("{}", &format!("Invalid column: `{value:?}`")),
        }
    }
}

/// Helper to override the `Display` trait for `OpQuery` enum,
/// to convert them to their `SQL` representation.
#[derive(Debug)]
pub struct PrintOperator {
    pub(crate) op: OpQuery,
}

impl fmt::Display for PrintOperator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.op {
            OpQuery::Cmp(op) => match op {
                OpCmp::Eq => {
                    write!(f, "=")
                }
                OpCmp::NotEq => {
                    write!(f, "<>")
                }
                x => {
                    write!(f, "{}", x)
                }
            },
            OpQuery::Logic(op) => match op {
                OpLogic::And => {
                    write!(f, "AND")
                }
                OpLogic::Or => {
                    write!(f, "OR")
                }
            },
        }
    }
}

/// Raw `SQL` literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SqlLiteral {
    /// Boolean value
    Bool(bool),
    /// Hexadecimal value
    /// ```sql
    /// 0x0000FCAB
    /// x'93dda0'
    /// ```
    Hex(String),
    /// Unparsed number (int, float, bigint, etc.)
    Number(String),
    /// String value
    String(String),
    /// Identifier (table, column, etc.)
    ///
    /// ```sql
    ///  "column_name"
    /// ```
    Identifier(Identifier),
}

/// `Table` name, with optional `alias`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SqlTable {
    pub name: Identifier,
    pub alias: Option<Identifier>,
}

impl SqlTable {
    pub fn new(name: Identifier, alias: Option<Identifier>) -> Self {
        Self { name, alias }
    }
}

#[cfg(test)]
impl From<&str> for SqlTable {
    fn from(value: &str) -> Self {
        SqlTable {
            name: value.into(),
            alias: None,
        }
    }
}

impl TryFrom<ObjectName> for Identifier {
    type Error = SqlUnsupported;

    fn try_from(value: ObjectName) -> Result<Self, Self::Error> {
        match value.0.as_slice() {
            [name] => Ok(name.clone().into()),
            _ => Err(SqlUnsupported::MultipleIdentifier(value.to_string())),
        }
    }
}

impl TryFrom<ObjectName> for SqlTable {
    type Error = SqlUnsupported;

    fn try_from(value: ObjectName) -> Result<Self, Self::Error> {
        let identifier = Identifier::try_from(value)?;
        Ok(SqlTable::new(identifier, None))
    }
}
