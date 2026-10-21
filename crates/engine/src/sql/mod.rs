mod classify;
mod format;
mod history;
mod keywords;
mod lex;
mod relations;
mod split;
pub(crate) mod tsql;

pub use classify::{DmlVerb, Invalidates, StmtKind, classify};
pub use format::format_statement;
pub use relations::{Vet, relation_names, vet};
pub use split::{ByteTarget, StmtSpan, pick, split_statements};

pub(crate) use history::{history_is_write, history_text};
#[cfg(all(test, feature = "pg-tests"))]
pub(crate) use keywords::{NEVER_A_FUNCTION, OTHER_KEYWORDS, RESERVED, TYPE_OR_FUNCTION_NAME};
#[cfg(all(test, feature = "pg-tests"))]
pub(crate) use relations::{VETTED, VETTED_OPERATORS, VETTED_SIZES};

pub(crate) mod order;
pub(crate) mod tsql;
