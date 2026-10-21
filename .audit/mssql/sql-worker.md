Historical implementation evidence. The final capability contract is recorded in implementation.md and README.md.

# Pure T-SQL module contract

Own only `crates/engine/src/sql/tsql.rs`, its inline behavior tests, and the minimal `pub(crate) mod tsql;` registration in sql/mod.rs. Do not modify model.rs, lib.rs, connection/session/console, frontend, dependencies or docs. The backend owner integrates calls to these functions. Use the existing `StmtSpan`, `ByteTarget`, `StmtKind`, `Invalidates`, `TableQuery` and `relations::NameRef` types.

Implement these crate-private functions.

```rust
split_batches(text: &str) -> Result<Vec<StmtSpan>, String>
validate_batch(text: &str) -> Result<(), String>
pick(text: &str, target: ByteTarget) -> Result<Vec<StmtSpan>, String>
classify(text: &str) -> StmtKind
has_transaction_control(text: &str) -> bool
is_write(text: &str) -> bool
relation_names(text: &str) -> Vec<super::relations::NameRef>
quote_ident(name: &str) -> String
quote_literal(value: &str) -> String
validate_expression(text: &str, order_by: bool) -> Result<(), String>
parse_order_by(text: &str) -> Option<Vec<(String, bool)>>
format_order_by(keys: &[(String, bool)]) -> String
format(text: &str) -> Result<String, String>
browse_sql(schema: &str, table: &str, projection: &str, pk: &[String], query: &TableQuery, limit: u32, offset: u64) -> String
count_sql(schema: &str, table: &str, query: &TableQuery) -> String
```

All returned spans are byte offsets into the exact original text. GO is a standalone case-insensitive line outside quotes/comments/brackets and may have trailing comments. Reject GO counts, sqlcmd directives and malformed lexical state before any execution. Preserve semicolons, variable scope, BEGIN/END blocks and procedure definitions inside batches. A cursor selects its batch, while a selection executes only exact selected text and rebases resulting spans. A blank/comment/GO line between batches selects nothing. Empty/comment-only batches are omitted. Use separate T-SQL lexical rules, never reinterpret PG array brackets globally.

`validate_batch` rejects an actual unquoted USE keyword before execution and performs the lexical/directive checks needed for an individual batch. Call it during planning so no earlier batch executes before a later selected batch's validation fails. Strings and bracket identifiers containing USE are valid. Exact selections validate their selected text, not unrelated unselected document text. This helper was added by coordinator acceptance after the backend identified a missing API for the already accepted explicit-USE restriction.

The lexer recognizes N strings, doubled single/double quotes, escaped closing brackets, nested block comments, line comments, @ variables, # temp tables and Unicode. BEGIN alone is a procedural block, not transaction begin. Detect explicit BEGIN/COMMIT/ROLLBACK/SAVE TRANSACTION safely for manual mode. Classification is advisory; the real driver probes transaction state.

Expression validation is a consuming conservative subset for raw WHERE/ORDER fragments. One GO batch is not a safety check. Reject separators, comment escapes, DML/DDL/EXEC/USE/DBCC/WAITFOR and other statement syntax, including a second statement without a semicolon. Preserve strings and quoted identifiers containing those words. Unsupported expressions must be refused, never accepted optimistically. The order parser accepts only a list of simple column identifiers and optional ASC/DESC; otherwise returns None and the advanced text is preserved. Do not case-fold SQL Server identifiers.

Bracket-quote identifiers with doubled closing brackets. Unicode string literals use N and doubled quotes. Generated LIKE patterns escape the escape character, %, _, and [. Use explicit ESCAPE and let case sensitivity follow database collation. COUNT_BIG is the count operation. Pagination uses ORDER BY plus OFFSET/FETCH, appends PK tie breakers and falls back to ORDER BY (SELECT NULL) without a key or explicit order. Projection is trusted engine-generated SQL, supplied by the backend to handle exact money casts. Filter/order inputs are validated by callers before execution.

Formatting may support a bounded safe subset. It must preserve the T-SQL token sequence, comments, strings and GO scope. Return an unsupported error for constructs it cannot preserve. Never call the PG formatter on T-SQL. Do not build a comprehensive SQL parser for this task.

Keep meaningful tests for GO in every quote/comment form, DECLARE/SELECT as one batch, two batches, malformed/directive/repetition rejection, UTF-8 spans, exact selections, transaction-vs-block classification, Unicode/bracket quoting, wildcard escaping, pagination and raw fragment injection without semicolons. This module is the rerunnable syntax gate for SQL Server integration.
