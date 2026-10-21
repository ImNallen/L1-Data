# Independent SQL Server review B, round 3

Pinned revision `24200b1bab2fdf49293b064d6bced18b2be2c262`. Reviewed the `main...HEAD` feature scope, with focused tracing of the integrated transaction, filter, identity, result and persistence paths. Read AGENTS.md, architecture Load-bearing decisions and reconciliation, README limits, `.audit/mssql/{implementation,verification,review}.md`, and all six requested prior review reports. Prior findings and supplied suite results were context, not approval.

Verdict: request changes. Two warning findings. No critical or nit findings proved.

## Findings

### 1. [warning] Existing PostgreSQL browse tabs with a semicolon or whitespace lose browsing after upgrade

Location: `src/features/editor/runActions.ts:85-86`. Restoration is at `src/boot.ts:48-54`; the page-request guard is at `runActions.ts:134`; controls depend on the predicate at `src/features/results/ResultsPane.tsx:296-319`.

The legacy predicate on `main` compares `textOf(tab.id).trim().replace(/;$/, "")` with the generated template. The new predicate compares the entire document with the cached template exactly. Fixing identifier spelling therefore leaves another supported legacy form incompatible. A saved table tab containing `SELECT * FROM public.orders;`, or the template surrounded by whitespace, was a browsing tab before this change and now becomes a script tab. Boot preserves its saved text. Initial page loading returns without issuing a run, and the pager, filters, header sorting and editing are removed. Manually running the text takes the script path and omits the saved table query/page.

This differs from the dismissed `binary` keyword claim. No identifier or keyword mismatch is needed.

Executed evidence: `/tmp/l1data-mssql-review-b-r3-ui.mjs` imports the actual current frontend modules through Jiti, uses the real CodeMirror editor state and workspace store, and supplies the correct engine template through mocked IPC. It calls the actual `loadTableOnce` for each saved form. `/tmp/l1data-mssql-review-b-r3-ui.log` records:

| Saved text | Legacy predicate | Current predicate | Calls after loading |
|---|---|---|---|
| `SELECT * FROM public.orders` | true | true | `console_configure`, `console_run` |
| `SELECT * FROM public.orders;` | true | false | none |
| Newline, template, newline | true | false | none |
| Template, semicolon, newline | true | false | none |

The tabs carry page 2 and a nonempty saved filter/sort. The control isolates document normalization from template quoting and engine availability.

Bounded fix: retain the legacy outer-whitespace and single-trailing-semicolon normalization for comparing the document with the engine template. This does not require frontend SQL parsing or generation. Add a restoration regression for these already-supported forms, including the actual page request and preserved table query.

### 2. [warning] SQL Server cancellation marks committed Auto-commit work as rolled back

Location: `crates/engine/src/console.rs:189`, consumed by `src/shell/TopBar.tsx:28,76,83`.

`Console::return_session` sets `txn_lost` whenever `session.txn_depth().is_some()`. Every SQL Server session returns `Some(depth)`, including `Some(0)`. Thus retirement of an idle Auto-commit session sets `txnLost=true`. The toolbar interprets this as a definite rollback, with title `Transaction rolled back: connection lost` and visible text `Auto-commit · rolled back`.

This assertion is false when an earlier Auto-commit batch committed before a later batch was cancelled. It is also stronger than what is known when an in-flight native transaction command loses its response. The implementation intentionally retires SQL Server clients and documents that earlier committed batches remain committed. The status must preserve that distinction so a user does not repeat an already-committed write after being told it rolled back.

Executed evidence: `/tmp/l1data-mssql-review-b-r3-cancel.py` used the already-running real Engine HTTP bridge. A separate observer console owned a uniquely named global temporary table in `tempdb`. An Auto-commit actor ran:

```sql
UPDATE [unique_disposable_table] SET n=2
GO
WAITFOR DELAY '00:00:30'
```

The observer first confirmed the value was `2`. Before cancellation the actor reported `txn=Idle`, `txnDepth=0`, and a running script. After cancellation it reported `outcome=Cancelled`, a Lost session, and `txnLost=true`. The observer still read `2`. `/tmp/l1data-mssql-review-b-r3-cancel.log` records the states and persisted value. Source tracing establishes the toolbar text from that exact returned flag; this was not a browser/native rendering test.

Bounded fix: do not use presence of SQL Server depth as proof that a transaction rolled back. Preserve an unknown outcome when necessary, and render SQL Server retirement as a session reset or transaction loss rather than guaranteed rollback. Keep the existing PostgreSQL known-rollback message. A regression should cover an Auto-commit write followed by cancellation, and an uncertain in-flight native transaction separately. Simply checking the last depth for zero cannot establish what an unobserved in-flight batch did.

## Independent verification

- `cargo test -p engine --features mssql-tests --test mssql_transactions -- --nocapture` passed all seven cases. Log: `/tmp/l1data-mssql-review-b-r3-transactions.log`. These exercise whole-selection refusal, scope/nesting, rollbackable module definitions, conditional/procedure controls, inherited options, errors and cancellation. A pass is not evidence for transaction forms outside the tested subset.
- Recompiled the earlier B Engine filter probe against the current existing `libengine-72507556f0ff8bc0.rlib` and its matching Tokio artifact. The source and output are `/tmp/l1data-mssql-review-b-r3-engine.rs` and `/tmp/l1data-mssql-review-b-r3-engine.log`. It uses isolated SQLite storage, MemoryKeyStore, and an empty fixed provider path. Actual text/ntext/XML Eq pages and counts now return the selected row and count 1. Canonical datetime Contains/StartsWith/Eq and binary Eq also return that row and count 1. The control query returns 1.
- The new cancellation probe used only unique disposable rows, and its separate observer kept the table alive across actor retirement. Cleanup succeeded and `sys.tables` returned zero matching objects. Both temporary bridge consoles were closed. The isolated Engine probe also dropped its table, verified zero remaining objects, shut down, and removed its data directory.
- Traced transaction planning before run mutation; moved-out session ownership; native state probing, cancellation and retirement; type-aware conversion; database-scoped catalogs and the configured-home alias; generated-page provenance; history/connection/chat compatibility; SQL Server assistant refusal at policy and execution; and the relevant PostgreSQL/frontend changes. No additional structural blocker was established. Type System Discipline informed the finding that one loss flag collapses known rollback and unknown SQL Server outcomes.
- A separate optional static toolbar-render experiment did not establish UI evidence and is not relied upon. The toolbar consequence above is proved by the returned flag and its direct JSX consumers.

## Limits and gates

This was a roughly ten-minute read-only review, not exhaustive execution of the full feature matrix. I did not independently rerun all workspace/PG/MSSQL/frontend suites, bridge smokes, native selfchecks, real provider tests, or browser/native acceptance. I did not run remote CI. The supplied broader passes remain context. Final app acceptance, refreshed decision/verification records, and remote CI remain owner gates; the audit records still describe second-round work as pending.

No repository source edits, commits, PR operations, subagents, service launches/restarts, real provider-account calls, saved connection edits, or seeded-row writes were performed. Only disposable unique SQL Server objects/rows and temporary review artifacts were created. HEAD remained the pinned revision and `git status --short` was empty at the final source check. Documented refusals and the disproved legacy keyword premise are not findings.
