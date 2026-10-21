# Independent review A, round 5

Pinned source: `98301df8f4d2c742931875f6e9e297f464989c9d`, reviewed against `main...HEAD`.

**Verdict: request changes. Counts: 1 critical, 0 warnings, 0 nits.**

This is one independent review. No agents were spawned, no panel or provider was invoked, and no production source was changed. The accepted first-release exclusions of SQL Server app-managed Manual mode and staged grid writes are respected. The finding is in the retained native Auto-commit script capability.

## Critical — Transaction-state observation changes native `@@ROWCOUNT` and can execute a delete whose guard should be false

**Location:** `crates/engine/src/session/mssql.rs:273`, called unconditionally after each native batch at lines 206–208.

**Reachable call chain:** `Engine::console_run` (`lib.rs:1033–1062`) splits a script into GO batches; `console::run_task` (`console.rs:316`) executes those batches sequentially through `run_outputs` → `Session::query` → `MsSession::query`. After the user batch, `MsSession::query` calls `probe`, which sends `SELECT @@TRANCOUNT, XACT_STATE(), DB_NAME()` on that same session. That SELECT replaces `@@ROWCOUNT` with 1 before the next user batch executes. Both batches are subsequently reported successful.

**Reproduction:** I compiled a disposable public-Engine probe against artifacts identified by a fresh Cargo JSON build, and compared it with direct Tiberius calls to the same SQL Server 2022 fixture. Each connection used only its own local temporary table. Initialize two rows `(1,10),(2,20)`, then execute:

```sql
UPDATE #review_table SET n=99 WHERE id=99
GO
IF @@ROWCOUNT > 0 DELETE FROM #review_table WHERE id=1;
SELECT COUNT(*) FROM #review_table WHERE id=1;
```

The UPDATE affects zero rows, so the native guard is false. The raw-driver control, sending the two batches separately without application SQL between them, leaves id=1 present and returns `1`. Engine deletes id=1 and returns `0`, with `RunOutcome::Ok`, both statements `Finished`, and the session still `Open/Idle`.

Additional controls reproduced the same root cause:

- An Engine single batch `UPDATE ... WHERE id=3; SELECT @@ROWCOUNT` returns `0`.
- Inserting GO between those statements makes Engine return `1`.
- `IF @@ROWCOUNT=0 INSERT ...` adds the missing third row through the raw driver, but Engine skips it and returns a count of `2` instead of `3`.
- Native BEGIN followed by the Engine Rollback button works in this probe: the transaction is observed Active/depth 1, rollback returns `Ok`, and the original value `20` remains. The finding does not depend on unsupported Manual mode.

This is a silent change to native write decisions, including demonstrated unintended deletion. The documented lack of an affected-row count in the application's output does not cover changing `@@ROWCOUNT` inside the user's SQL.

**Evidence:**

- `/tmp/l1data-mssql-review-a-r5-state-probe.rs`
- `/tmp/l1data-mssql-review-a-r5-state-probe.log`
- `/tmp/l1data-review-a-r5-artifacts.json`

The log contains `RAW_GUARDED_DELETE_SURVIVING_ROW=Some(1)` and `ENGINE_GO_GUARDED_DELETE` with rows `[[["0"]]]`, plus the complete successful Engine snapshots.

**Bounded correction:** Fix state observation at the SQL Server adapter boundary so it cannot alter session state consumed by subsequent native batches. Investigate obtaining state from the protocol rather than executing an ordinary SELECT on the user's session; any replacement SQL-based observation must prove preservation before adoption. Keep user SQL in its native execution scope. This review does not claim a ready replacement: disposable checks of dynamic EXEC, `sp_executesql`, Tiberius's query/RPC path, and a local temporary procedure all still left `@@ROWCOUNT=1` on this fixture. Merely wrapping the probe is not a demonstrated fix. Those checks and cleanup are recorded in `/tmp/l1data-mssql-review-a-r5-rpc-probe.{rs,log}`.

**Required regression:** Public-Engine tests should compare native batch execution against an uninstrumented driver for zero-row and multirow updates, conditional insert/delete, and both GO-separated batches and consecutive Run requests. Assert saved rows and successful/failed outcomes, not only reported transaction depth. Recheck cancellation, explicit transaction buttons, and the database-change guard after changing observation.

**Cleanup and limits:** Probe tables and the experimental procedure were local temporary objects in probe-owned sessions, explicitly dropped; Engine's cleanup query returned NULL for the dropped table's `OBJECT_ID`. Raw clients and Engines were closed, and their UUID-named temporary SQLite directories were removed. No seeded rows, application state, shared services, or provider accounts were changed. The destructive branch was proved only on a disposable local temporary table; no persistent user table was touched. The raw-driver controls independently establish the server semantics; no external documentation claim is needed for the finding.

## Checks and bounded coverage

Read AGENTS.md, README capability limits, the architecture load-bearing decisions and relevant reconciliation, `.audit/mssql/implementation.md` final scope paragraph, and `.audit/mssql/review.md`. Reviewed the full diff inventory and traced the retained SQL Server execution/state/conversion paths, final engine capability guards, relevant PostgreSQL driver cutover, named pending-key submission, SQL construction/validation, history kind persistence, and assistant refusal boundaries.

Independently ran:

- `cargo build -p engine --lib --features 'pg-tests mssql-tests' --message-format=json`: passed. Exact returned library artifacts were used for the temporary probes.
- `cargo test -p engine --features 'pg-tests mssql-tests' --test mssql_transactions --test pg_key_identity`: passed, 6 SQL Server tests and 1 PostgreSQL test. Log: `/tmp/l1data-mssql-review-a-r5-targeted-tests.log`. These tests use disposable objects and include cleanup checks.
- Public-Engine probes: reproduced the critical finding and verified SQL Server Manual configuration refusal, staged-edit refusal, and native transaction rollback, with cleanup as described above.
- Source/diff integrity: HEAD remained pinned. The only tracked worktree change observed at the end was parent-owned `docs/images/l1-data.png`; I did not modify it.

The code-quality pass found no additional actionable structural blocker in the paths inspected. Native client types remain behind private drivers, and the removed SQL Server Manual/write implementations were not retained as alternate execution paths. The changed source-file line counts show no file newly crossing the 1,000-line threshold; the larger files were already above it on main. These observations do not cancel the correctness finding.

I did not rerun the full workspace/frontend suites, TLS matrix, native/browser UI, or remote CI, and did not call any real provider. Parent-provided passes were context, not independent verification by this reviewer. **Root UI, screenshots, and CI gates remain owner-owned.** This report is not an approval of those gates or of uninspected paths.
