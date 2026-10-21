# Independent review A, round 7

Pinned source: `8b78498cf55bbc8710194daaac48e472621d75aa`, `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`. Review scope: `main...HEAD`, with focused tracing of native session semantics, catalog invalidation/visibility, capability boundaries, persisted kind, and frontend pending-edit identity. The final scope overrides in `.audit/mssql/implementation.md` are authoritative; historical Manual/staged SQL Server contracts are not requirements of this review.

**Verdict: request changes. Counts: 0 critical, 1 warning, 0 nit.** This is my independent result only. No nested agents, source edits, commits, PRs, real provider calls, seeded-row writes, or service changes were made.

## A7-W1 — SQL Server SELECT INTO creates a table without invalidating the catalog

**Severity: warning.** Location: `crates/engine/src/sql/tsql.rs:344` and `:370` (especially the effect scan at `:379`). Reachable caller: `crates/engine/src/console.rs:364`.

`SELECT ... INTO dbo.new_table` is supported native T-SQL that creates a persistent table. The new SQL Server invalidation implementation recognizes opaque procedure execution and CREATE/ALTER/DROP/TRUNCATE/GRANT/DENY/REVOKE, but never recognizes this table-creation form. `SELECT` is a known statement word, so it also does not enter the conservative opaque-call path. The result is `Invalidates::Nothing`.

The reachable chain is `Engine::console_run(Script)` → `console::run_task` → `tsql::invalidates(sql)` → `ConsoleInner::invalidate`, whose Nothing case returns without recording an obligation. Native execution then succeeds. `finish_refreshes` has no obligation to settle, leaving `catalog_get` on its previous Ready snapshot. Consequently the new table is absent from the catalog used by the sidebar, completion, and assistant schema tools until the user manually refreshes. This concerns the new SQL Server native-execution/catalog path, not a request to expand the deliberately removed write capabilities or fix unrelated PostgreSQL baseline behavior.

**Disposable real Engine reproduction:** `/tmp/l1data-mssql-review-a-r7-select-into.rs`, compiled against the current engine and dependencies selected from `/tmp/l1data-mssql-review-a-r7-artifacts.jsonl`. Output: `/tmp/l1data-mssql-review-a-r7-select-into.log`.

1. Open an isolated Engine with a temporary data directory, memory credentials, and disabled provider path. Connect to SQL Server `shop`, create two console sessions, and load its catalog.
2. Run `SELECT 7 AS id INTO dbo.[l1_opaque_d8c2155571a44476b2011c3ad2eb0e71_table]` through the public Engine API.
3. The run finishes `RunOutcome::Ok`. The independent observer console sees the table in `sys.tables`.
4. After 2.2 seconds, another public catalog read still has version 1 and no relation with that name.
5. Explicit `refresh_catalog` produces version 2 with the relation present.

Observed output:

```text
BEFORE_VERSION=1
OBSERVER_EXISTS=true
CACHED_VERSION=1 CACHED_EXISTS=false
MANUAL_REFRESH_VERSION=2 EXISTS=true
```

**Bounded correction:** recognize SELECT INTO as a catalog-changing operation in the T-SQL effect calculation and feed the existing pending-refresh policy. Keep the decision in the SQL layer; do not add a special console branch, observation SQL, or SQL rewriting. `is_write` already recognizes INTO, so keep the effect and history interpretations consistent. Add a public Engine regression that loads a catalog, executes SELECT INTO, and observes automatic version/relation convergence; retain an ordinary SELECT control that creates no obligation. A transaction-ending case should use the existing native-end policy.

**Cleanup and limits:** the probe dropped its unique table and independently verified absence; its cleanup also verified the unused unique procedure/database names absent. It closed its consoles/connection and shut down the temporary Engine. No seeded rows were touched. The failure is the stale catalog snapshot, not a claim that arbitrary SQL SELECT or a direct TablePage request cannot read the newly created table. No GUI reproduction was performed by this reviewer.

## Independent checks and quality assessment

- Current-source `cargo build -p engine --lib --message-format=json`: passed. The disposable probe used the reported artifacts, not guessed rlibs.
- `cargo test -p engine --features mssql-tests --test mssql_catalog_visibility --test mssql_opaque_refresh --test mssql_session_state`: **15 passed**. Log: `/tmp/l1data-mssql-review-a-r7-regressions.log`. This independently exercises the R6 hidden-alias corrections, explicit/implicit/dynamic procedure refreshes, native rowcount/error/options preservation, protocol transaction state, and session retirement.
- `cargo test -p engine --features 'pg-tests mssql-tests' --test pg_key_identity --test mssql_transactions`: **7 passed**. Log: `/tmp/l1data-mssql-review-a-r7-capabilities.log`. This covers named PostgreSQL keys across primary-key replacement and the SQL Server Manual/staged refusal boundaries, including prior native work and cancellation.
- Focused Vitest runs for editor actions, ResultGrid, table edits, TopBar, and database identity: **62 passed in 5 files**. Log: `/tmp/l1data-mssql-review-a-r7-frontend.log`. These are frontend tests, not UI acceptance evidence.
- `vendor/tiberius/verify-upstream.py` against the pinned cached 0.13.0 archive: **passed**; 96 included files unchanged and seven modified files match the 210-line `UPSTREAM.patch`. Reviewed that delta and the adapter boundary, not the unchanged upstream source as new application code.
- Source tracing found no additional actionable security or structural blocker in the inspected paths. Driver ownership, canonical conversion, the shared query-event path, and removal of unsupported write machinery fit the stated design. No application Rust/TypeScript file crosses from below 1,000 lines to above it in this diff; the large engine/model files already exceeded it on main. A suspected partitioned-table edit regression was excluded after checking main, which already restricted grid editing to kind Table.
- Final source check remained at the pinned SHA with a clean tracked working tree.

The full workspace suite, full frontend suite/build, TLS matrix, native desktop/browser acceptance, screenshots, and remote CI were not rerun by this reviewer. **Root UI/native and CI gates remain owner-owned and open according to the parent brief.** Passing focused checks do not approve the outstanding warning or replace those gates.
