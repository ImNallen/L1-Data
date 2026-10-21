# Independent review C, revision a3b1add

## Verdict

No actionable findings in this bounded independent review. Counts: 0 critical, 0 warnings, 0 nits. This is my review result only, not a panel or release approval.

## Scope and evidence

I reviewed the final SQL Server capability override and the current source at `a3b1add9b5d244a54193c76e19629c3b818ff8b3`. I read `AGENTS.md`, the README capability limits, the architecture load-bearing decisions and implementation reconciliation, `.audit/mssql/implementation.md`, and `.audit/mssql/review.md`. The fresh code pass concentrated on the R8 count correction and its reachable call chain: `exact_row_count` in `crates/engine/src/lib.rs`, the exclusive console session and `count_task`, the SQL Server lease/cancellation behavior, generated count and page SQL, the Tauri/bridge binding migration, and the Last-page UI stale-context checks. I also checked the existing database-scoped invalidation settlement around session return and retirement.

The focused real Engine integration command passed:

```text
cargo test -p engine --features mssql-tests --test mssql_counts -- --test-threads=1
6 passed, 0 failed
```

The six cases exercise native `SET` context and uncommitted rows, refresh settlement after a native error, lazy session setup and query errors, competing operations and cancellation, close/disconnect while blocked, and caller abandonment. The harness uses a unique disposable table per case and asserts its removal during cleanup. No service was restarted.

The traced ownership path is consistent with the documented session invariant. SQL Server count validates the tab and expected database before metadata work and again after awaits, then takes the same exclusive Busy slot as a run. Its worker returns or orphans the session through the run-id ownership check. Abandonment drops the TDS lease and retires the session. The frontend checks the captured tab target, table query, text, page, and page size before starting the counted page request and through console setup/acceptance.

## Limits

I did not rerun the full workspace or database suites, TLS/container checks, frontend suite/build, bridge smokes, desktop/native UI acceptance, or remote CI. The root-owned UI/native and CI gates remain with the parent reviewer. Earlier review reports and test claims were treated as context, not as approval for this revision.
