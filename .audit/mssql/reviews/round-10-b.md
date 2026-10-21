# Independent review B, round 10

Reviewed revision: `0812b40f2198d14c69ef7f5d1a5121ccd227923b` against `main`.

Verdict: **Approve within the bounded code-review scope below.** No findings: **0 critical, 0 warning, 0 nit**. This is my independent verdict only. It is not a panel verdict or release approval, and it does not carry forward any previous review's approval.

I read the repository instructions, README capability limits, architecture load-bearing decisions and current reconciliation, and the final capability/protocol overrides in `.audit/mssql/implementation.md` and the historical dispositions in `review.md`. I accepted the intentional first-release boundary: SQL Server native Auto-commit scripts remain available; app-managed Manual mode and staged writes must refuse, and saved Manual mode must not silently become Auto.

Review concentration was the new activity model, count ownership/lifetime, page queue and routine insertion templates, with surrounding shared-driver/native-execution and persistence context. Traced paths included:

- `Engine::exact_row_count` in `crates/engine/src/lib.rs:957` through setup revalidation, `Console::take_session`, `console::count_task` in `console.rs:434`, the SQL Server stream/lease, revision publication and `return_session`. I checked result/history preservation, count errors, cancellation, abandoned callers, close/disconnect, and refresh settlement against the existing ownership model.
- `ConsoleWork` derivation from Busy ownership, `status.ts` busy/cancellation distinction, `TopBar`, `cancelActive`, and `runQueuedPages`. I followed pending-native-count and page-intent identities through count replies arriving before/after clearing snapshots, newer filters/pages, cancellation and session replacement.
- Rust SQL Server catalog routine/parameter reads through `routine_insert_text` and both tree paths' shared `FunctionRow`, including separately quoted schema/name identities, procedure syntax, ordered placeholders, TVP limitations and preservation of PostgreSQL insertion behavior.
- The narrow vendored protocol delta in `vendor/tiberius/UPSTREAM.patch`, native error/response draining and transaction presence, generated count caller migration, kind-aware historical data, and current SQL Server capability refusals. This was not an exhaustive re-audit of every unchanged upstream vendor line or every earlier review fixture.

No actionable correctness, security, or structural regression was established. The new work field is derived from the existing exclusive session owner rather than a second activity state. The count task reuses the existing session return/orphan machinery. Frontend pending-count identity handles IPC sequencing separately from the engine-owned activity snapshot. Routine insertion now lives in the canonical SQL-owning layer. I did not find a concrete simplification that would remove substantial complexity while preserving these observed behaviors.

## Checks independently executed

1. `cargo test -p engine --features 'pg-tests mssql-tests' --test mssql_counts --test routine_insertion --message-format=json`: PASS, 6 SQL Server count integrations and 2 SQL Server/PostgreSQL routine integrations. These exercised native settings/uncommitted rows, original results/history, native error/refresh behavior, Busy competitors, cancellation, abandonment, replacement sessions, and quoted routine templates. Their disposable-object cleanup assertions passed.
2. `pnpm exec vitest run src/features/editor/runActions.test.ts src/features/sidebar/FunctionRow.test.tsx src/shell/TopBar.test.tsx`: PASS, 61 tests across 3 files. This covers stale count contexts, count/page IPC ordering, newer page/filter intents, cancellation and toolbar activity, and actual routine row insertion behavior. These are frontend tests, not a claim of new browser/WKWebView acceptance.
3. Additional disposable public Engine probe: PASS. With `XACT_ABORT OFF`, `LOCK_TIMEOUT 120`, a native open transaction and a session temp-table marker, another tab held an exclusive lock on a uniquely named disposable table. The exact count returned the real lock-timeout error within five seconds, cleared `work` to None, preserved the original run/result and same session, and retained native `@@TRANCOUNT=1`, `XACT_STATE()=1`, and temp marker `73`. Releasing the blocker allowed a subsequent count of 3, followed by rollback. Cleanup verified `OBJECT_ID` NULL for `dbo.l1_native_count_fafb44cc037847bc8787e34f84eaee42`, then closed/disconnected/shut down the disposable Engine.

Probe source: `/tmp/l1data-review-b-r10-probe.rs`. Probe output: `/tmp/l1data-review-b-r10-probe.log`. It was compiled using exact current Cargo JSON artifact paths from `/tmp/l1data-review-b-r10-artifacts.jsonl`, not a guessed rlib. The probe reuses the scoped test harness in a temporary source copy; production source was not edited.

## Limits and ownership

The review is bounded by the requested timebox. I did not call real assistant providers, touch seeded rows, create a PR/commit, delegate, or change/restart services. Database changes were confined to unique disposable test objects and their own rows; the tests and extra probe completed cleanup. The probe's temp marker was session-local and was removed when its session closed.

I did not rerun the full workspace, TLS/container matrix, native builds, actual browser/WKWebView interactions, screenshots or remote CI, and I do not independently attest the parent's supplied evidence for those checks. **Root UI/native acceptance and CI gates remain owner-owned.** Their pending status is not a code finding.

Final source verification: HEAD remained the requested pin and `git status --short` was empty. I have stopped review work after writing this report.
