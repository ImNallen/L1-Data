# Independent review C r6

Pinned revision: `7ebc2f07c2bbfec284317caebab4942078c36997`.

**Verdict: request changes.** Findings: 0 critical, 1 warning, 0 nit.

## Warning: `EXEC` can change schema without refreshing the cached catalog

**Location:** [crates/engine/src/sql/tsql.rs](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/sql/tsql.rs:363), reached from [crates/engine/src/console.rs](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/console.rs:362).

`run_task` records SQL Server refresh obligations by calling `tsql::invalidates(sql)` before sending the native batch. `invalidates` only recognizes the direct `CREATE`/`ALTER`/`DROP` family through `ddl_at`; it returns `Nothing` for `EXEC`/`EXECUTE`, even though `is_write` recognizes those as potentially effectful. The engine therefore executes a stored procedure unchanged but schedules no catalog refresh when the procedure creates or drops an object. `Engine::catalog` continues serving the old `DbSlot` snapshot, so the tree/completion view can omit an object that already exists.

I reproduced this through a disposable real Engine probe against the SQL Server fixture. The probe loaded the `shop` catalog, created a uniquely named procedure whose body creates a uniquely named table, then invoked the procedure through `console_run` in Auto-commit mode. The observer query returned a non-null `OBJECT_ID`, while the catalog stayed at version `2` and did not contain the table after `EXEC`. Calling `refresh_catalog` advanced the version and loaded the table. The probe then dropped the unique table and procedure and asserted both `OBJECT_ID` values were null. The temporary test source was removed afterward.

**Suggested fix:** conservatively record refresh obligations for opaque execution constructs such as `EXEC`/`EXECUTE` (including dynamic SQL), while continuing to send their text unchanged. At minimum this needs a catalog refresh; treating opaque calls as potentially changing the database list is safer where the supported SQL Server procedure surface allows it. Existing `RefreshGate` coalescing can bound the extra metadata work. Add an Engine regression that invokes a procedure or dynamic batch which creates and drops a table.

## Review scope and checks

Read the repository instructions, README capability limits, architecture load-bearing decisions and reconciliation, plus the final-scope paragraph in `.audit/mssql/implementation.md` and the integrated review dispositions. Traced the final SQL Server query/refresh path, the protocol-only state getter/lease retirement path, and the restricted-login detail handling. I found no additional reportable issue in those reviewed paths.

Ran the focused disposable Engine reproduction described above. The probe passed because it asserted the observed stale-catalog behavior, and its cleanup assertions passed. Independently ran `python3 vendor/tiberius/verify-upstream.py` against the cached pinned 0.13.0 crate; SHA and the recorded seven-file patch matched (96 upstream files unchanged).

The root-owned final UI/native acceptance and remote CI gates remain outstanding owner work; I did not run or claim them. This report is an independent review result, not approval.
