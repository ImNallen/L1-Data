# Independent SQL Server review C, R7

## Verdict

No blocking correctness or security finding in the reviewed paths. One non-blocking code-quality nit.

| Severity | Count |
|---|---:|
| critical | 0 |
| warning | 0 |
| nit | 1 |

This is my independent verdict only; it does not approve earlier rounds or satisfy the root-owned UI, native acceptance, or remote CI gates.

## Finding

### nit — SQL Server assistant instructions are patched by exact prose replacement

**Location and path:** `crates/engine/src/assistant/turn.rs:675-684`; `Turn::start` calls `system_prompt(database_kind)` before constructing the provider codec. The SQL Server branch mutates the PostgreSQL prompt with three chained `str::replace` calls.

**Evidence:** The current source strings match, so I observed no current prompt-contract failure. The replacement API silently leaves the original text in place when a phrase changes or moves. In particular, a future edit to `assistant/system_prompt.md` can silently restore the claim that `run_query` runs read-only and the advice to use `LIMIT`, while SQL Server execution remains refused. Existing prompt coverage reads and checks the PostgreSQL resource; I found no test for the rendered SQL Server prompt.

**Bounded suggestion:** Give SQL Server an explicit prompt resource or compose shared instructions with explicit dialect-specific instructions, then assert that the rendered SQL Server prompt names T-SQL, says `run_query` is disabled, and does not recommend PostgreSQL-only `LIMIT`.

## Review scope and checks

Read the supplied `AGENTS.md`, README SQL Server limits, architecture load-bearing and reconciliation sections, `.audit/mssql/implementation.md` final-scope paragraph, and `.audit/mssql/review.md`. Traced the final alias-type fallback, opaque T-SQL invalidation, SQL Server transaction refusal/execution boundaries, session protocol observation path, and assistant policy/execute paths. I did not review unchanged upstream Tiberius files as new application code.

Fresh disposable Engine probes passed:

- `cargo test -p engine --features mssql-tests --test mssql_catalog_visibility -- --nocapture` — 2 passed. Both restricted-login cases confirmed readable columns, exact decimal/money browsing and, in the executable case, routine visibility. Each test reported zero remaining objects, types, users, logins, or sessions after cleanup.
- `cargo test -p engine --features mssql-tests --test mssql_opaque_refresh -- --nocapture` — 6 passed. Procedure, dynamic execution, database-list, transaction settlement, rename, and table-page cases converged; cleanup reported the disposable tables, procedure, and database absent.
- `cargo test -p engine --features mssql-tests --test mssql_session_state go_and_consecutive_runs_match_raw_guarded_writes -- --nocapture` — 1 passed, comparing Engine and raw-driver guarded writes across GO and consecutive runs. The test uses session-scoped temporary tables and its harness shuts down the Engine.

I did not run the full workspace/database suites, TLS matrix, desktop UI/native acceptance, or remote CI; those gates remain with the root agent. No source files were changed. The only artifact written was this report.
