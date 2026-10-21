# Independent SQL Server review A — round 9

Reviewed commit: `a3b1add9b5d244a54193c76e19629c3b818ff8b3` against `main`.

**Verdict: request changes. Counts: 0 critical, 1 warning, 0 nit.**

This is my independent, bounded review. Earlier reviewers' verdicts and the parent's passing gates are context, not my approval. I accepted the stated first-release capability boundary: native SQL Server scripts and transaction buttons remain supported; app-managed Manual mode and staged writes are unavailable. I read the repository instructions, README limits, architecture decisions/reconciliation, and historical/current audit dispositions. I concentrated on the new count ownership path and its interaction with the existing driver, console lifecycle, refresh obligations, generated callers, and frontend activity model; I also inspected retained shared-driver, metadata, SQL construction, persistence, and assistant boundaries.

## A9-W1 — warning: a blocked Last-page count monopolizes the tab but cannot be cancelled from its UI

**Primary introduced location:** `crates/engine/src/lib.rs:1017–1022` (`exact_row_count` takes the tab session and publishes it without exposing the count activity).

**Reachable call chain:** `ResultsPane.tsx:106` Last button → `runActions.ts:334–345` `lastTablePage` → generated `exactRowCount` → `Engine::exact_row_count` → `Console::take_session` → `count_task`. `Console::snapshot` at `console.rs:150–158` collapses both Idle and Busy sessions into `SessionState::Open` and retains the preceding `RunState::Finished`. `status.ts:8–9` recognizes activity only when that run is `Running`. Consequently `TopBar.tsx:120–139` renders Run instead of Cancel, and `runActions.ts:208–213` makes Esc return false before it can invoke `console_cancel`.

**Observed result:** I created a uniquely named disposable SQL Server table with three rows, set the temporary Engine's page size to one, opened a native transaction, and successfully loaded a TablePage. A second tab held `TABLOCKX,HOLDLOCK` on that table. The real public `exact_row_count` then remained pending, while the published snapshot was `run=Finished`, `session=Open`, `txn=Open`. Competing Run and Rollback requests both returned Busy. Feeding that actual captured snapshot into the current TopBar and cancellation handler proved:

- The pager's Last action was eligible for the full first page.
- Run was rendered and Cancel was absent.
- `cancelActive()` returned false and sent zero cancellation IPC requests.
- Calling `Engine::console_cancel` directly released the blocked operation and retired its session, proving that the driver cancellation path exists but the user cannot reach it through the advertised controls.

This is an introduced consequence of moving the count from metadata onto the tab's exclusive session. A count can wait indefinitely behind a lock with the current unlimited command timeout. While it waits, the tab rejects further execution and transaction buttons; the user has to release the blocker elsewhere or close/disconnect the session. The same hidden Busy state also bypasses the frontend's page-request queue predicate (`runActions.ts:143`). This finding is not merely the pending root UI acceptance gate.

**Bounded suggestion:** Preserve the previous result/run/history, but expose the owned count's cancellable activity in the console state or an equally authoritative activity model. Use that state for the toolbar, Esc, and busy action/page handling. Do not overwrite the finished run just to manufacture a spinner, and do not move counts back to metadata. Add a test that holds a real count blocked, feeds its published state through the frontend controls, and cancels through the actual UI action.

**Evidence:**

- `/tmp/l1data-review-a-r9-count-probe.rs` and `/tmp/l1data-review-a-r9-count-probe.log`: public Engine reproduction, compiled against exact Cargo JSON artifact paths, not guessed rlibs.
- `/tmp/l1data-review-a-r9-count-state.json` and `/tmp/l1data-review-a-r9-count-context.json`: captured live state and page context.
- `/tmp/l1data-review-a-r9-ui/ui.test.ts`, `/tmp/l1data-review-a-r9-ui.log`, `/tmp/l1data-review-a-r9-ui-evidence.json`: current frontend component/handler assertions using the captured state.
- `/tmp/l1data-review-a-r9-count-toolbar.html`: rendered toolbar.

The final custom probe removed `dbo.l1_native_count_f3b6b6c551f84bfc95aad4c5236f0287` and verified `OBJECT_ID` was NULL. The earlier probe likewise verified its uniquely named table absent. Control transactions were rolled back, both tabs closed, and the temporary Engine disconnected and shut down. No seeded rows were changed.

## Checks and review limits

Independently executed successfully:

- Six `mssql_counts` public Engine integrations, including native settings/uncommitted rows, error refresh settlement, Busy competitors, cancellation, abandonment, and close/disconnect replacement ownership. Log: `/tmp/l1data-review-a-r9-count-tests.log`.
- Seven `mssql_session_state` integrations covering native registers, guarded writes, errors, transaction buttons, conversion draining, restricted login and cancellation. Log: `/tmp/l1data-review-a-r9-mssql_session_state.log`.
- Eight `mssql_refresh_outcomes` integrations, including PostgreSQL commit scope and rollback/retirement controls. Log: `/tmp/l1data-review-a-r9-mssql_refresh_outcomes.log`.
- One deterministic count catalog-await revalidation unit; two assistant policy/execution-boundary units, with no real provider. Logs: `/tmp/l1data-review-a-r9-{setup-guard,assistant-policy,assistant-boundary}.log`.
- All 41 existing `runActions.test.ts` and `TopBar.test.tsx` tests, plus the independent real-state frontend probe. The existing passes do not cover A9-W1. Log: `/tmp/l1data-review-a-r9-frontend.log`.
- Vendored Tiberius archive checksum and exact patch verification: 96 unchanged files and only the seven documented modifications. Log: `/tmp/l1data-review-a-r9-vendor.log`.

The database tests' cleanup assertions passed, including disposable objects, temp tables and the restricted login. The exclusive count task's result preservation, orphan handling and native-end refresh settlement held under the exercised cases. I found no additional concrete correctness, security or structural blocker in the inspected paths. The pre-existing large engine facade is already above 1,000 lines on main; I did not promote that baseline size alone into a new finding.

I did not rerun the full workspace, TLS/container matrix, production/native builds, or actual browser/WKWebView acceptance. The frontend finding uses the real Engine snapshot with component rendering and the real action handler, not a claimed end-to-end browser run. Root UI/native acceptance and remote CI remain owner-owned gates. No source edits, commits, PRs, nested agents, service changes, or real-provider calls were made. The source remained pinned and the tracked worktree was clean at the final check.

**Review stopped after this report.**
