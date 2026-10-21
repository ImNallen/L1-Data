# Independent SQL Server review A — round 10

Reviewed pin: `0812b40f2198d14c69ef7f5d1a5121ccd227923b`, against `main`.

**Verdict: approve within this bounded code review. Findings: 0 critical, 0 warning, 0 nit.**

No actionable correctness, security, or structural finding was established. This is my own review, not a panel verdict or release approval. Earlier approvals and the parent's reported checks are context, not substituted evidence. The final native-only SQL Server transaction contract is accepted: app-managed Manual mode and staged grid writes must refuse; native scripts and transaction buttons remain supported.

## Scope and traced paths

I read AGENTS.md, the README capability limits, architecture load-bearing decisions and relevant implementation reconciliation, the final capability/protocol overrides in `.audit/mssql/implementation.md`, and the historical/current dispositions in `.audit/mssql/review.md`. I inspected the full feature's changed-file inventory and relevant source/diffs, concentrating on the changes since a3b1add. This was a bounded review, not an exhaustive examination of every changed line.

- `Engine::exact_row_count` → target/instance/console/mode/Busy validation → `Console::take_session(..., Count)` → `console::count_task` → private `Session::count`/`MsSession::count` → `Console::edit`/`return_session`. The task uses the existing exclusive ownership identity, preserves the previous run/results/history, observes native transaction presence, retires an abandoned active lease, and settles native-end refresh obligations. The post-await guards and deterministic setup-race control cover stale target, close, disconnect, and unsupported legacy mode.
- All five `take_session` callers and `Console::snapshot` derive `ConsoleState.work` from actual Busy ownership. The required generated field reaches `isBusy`/`canCancel`, TopBar, ResultsPane, and the editor cancellation handler. Count remains separate from the previous Finished run; no fabricated RunState or transaction depth is introduced.
- `lastTablePage` → `pendingNativeCount`/immutable intent identities → `runTab`/`runQueuedPages`. I traced newer page/filter/page-size intent, count reply versus clearing-snapshot order, cancellation, loss/close, stale context checks through page setup/acceptance, and replacement counts receiving an older reply. The extra test below checks the latter sequence against the actual frontend functions with a deferred transport.
- `FunctionInfo.insert_text` is computed by the private catalog drivers and consumed verbatim by the shared `FunctionRow`, including both tree paths. SQL Server separately quotes schema and routine names, uses structured ordered parameter rows, excludes the return parameter, and distinguishes EXEC templates from function expressions. Editable NULL/output/TVP limitations are documented. PostgreSQL retains its previous insertion text. This nonpersisted field does not need a legacy stored-data reader.
- Retained feature boundaries inspected include private driver dispatch, native response draining and the narrow Tiberius protocol getter, SQL Server mode/write refusals, generated browse/filter/count SQL, refresh settlement/fan-out, connection identity, persisted kind/history fallback, PostgreSQL named keys, and assistant refusal before approval and execution.

No code-quality blocker emerged. The activity change extends existing session ownership instead of maintaining a competing activity store; the count queue contains client request ordering rather than authoritative engine activity. Routine SQL construction is now in its canonical Rust layer. The existing engine facade grows from 1,650 to 2,030 lines; no existing production Rust/TypeScript file crosses from below 1,000 to above 1,000 lines. This is not an endorsement of unlimited future facade growth. I reviewed the narrow vendor delta, not the unchanged upstream implementation as newly authored application code.

## Independently executed checks

All commands below completed successfully in this review.

| Check | Observed result | Evidence |
|---|---|---|
| `cargo test -p engine --features 'pg-tests mssql-tests' --test mssql_counts --test routine_insertion -- --nocapture` | 8 passed: six count integrations and two SQL Server/PostgreSQL routine integrations, with cleanup checks | `/tmp/l1data-r10-a-focused.log` |
| Same feature set, `mssql_session_state`, `mssql_transactions`, `mssql_refresh_outcomes`, `mssql_cross_database_refresh`, `pg_key_identity` | 25 passed: native register/error/transaction behavior, capability refusals, refresh lifecycle and connection scope, PostgreSQL named-key refusal | `/tmp/l1data-r10-a-boundaries.log` |
| `cargo test -p engine --features 'pg-tests mssql-tests' --lib count_ -- --nocapture` | 5 passed; includes the deterministic count catalog-await revalidation test and its four state-change variants. The name filter also selected unrelated pure unit cases. | `/tmp/l1data-r10-a-count-guards.log` |
| Vitest: editor runActions, TopBar, FunctionRow, status | 84 passed across four files | `/tmp/l1data-r10-a-frontend.log` |
| Extra native count probe | 2 passed, described below; both disposable tables verified absent | `/tmp/l1data-a-r10-native-count-probe.log` |
| Extra replacement-count frontend probe | 1 passed with actual application functions and mocked/deferred IPC | `/tmp/l1data-a-r10-queue-probe.log` |
| Vendor archive verifier | Pinned archive SHA-256 verified; 96 included files unchanged; seven changed files exactly match the recorded 210-line patch | `/tmp/l1data-r10-a-vendor.log` |

The extra public-Engine probe at `/tmp/l1data-a-r10-native-count-probe.rs` adds two checks beyond rerunning existing tests:

1. With `XACT_ABORT OFF`, an open native transaction first inserts a disposable row. A generated count raises divide-by-zero. The count clears its work ownership while retaining the original Finished run and an Open transaction with unknown protocol depth. A subsequent native query observes `@@ERROR=8134`, `@@TRANCOUNT=1`, and `XACT_STATE()=1`. An independent control session sees the inserted row under READUNCOMMITTED; the tab counts four rows. Rollback removes the insert, confirmed by the control session. The count neither silently resets the native transaction nor commits the prior write.
2. Under native `NOEXEC ON`, two counts fail honestly with no returned count row while preserving the same native session and original run. Explicit `NOEXEC OFF` restores a successful count of three. The failure path does not silently clear native options or replace the session.

The probe was compiled against exact fresh Cargo JSON artifact paths, recorded in `/tmp/l1data-a-r10-cargo.json` and `/tmp/l1data-a-r10-probe-artifacts.json`; no rlib path was guessed.

The extra frontend probe at `/tmp/l1data-a-r10-queue-probe.ts` cancels count A, publishes its lost-session state, starts replacement count B, and queues page 3. A's error arrives after B has acquired its session. The old reply does not clear B's pending identity or dispatch the page prematurely. Even B's clearing snapshot alone does not release the page; B's response releases exactly one page-3 request. This proves application ordering with a controlled transport, not an additional GUI/WKWebView or live-engine IPC acceptance run.

## Cleanup and limits

The database checks used their uniquely named disposable objects or session-local temporary tables. Cleanup assertions checked absence and closed the temporary sessions/Engine instances. The two additional native-probe tables were explicitly observed absent. No seeded rows were changed. No production source was edited, no commits or PRs were made, no real providers were called, no agents were spawned, and no shared services were stopped, restarted, or reconfigured. Temporary reports/probes/logs and ordinary build artifacts are the only reviewer-created artifacts. HEAD remained pinned and `git status --short` was clean at the final check.

I did not independently rerun the full workspace/frontend suites, production/native builds, TLS containers, GUI/WKWebView flows, or remote CI. I did not exhaustively exercise arbitrary T-SQL forms or every possible hidden-metadata combination. Those limitations are not findings and are not concealed by the bounded approval.

**Root UI/native acceptance, screenshots, and CI gates remain owner-owned. Reviewer work is stopped after this report.**
