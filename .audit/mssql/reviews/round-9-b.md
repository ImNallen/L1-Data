# Independent review B, round 9

Pinned source: `a3b1add9b5d244a54193c76e19629c3b818ff8b3` in `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`.

**Verdict: request changes. Counts: 0 critical, 2 warning, 0 nit.** These are my independent findings and checks. No panel or release approval is claimed.

## B9-W1 — warning: a blocked Last-page count owns the tab indefinitely without a working UI Cancel action

**Location and reachable chain:** `crates/engine/src/lib.rs:1017-1022` moves the session into the new count task and publishes without changing `inner.run`. `crates/engine/src/console.rs:145-158` projects both Idle and Busy session slots as `SessionState::Open` and retains that old run. `src/state/status.ts:8-9` defines activity solely as `run.kind === "Running"`. Therefore `src/shell/TopBar.tsx:120-138` renders Run instead of Cancel, and `src/features/editor/runActions.ts:208-212` returns false from `cancelActive` without sending `console_cancel`. The pager also uses that same predicate in `runTab:143-145`, so requests during a count are sent to the busy backend instead of entering the normal latest-page queue.

**Observed reproduction:** Through the public real Engine, I created a unique two-row table, loaded its TablePage, and held an exclusive table lock from a separate disposable tab. I then started `exact_row_count` on the browsing tab. After its revision rose from 7 to 8 and it remained blocked, the actual snapshot still said `run: Finished/Ok`, `session: Open`. A next-page request, Commit and Rollback each returned Busy. Feeding that exact captured snapshot into the actual frontend `cancelActive` function returned false and made zero IPC calls. A separate render of the actual TopBar with the same snapshot contained `run-btn` and no `cancel-btn`. This is a source/behavior defect, independent of the owner's pending browser acceptance. The driver config sets no command timeout (`session/mssql.rs:90`), so an ordinary blocking lock or long scan can hold the session until the external cause clears; closing the tab/disconnecting is the remaining UI escape and destroys session context.

Direct `Engine::console_cancel` immediately ended this probe's count with `Count cancelled`, confirming that the engine cancellation works but is unreachable through the normal UI control. The existing count integration's direct cancellation test therefore does not cover this gap.

**Bounded correction:** Preserve the finished run/results, but expose count activity independently in the revisioned console snapshot. Make the toolbar/Escape and page queuing consume that activity, so the operation that owns Busy is cancellable and later page actions can follow the established queue policy. Avoid a frontend-only timer that guesses whether the engine has returned the session.

**Regression and evidence:** Add an Engine-plus-UI blocked-count test proving a visible Cancel control, Escape dispatch, actual session retirement, and release of Busy. Also check that a page action during a count is queued or explicitly disabled rather than lost to Busy. My independent evidence is `/tmp/l1data-review-b-r9-probe.rs`, its `.log`, `/tmp/l1data-review-b-r9-count-snapshot.json`, `/tmp/l1data-review-b-r9-ui.test.ts`, `/tmp/l1data-review-b-r9.vitest.config.ts`, and `/tmp/l1data-review-b-r9-ui.log`. Both temporary frontend assertions pass while demonstrating the bug. Cleanup rolled back the lock owner, cancelled the count through the Engine, dropped the disposable table, and verified its absence.

## B9-W2 — warning: newly exposed SQL Server routine rows insert invalid routine calls

**Location and reachable chain:** The new `crates/engine/src/session/mssql/catalog.rs:117-127` populates the sidebar's routines with SQL Server scalar functions and procedures, including their distinct `FnKind`. Both ordinary and filtered tree paths pass them to `FunctionRow` (`src/features/sidebar/ConnectionTree.tsx:505,806`). Its double-click handler at `402-410` always inserts the bare `${fn.name}()` and discards the schema and routine kind. The handler predates this PR, but routing SQL Server routines into it is new. Even ordinary parameterless dbo routines now expose an action that always generates invalid text; this is not a claim about pre-existing PostgreSQL name edge cases. README line 20 advertises this action without a SQL Server exception.

**Observed reproduction:** I created one unique parameterless `dbo` scalar function returning 42 and one unique `dbo` procedure selecting 43. Public `Engine::catalog` returned both with the expected Function/Procedure kinds. I sent the exact strings emitted by the handler through public `Engine::console_run`:

- After typing `SELECT `, double-click insertion produces `SELECT l1_review_b_r9_fn_<uuid>()`. It fails with SQL Server error 195, `is not a recognized built-in function name`.
- The procedure insertion `l1_review_b_r9_proc_<uuid>()` fails with error 102, `Incorrect syntax near ')'`.
- Controls `SELECT dbo.[l1_review_b_r9_fn_<uuid>]()` and `EXEC dbo.[l1_review_b_r9_proc_<uuid>]` succeed with the exact values 42 and 43.

The frontend handler was traced in source; I did not claim to click it in a browser. These actual Engine controls establish the runtime consequence of its deterministic output.

**Bounded correction:** Supply schema-qualified, quoted, dialect- and routine-kind-appropriate insertion text from Rust, as with engine-owned table templates, and use it in this action. Alternatively, explicitly make SQL Server routine insertion unavailable with guidance and an honest documented limit. Do not add another frontend SQL generator for this dialect.

**Regression and evidence:** Cover a parameterless SQL Server scalar function and procedure through catalog, insertion and Engine execution, retaining PostgreSQL behavior. Probe and final log: `/tmp/l1data-review-b-r9-routine-probe.rs` and `/tmp/l1data-review-b-r9-routine-probe.log`. Both routines were dropped and `OBJECT_ID` returned NULL for each. An initial version of my temporary harness tried to inspect an older result after a subsequent run had replaced its arena; I corrected the harness to capture rows immediately, removed that initial pair of routines with NULL identity controls, and reran successfully. That harness error is not an application finding.

## Checks, review bounds, and cleanup

I read AGENTS.md, README limitations, architecture load-bearing decisions and reconciliation, the implementation document's final/protocol overrides, and review.md. I examined the full-feature diff and relevant driver/dialect, catalog, capability-refusal, immutable-key, assistant/history, count ownership and frontend caller paths. No additional structural blocker or new security finding was established. In particular, I did not propose reopening SQL Server Manual mode or staged writes.

Independently executed at this pin:

- Six real SQL Server `mssql_counts` integration tests: pass, with disposable-table absence checks. Includes settings/uncommitted rows, native error refresh settlement, Busy/cancel, target refusal, orphaning and caller abandonment. Log: `/tmp/l1data-review-b-r9-count-tests.log`.
- Nine repository Last-page frontend tests: pass. Log: `/tmp/l1data-review-b-r9-existing-last-tests.log`.
- Two temporary frontend tests using the captured real Engine snapshot: pass, reproducing W1.
- Both independent public Engine probes described above, with verified cleanup.
- Vendored archive SHA and exact patch verifier: pass, 96 unchanged upstream files and the seven recorded changed files. Log: `/tmp/l1data-review-b-r9-vendor.log`.
- Final `git diff --check`: pass; `git status --short`: empty; HEAD still matches the pin.

The Rust probes linked exact compiler-artifact paths from a fresh Cargo JSON `--no-run` invocation (`/tmp/l1data-review-b-r9-artifacts.jsonl`), not guessed rlibs. Engine stores used temporary directories and an empty fixed provider path. I did not invoke a real provider, alter seeded rows, edit production source, create commits/PRs, spawn/delegate agents, or restart/change shared services. Only unique disposable SQL objects were created; probe connections and sessions were closed.

This was a bounded review, not a rerun of the entire workspace, TLS matrix, native application or remote CI. Root-owned browser/native acceptance, screenshots and CI remain owner-owned gates. Their pending status is not counted as a code finding or implicitly approved here. Review stopped after this report.
