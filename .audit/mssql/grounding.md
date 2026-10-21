Historical implementation evidence. The final capability contract is recorded in implementation.md and README.md.

# SQL Server support grounding

The user requests SQL Server connections and automatic SQL dialect selection for each connection while retaining PostgreSQL support.

## Existing behavior

- `model.rs` owns all IPC types. `ConnectionConfig` and `ConnectionDraft` have no engine discriminant. Saved connection JSON must remain loadable with hand-written deserialization fallbacks.
- `session.rs` exposes a PostgreSQL `Client`, cancellation token, backend PID, notices and socket death watch. One session belongs to each console and moves into its run task.
- `connection.rs` owns instance/database slots and PostgreSQL connection configuration and URL parsing.
- `catalog.rs` and `detail.rs` query PostgreSQL system tables directly through that client. Object identifiers are scoped by `DatabaseRef` throughout.
- `console.rs` prepares for metadata, then streams PostgreSQL simple-protocol text rows into Rust-owned `ResultSet`. Each planned statement currently has one result reference. `lib.rs` directly uses the client for metadata, transactions, counts and staged edits.
- `text.rs` builds table browse/count/edit SQL with PostgreSQL quoting, ILIKE and LIMIT/OFFSET. It also owns UTF-16 conversion, formatting and statement outlines.
- `sql/` owns PostgreSQL lexing, splitting, classification, relation extraction and assistant safety vetting.
- `assistant/tools.rs` is the only place model SQL executes. PostgreSQL uses fresh READ ONLY sessions and rolls back by dropping. Preserve this protection. SQL Server has no equivalent per-transaction read-only switch; do not silently weaken it.
- CodeMirror's PostgreSQL dialect is hardcoded in editor states, result filters, SQL cards and history highlights. Connection forms default to PostgreSQL parameters.

## Important constraints

All SQL interpretation and generation remain in the engine. Tauri stays thin, bindings are generated, rows stay in Rust, cells stay string or null, snapshots retain revision rules, and database identity scopes every catalog lookup.

T-SQL batches need `GO` support and bracket identifiers. Splitting every semicolon can break DECLARE variables and procedure bodies. Multiple result sets and output from DML require a deliberate contract.

SQL Server cannot set a session-only default schema in the way PostgreSQL sets search_path. Do not change a user's default schema with ALTER USER to emulate this.

## Baseline and environment

`cargo test --workspace`, `pnpm test` with 360 tests, and `pnpm typecheck` pass before edits. Postgres 16 runs on localhost:5433. SQL Server 2022 Developer is starting on localhost:1434 in an amd64 container under local Apple Silicon emulation. Login `sa`, development-only password `L1Data_dev_Str0ng`. Container name `l1-data-mssql`.

The maintained upstream `tiberius` crate is 0.13.0 per docs.rs checked this session. Check source APIs rather than assuming old 0.12 behavior. PostgreSQL returns native text; TDS returns typed values, so canonical text conversion belongs only in the SQL Server driver and requires an architecture reconciliation.

## Intended predicate

An existing PostgreSQL connection loads without migration loss. Users can create and reopen a SQL Server connection, discover databases/schema/tables, execute T-SQL with correct cancellation and transaction semantics, page/filter/count/edit data, and get the right editor/completion/highlight dialect per tab. PostgreSQL tests remain green. Any feature that cannot preserve its safety contract must return an explicit unsupported result and be described in the UI and docs.
