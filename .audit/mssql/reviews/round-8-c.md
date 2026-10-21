# Independent review C, R8

## Verdict

**Request changes:** 1 warning, 0 critical, 0 nit.

The bounded review found one user-visible SQL Server correctness issue in exact filtered counts. The three focused SQL Server Engine tests and one additional disposable Engine probe completed successfully, with disposable objects cleaned up. This is a fresh finding against the pinned `63acd96127fab0e0178bb61232f71e06afb79d0e`; prior round reports are not approvals.

## Findings

### Warning — filtered SQL Server counts ignore the table tab's session settings

**Location and reachable path:** `Engine::exact_row_count` in `crates/engine/src/lib.rs:954-968` builds a count from `TableQuery`, then calls `session_in` with the database slot's `meta` session (`:966-968`). SQL Server `TablePage` instead runs through `console_run` and the tab-owned session (`crates/engine/src/lib.rs:1065-1080`, then the console run task). In the UI, `lastTablePage` calls `commands.exactRowCount(db, schema, table, query)` (`src/features/editor/runActions.ts:325-334`); the corresponding page request uses `RunRequest::TablePage` on that tab (`:142-147`). Raw T-SQL predicates admit `DATEPART` (`crates/engine/src/sql/tsql/expression.rs`, `SCALAR_FUNCTIONS`) and are passed to both browse and count SQL (`crates/engine/src/text.rs:485-489`).

**Observed state:** A temporary public-Engine probe created a uniquely named table containing 2024-01-01 and 2024-01-02, executed `SET DATEFIRST 1` on the table tab, and used the valid raw filter `DATEPART(weekday, [d]) = 1`. The tab reported `@@DATEFIRST = 1`; its `TablePage` returned the Monday row (`id=1`); `exact_row_count` returned `Ok(0)`. SQL Server's weekday result depends on `DATEFIRST`, and the count ran on the separate metadata session. The probe dropped the table and verified `sys.tables` count `0`, then closed the tab, disconnected, and shut down its disposable Engine. It did not modify seeded rows.

The UI exposes this through **Last page**: its page index is calculated from the wrong count and its persisted exact total can disagree with the rows the table tab just returned. This violates the first-release count behavior even though paging and ordinary typed filters work.

**Bounded suggestion:** Execute filtered SQL Server counts using the table tab's session context, or reject/document session-dependent raw predicates when the count cannot preserve that context. Do not return an apparently exact count that can contradict the page. A regression should set `DATEFIRST` on the table tab, compare the page and count for a weekday filter, and verify cleanup.

## Checks run

- `cargo test -p engine --features mssql-tests --test mssql_cross_database_refresh -- --nocapture` — 3 passed. Its disposable tables were verified absent in `shop` and `reporting`; tracked-slot and second-connection controls passed.
- `cargo test -p engine --features mssql-tests --test mssql_session_state server_errors_preserve_error_number_and_recover_without_observation_sql -- --nocapture` — 1 passed. Uses a session-local temporary table and its test cleanup verified removal.
- `cargo test -p engine --features mssql-tests --test mssql_transactions manual_configuration_and_staged_edits_refuse_before_trigger_execution -- --nocapture` — 1 passed. The test verified refusal before trigger execution and cleans its unique table, trigger, sequence, and audit table.
- Temporary public-Engine `DATEFIRST` probe — reproduced the mismatch and verified the unique table absent (`cleanup_count=0`). Probe files are under `/tmp/l1data-review-probe`; no repository source was edited.

I did not rerun broad suites, the UI, the desktop bundle, or remote CI. Those final UI and CI gates remain owner-owned.
