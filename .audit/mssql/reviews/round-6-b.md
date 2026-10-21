# Independent review B, round 6

Pinned source: `7ebc2f07c2bbfec284317caebab4942078c36997`, compared with `main`.

**Verdict: Request changes. Counts: 0 critical, 2 warnings, 0 nits.** This is one independent reviewer's result. No panel verdict or approval is claimed. The final first-release contract, including refusal of SQL Server Manual mode and staged writes, is the review target.

## W1 — warning: A hidden routine parameter type fails the whole SQL Server catalog

**Location:** `crates/engine/src/session/mssql/catalog.rs:130–137`, specifically `TYPE_NAME(p.user_type_id)` and `required(&r, 2)?`.

**Reachable call chain:** Public `Engine::catalog` → `Shared::catalog` / `Shared::load_catalog` (`crates/engine/src/lib.rs:264–306`) → `Session::catalog` → `MsSession::catalog` → required parameter-type text. The error marks the database catalog Failed. On its first load there is no earlier catalog to serve, so the sidebar, completion and catalog-dependent browsing/schema tools lose the entire catalog.

**Reproduction and observed state:** A disposable real Engine probe created an alias type based on `nvarchar(30)`, a table using it, and a procedure with one parameter using that alias. A unique SQL login/user received SELECT on the table and EXECUTE on the procedure, without metadata or type privileges. Under that login:

- Direct `EXEC dbo.[unique_proc] @x=N'readable'` returned `readable`.
- The real Engine successfully ran `SELECT * FROM dbo.[unique_table]`.
- A native query of the visible procedure's `sys.parameters` row returned SQL NULL for `TYPE_NAME(user_type_id)`. The owner control returned the actual alias name.
- Public `Engine::catalog` returned `Err(Failed { message: "Missing SQL Server metadata field 2" })`.

This is another legitimate metadata-visibility NULL, outside the default/computed/CHECK fields corrected in R5. It is not an unsupported CLR value or an attempt to execute model SQL. One accessible procedure with an unavailable parameter type must not prevent all otherwise readable table metadata from loading.

**Bounded suggestion:** Handle unavailable parameter type names at the SQL Server metadata boundary. Preserve the visible routine and parameter identity/order and other catalog data, while marking or omitting unavailable type text honestly. Do not require additional privileges merely to keep the catalog usable. Keep owner-visible type names intact.

**Regression test:** Create the unique alias/table/procedure/login arrangement above, prove both a native procedure call and Engine table read work under the restricted login, and require Engine catalog loading to succeed with the visible relation and routine. Include the owner control.

**Evidence:** `reviewer_alias_type_visibility` in `/tmp/l1data-mssql-review-b-r6-probe.rs`; final combined run `/tmp/l1data-mssql-review-b-r6-confirmed.log`. The final test intentionally fails at the catalog assertion after observing the public error, then completes cleanup.

**Cleanup and limits:** Only unique disposable objects and a unique login/user were created. The probe closed its Engine and native reader, dropped its procedure/table/type/user, waited for that login's sessions to disappear, then dropped its login. Independent owner queries asserted zero remaining matching objects, type and login. No seeded rows or privileges of existing users changed. This establishes the alias-type parameter case, not every possible metadata-visibility restriction.

## W2 — warning: Native `sp_rename` leaves the catalog stale and its table entry unusable

**Location:** `crates/engine/src/sql/tsql.rs:363–381`, with its consumer at `crates/engine/src/console.rs:362–365`. The invalidation scan considers only `ddl_at` matches, which recognize CREATE/ALTER/DROP/TRUNCATE/GRANT/DENY/REVOKE. A normal `EXEC sys.sp_rename` yields `Invalidates::Nothing`.

**Reachable call chain:** Public `Engine::console_run` → native SQL Server run plan → `console::run_task` → `tsql::invalidates(sql)` → `ConsoleInner::invalidate`. Because no obligation is recorded, the R5 native-end settlement policy has nothing to publish after the successful rename.

**Reproduction and observed state:** A real Engine connected to `shop`, loaded its catalog containing a unique empty disposable table, and executed:

```sql
EXEC sys.sp_rename N'dbo.unique_old_name', N'unique_new_name', N'OBJECT';
```

The run finished Ok. A separate native session verified the new table exists. After two seconds and another public catalog read, the version remained `2 → 2`, the old name remained cached, and the new name was absent. Running `RunRequest::TablePage` for that cached old table then failed with SQL Server error 208, `Invalid object name`. Explicit public `refresh_catalog` advanced the version to 3 and repaired both names.

This is a common SQL Server schema operation supported by the retained native execution path. Its omission causes the advertised live catalog to retain a broken table entry until the user refreshes manually. It is independent of the transaction-end fix: this reproduction is an ordinary successful Auto-commit rename with no open transaction.

**Bounded suggestion:** Account for supported SQL Server metadata-changing system procedures, starting with `sys.sp_rename`, in the canonical invalidation planner. Feed the existing pre-execution obligation and native-end settlement path; preserve the user's batch and avoid observation SQL on the console session. If handling opaque invocations conservatively, keep that decision in the same SQL interpretation layer rather than introducing UI procedure checks.

**Regression test:** Through the public Engine, load a catalog, rename a disposable table using native `sp_rename`, and require automatic catalog version/name convergence without an explicit refresh. Verify the newly named table browses successfully. A column rename is a useful second case.

**Evidence:** `reviewer_rename_invalidation` in `/tmp/l1data-mssql-review-b-r6-probe.rs`; `/tmp/l1data-mssql-review-b-r6-confirmed.log` records the successful native rename, unchanged cache, failing cached TablePage and manual-refresh control.

**Cleanup and limits:** The test dropped both unique possible table names and asserted that neither exists. The table was empty. This establishes this built-in rename path; it does not claim exhaustive detection of arbitrary stored-procedure or dynamic-SQL side effects.

## Checks and review bounds

Read AGENTS.md, README capability limits, architecture load-bearing decisions and reconciliation, `.audit/mssql/implementation.md` final-scope override, review history/dispositions, the R5 B report, and the narrow vendor patch documentation/delta. Reviewed the full feature diff inventory and traced selected older driver/dialect, identity, history/persistence, named-key and frontend target/mode paths as well as the R5 changes. This was a bounded adversarial review, not a claim to have inspected every upstream or application line.

Built the pinned integration target using `cargo test -p engine --test mssql_session_state --features mssql-tests --no-run --message-format=json`; build passed. `/tmp/l1data-mssql-review-b-r6-build.py` selects the exact Engine and dependency rlibs from that Cargo JSON and compiles the temporary test executable. No guessed/stale rlib glob was used. The final scoped run contains two expected failing regressions above and two passing checks.

The independent native-state probe passed:

- A table SELECT under IMPLICIT_TRANSACTIONS ON reports `Open` / unknown depth; the Commit button completes it.
- A refused raw-money conversion drains the original response through its later native BEGIN, UPDATE and result. The session remains usable and correctly Open; subsequent native state and table values match the raw control, followed by Rollback.
- A dynamic EXEC that leaves extra BEGIN nesting reports the same native error and open transaction as the raw control, followed by Rollback. An initial probe incorrectly expected that native EXEC to succeed; it was corrected and does not support a finding.

Initial alias-probe setup incorrectly created and referenced a type in one batch, and a later cleanup attempt raced asynchronous reader shutdown. Neither is application evidence. Separate cleanup probes verified removal of both initial unique fixture sets; the final probe separates type creation and waits for login session release before DROP LOGIN. The final combined log records successful cleanup for both findings.

No additional independent structural blocker was established. The enum-backed private drivers, focused T-SQL modules and removal of unsupported write machinery fit the existing architecture. A changed-file line-count comparison found no existing non-vendor application source file crossing from below 1,000 lines to at least 1,000 lines. Existing larger files predate the change. These bounded checks are not an exhaustive maintainability approval.

No source was edited, nested agent dispatched, real provider called, seeded row written, service changed/restarted, commit made or PR opened. Temporary Engine stores and session-local tables were scoped to the probes. HEAD remained pinned; the only working-tree change observed at the end was the root-owned `docs/images/l1-data.png` screenshot.

TLS/container validation, full suites, browser/native UI acceptance, screenshots and remote CI were not rerun or claimed here. **Root UI/native and CI gates remain owner-owned.** Prior parent-provided broad passes are context, not this reviewer's verification or an approval.
