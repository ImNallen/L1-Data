# Independent reviewer A — round 8

Verdict: **APPROVE within this bounded code review; no findings.**

Counts: **0 critical, 0 warning, 0 nit.** This is my own review result, not a panel verdict or release approval. Root-owned UI/native acceptance and remote CI gates remain outstanding owner-owned gates.

Reviewed pin: `63acd96127fab0e0178bb61232f71e06afb79d0e`, against `main...HEAD`, in `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`. The pin and clean tracked working tree were checked at the beginning and end. No production source, tracked tests, audit documents, services, providers, commits or PRs were changed.

## Scope and reasoning

Read AGENTS.md, README capability limits, architecture load-bearing decisions and reconciliation, the authoritative final capability/protocol overrides in `.audit/mssql/implementation.md`, and the review dispositions. Historical Manual/staged-write plans were not treated as the current contract.

The deepest tracing covered private session ownership and abandonment; protocol-only identity/transaction observations; original-response error and conversion draining; native transaction buttons and cancellation; SELECT INTO/opaque execution invalidation; settlement after native transaction completion; tracked-slot cross-database refresh; immutable connection kind and exact home alias; SQL Server metadata/type fallbacks and value conversion; generated browse/filter/order/count SQL; persistence compatibility; PostgreSQL driver migration and named staged keys; and assistant refusal at approval and execution boundaries.

Relevant call chains checked include:

- `Engine::console_run` → `txn::plan` → `console::run_task`/`run_outputs` → private `Session::query` → `MsSession::query`/`native_query` → `Client::session_state`. No observation SQL remains in the native console path. Unsupported conversions drain the original response; uncertain or abandoned sessions cannot return a leased native client.
- `tsql::invalidates` → `ConsoleInner::invalidate`/`finish_refreshes` → `Shared::schedule_refreshes` → each tracked slot's existing `RefreshGate`. The new SELECT INTO predicate handles nested SELECT scopes without rewriting execution SQL. Native-end settlement retains the original connection; the SQL Server fan-out stays within its tracked slots. PostgreSQL retains its separate commit-only policy.
- `console_configure`, `console_run`, and `table_apply` reject unsupported SQL Server capabilities at their boundaries; `table_apply` rechecks under the console lock after awaits. The deleted SQL Server managed-write implementation is not retained as a reachable alternate route.
- Assistant `policy` refuses SQL Server query requests before approval; `execute` checks the instance kind, and `Session::open_assistant` independently refuses a non-PostgreSQL driver.

No actionable structural regression emerged in these paths. The facade keeps native clients private, and the final capability removal removes the dangerous planner/write branches rather than extending them. Refresh behavior is centralized through the existing obligation/gate mechanism. The engine-source line-count check found no file crossing from below 1,000 lines to above it: the existing `lib.rs` grows from 1,650 to 1,927; `console.rs` decreases from 535 to 517; the new T-SQL root is 921 lines including tests and already separates expression, format and table helpers. This is not a claim that every frontend or upstream line was exhaustively reviewed.

## Independently executed evidence

All test executables and custom-probe dependency paths were taken from fresh Cargo JSON artifacts, not guessed rlibs. No full expensive workspace suite was rerun.

| Check | Observed result | Evidence |
|---|---|---|
| `mssql_session_state` | 7 passed: startup/register preservation, GO/separate-run native effects, errors, transaction buttons, conversion drain, low-privilege identity, cancellation/Busy | `/tmp/l1data-a-r8-mssql_session_state.log` |
| `mssql_opaque_refresh` | 8 passed: SELECT INTO, EXEC/EXECUTE/implicit calls, dynamic execution, database list, commit/rollback settlement and rename/TablePage convergence | `/tmp/l1data-a-r8-mssql_opaque_refresh.log` |
| `mssql_cross_database_refresh` | 3 passed: three-part and cross-database dynamic DDL, retirement/retarget, independent connection and ordinary-read controls | `/tmp/l1data-a-r8-mssql_cross_database_refresh.log` |
| `pg_key_identity` | 1 passed: Auto/Manual update and delete refuse after PK replacement/catalog refresh; observer rows unchanged | `/tmp/l1data-a-r8-pg_key_identity.log` |
| `mssql_transactions` | 6 passed: Manual/staged refusal, trigger/sequence sentinel, prior native work/XACT_ABORT preservation, module transactions, error/cancel behavior and earlier committed writes | `/tmp/l1data-a-r8-mssql_transactions.log` |
| Narrow vendor verification | Pinned archive checksum verified; 96 included files unchanged; seven changed files exactly match the 210-line recorded patch | `/tmp/l1data-a-r8-vendor.log` |

Total: **25 passing focused integration tests**, plus two additional disposable public-Engine probes:

1. `/tmp/l1data-a-r8-native-probe.rs`, log `/tmp/l1data-a-r8-native-probe.log`, exit 0. Reused the test harness's raw Tiberius control and public Engine calls to compare additional native cases: caught XACT_ABORT failure with the server's batch-end rollback, nonfatal server error followed by two BEGINs, raw-money conversion refusal followed by a native write and BEGIN, and NOEXEC ON/OFF. Transaction state, native registers and resulting temp-table values matched the control where compared. The conversion refusal correctly drained the later write and transaction tokens while reporting failure; no observation query changed their effects.
2. `/tmp/l1data-a-r8-refresh-probe.rs`, log `/tmp/l1data-a-r8-refresh-probe.log`, exit 0. Nested scalar SELECTs inside a CTE followed by SELECT INTO, and SELECT INTO inside TRY/CATCH after an earlier SELECT, both produced the expected native table, newer catalog, and exact TablePage value. Catalog versions advanced 1→2→3→4→5 across creation/deletion cycles.

A first version of the native probe incorrectly assumed a caught doomed transaction would survive batch end. The raw control instead returned SQL Server error 3998 and rolled it back. The probe was corrected to compare actual native behavior; that rejected hypothesis is not a finding. The harness caught the failed attempt and ran cleanup.

Build/selection records: `/tmp/l1data-a-r8-cargo.json`, `/tmp/l1data-a-r8-test-artifacts.json`, `/tmp/l1data-a-r8-boundary-artifacts.json`.

## Limits and cleanup

The probes used session-local temporary tables or UUID-named disposable tables/procedures/databases/logins/schemas. Cleanup paths verified absence through independent queries where provided by the harness, closed sessions, and shut down their private Engine instances. No seeded rows were written. No shared service was stopped, restarted or reconfigured. Only reports, probe sources/binaries/logs under `/tmp` and normal Cargo build artifacts were produced.

This pass did not independently rerun GUI/WKWebView flows, TLS containers, real providers, all frontend tests or the full workspace suite; supplied parent results were context, not substituted for checks claimed above. Hidden metadata combinations beyond the traced/tested cases and arbitrary T-SQL forms remain bounded coverage, not an exhaustive proof. I reviewed the narrow vendor delta, not 38k unchanged upstream lines.

**Root UI/native acceptance and CI gates remain owner-owned. Reviewer work is stopped after this report.**
