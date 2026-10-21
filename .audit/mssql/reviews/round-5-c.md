# Independent review C — R5

**Revision:** `98301df8f4d2c742931875f6e9e297f464989c9d`
**Verdict:** Request changes — **1 warning**, 0 critical, 0 nit.

## Finding

### Warning — SQL Server rollback can leave committed DDL missing from the cached catalog

**Location and call chain:** [console.rs](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/console.rs:329) records SQL Server DDL invalidations in `ConsoleInner.owed`. While the observed transaction remains active, `run_task` retains that obligation. [lib.rs](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/lib.rs:578) handles `console_rollback`: after the driver reports Idle, it takes the owed refresh at line 582 but returns a refresh request only when `committed` is true at lines 583–584. The caller schedules only a returned request at lines 587–588. A native rollback therefore discards the SQL Server invalidation.

**Observed behavior:** I built the pinned Engine and ran a disposable public-Engine probe against the local SQL Server fixture. A unique `AFTER CREATE_TABLE` database trigger committed the original transaction and began a replacement transaction when the probe created a uniquely named table. The Engine reported the batch `Ok` with `txn=Active`, depth 1. `console_rollback` returned successfully and the console became Idle, but an observer still counted the table in `sys.tables`. The cached catalog stayed at version 3 and did not contain it; an explicit `refresh_catalog` advanced to version 4 and exposed the table. The Engine currently presents rollback as the end of the transaction while leaving its catalog snapshot stale.

**Bounded fix:** When a SQL Server native transaction ends, conservatively schedule accumulated catalog/database refresh obligations even after Rollback. Keep PostgreSQL's confirmed-rollback behavior unchanged. Apply the same reasoning to forced close or lost-session paths that clear owed SQL Server refreshes.

**Probe and cleanup:** The probe used unique names and changed no seeded rows. It dropped the temporary table and database trigger, refreshed and verified the table was absent, closed its tabs, and removed its private Engine store. One initial setup attempt used an invalid schema prefix for a database trigger and failed before creating it; that attempt's private store was also removed. The successful probe used the exact artifact from `cargo build -p engine --features mssql-tests --message-format=json`.

## Review limits

I read `AGENTS.md`, the README SQL Server limits, the architecture load-bearing decisions and implementation reconciliation, and the final-scope and review audit notes. I traced the final Engine Manual/edit refusals, SQL Server session dispatch, T-SQL table SQL path, assistant refusal, and PostgreSQL named-key state path. I found no additional actionable issue in those paths during this timeboxed pass. I did not rerun the supplied full suites or drive the UI. The root UI and remote CI gates remain owner-owned; no approval is implied by this report.

No production source was edited. The shared worktree already showed a changed `docs/images/l1-data.png`; I left that owner-owned artifact untouched.
