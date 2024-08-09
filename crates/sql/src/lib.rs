//! # SQL Support
//!
//! SpacetimeDB supports two subsets of `SQL:2016`:
//!
//! ## Query and DML
//!
//! General `SQL` queries and `DML` statements.
//!
//! See the [`ast::ad_hoc`] module for more information.
//!
//! ## Subscriptions
//!
//! Subscriptions are a way to subscribe to changes in the database.
//!
//! See the [`ast::subscription`] module for more information.
//!
//! ## Type System
//!
//! [SATS](spacetimedb_sats::algebraic_type) is the type system of SpacetimeDB `SQL`.
//!
//! However, the language has limited support for `algebraic data types`.
//!
//! It only supports primitively typed literal values, as it cannot express `product`, `sum`, or `array` types directly,
//! and in general does not support scalar operations on those types,
//! the one caveat being the [Identity](spacetimedb_lib::identity) and [Address](spacetimedb_lib::address) types.
//!
//! ## Terminals
//!
//! ### Bool (SATS [boolean](spacetimedb_sats::algebraic_type::AlgebraicType::Bool))
//!
//!  ```ebnf
//! TRUE
//!    = 'true'
//!   ;
//! FALSE
//!   = 'false'
//!  ;
//! ```
//!
//! ### Integer
//!
//! **Note:** The concrete `SATS` type is inferred from the context.
//!
//! ```ebnf
//! DIGIT
//!     = 0..9
//!     ;
//!
//! NUM
//!     = DIGIT { DIGIT }
//!     ;
//!
//! INTEGER
//!     = [ '+' | '-' ] NUM
//!     | [ '+' | '-' ] NUM 'E' [ '+' | '-' ] NUM
//!     ;
//! ```
//!
//! ### Float
//!
//! **Note:** The concrete `SATS` type is inferred from the context.
//!
//! ```ebnf
//! FLOAT
//!     = [ '+' | '-' ] [ NUM ] '.' NUM
//!     | [ '+' | '-' ] [ NUM ] '.' NUM 'E' [ '+' | '-' ] NUM
//!     ;
//! ```
//!
//! ### String (SATS [string](spacetimedb_sats::algebraic_type::AlgebraicType::String))
//!
//! Where `CHAR` is a `utf-8` encoded unicode character.
//!
//! ```ebnf
//! STRING
//!     = "'" { "''" | CHAR } "'"
//!     ;
//! ```
//!
//! ### Hex
//!
//! Hexidecimal literals represent either [Identity](spacetimedb_lib::identity)
//! or [Address](spacetimedb_lib::address) types.
//!
//! **Note:** The concrete `SATS` type is inferred from the context.
//!
//! ```ebnf
//! HEX
//!     = 'X' "'" { HEXIT } "'"
//!     | '0' 'x' { HEXIT }
//!     ;
//!
//! HEXIT
//!     = DIGIT | a..f | A..F
//!     ;
//! ```
//!
pub mod ast;
pub mod errors;
#[macro_use]
pub mod parser;
