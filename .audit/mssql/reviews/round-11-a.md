# Independent SQL Server review A — round 11

Reviewed pin: `f70c2a9dff4d372eebd181e2aa7b4142b39d7565`, against `main`.

**Verdict: approve within the bounded code-review scope below. Findings: 0 critical, 0 warning, 0 nit.**

No actionable correctness, security, or structural finding was established. This is my own verdict, not a panel verdict or release approval. Earlier reviewer approvals and the parent's reported checks are context, not independent evidence for this review.

## Scope and traced paths

I read AGENTS.md, the README limits, architecture load-bearing decisions and relevant current reconciliation, the final capability/protocol overrides in `.audit/mssql/implementation.md`, the historical/current dispositions in `.audit/mssql/review.md`, and all three R10 reports. I inspected the full feature's changed-file inventory and selected surrounding implementation, concentrating on the complete `0812b40..f70c2a9` insertion-boundary delta. This was not an exhaustive re-audit of every line in `main...HEAD`.

- Normal tree: database-scoped catalog lookup → `DatabaseNode` → `SchemaNode` → `FunctionRow`. Filtered tree: `filterTree` constructs each `DatabaseRef` from its connection ID and database row → `FilteredTree` → `FilteredSchemaNode` → the same `FunctionRow`. Both pass the source identity explicitly; neither derives it from a display path or routine name.
- `src/features/sidebar/ConnectionTree.tsx:403-425`: the double-click handler reads the current workspace's active tab, tabs and saved configs. `tabTarget` obtains the destination from that tab. Missing active/target/config and missing source config return through a visible notification before `insertAtCursor`. The PostgreSQL fallback in `databaseKindFor` therefore does not authorize an unresolved source or target. Kind mismatch likewise returns before any editor change. There is no await between validation and insertion.
- Saved connection authority: `Engine::connection_upsert` rejects a kind change for an existing connection before secret writes (`crates/engine/src/lib.rs:712`). The frontend guard uses that saved kind rather than catalog loading state, connection name, default dialect, or server availability. A disconnected same-kind target is still a valid insertion destination under the documented contract.
- `insertAtCursor` consumes the Rust-provided text verbatim and makes only an editor transaction. SQL Server catalog construction quotes schema and routine names separately and uses structured parameter rows; the new guard neither reparses templates nor retargets a tab. Same-kind insertion across different connections/databases remains intentional.
- I also checked the surrounding database-scoped catalog/filter paths, editor state ownership, and count/page stale-text acceptance relevant to inserting into a table tab. The new boundary leaves those paths intact; the focused editor/count tests below cover their existing interactions.

No structural blocker emerged. The fix adds required source identity and one guard in the already-shared row, avoiding duplicated normal/filtered behavior or a new insertion framework. SQL generation stays in Rust. The source-size scan found no existing production Rust/TypeScript file crossing from below 1,000 to at least 1,000 lines in this branch; unchanged upstream vendor files were excluded. This is a bounded maintainability assessment, not a claim that every existing large file is ideal.

## Independently executed checks

1. `pnpm exec vitest run src/features/sidebar/FunctionRow.test.tsx src/features/editor/editorStates.test.ts src/state/databaseIdentity.test.ts src/features/editor/runActions.test.ts src/shell/TopBar.test.tsx` — **PASS, 85 tests across five files**. Log: `/tmp/l1data-review-a-r11-focused.log`. These include both tree paths and mismatch directions, current-target changes, unresolved connections, same-kind templates, unchanged document/cursor/undo state on refusal, and count/page activity behavior.
2. Additional source-external probe against actual `FunctionRow`, Zustand workspace and CodeMirror editor states — **PASS, five cases**. Source/config: `/tmp/l1data-review-a-r11-probe/`; log: `/tmp/l1data-review-a-r11-probe.log`.
   - A handler created while a SQL Server tab is active refuses after the active tab changes to PostgreSQL; both editor-state objects retain exact identity.
   - Saved source/target connection removal after handler creation refuses without touching either editor.
   - Closing every tab before invoking the retained handler leaves tabs and documents unchanged and reports the missing target.
   - Same-kind insertion into another saved connection's table tab works while offline, inserts the exact template at the cursor, preserves the complete tab/table target object, and supports undo back to the original document.

The first extra-probe attempt mistakenly imported a private editor helper and failed in that probe setup. I corrected only the temporary probe to use exported `moveCursor`; the final five cases passed. This was not an application failure. The probes use actual application component/state/editor functions, but do not mount a browser DOM or exercise native IPC.

## Cleanup and limits

No production source was edited. No agents were spawned, no commits or PRs were created, no real providers were called, no database objects or rows were written, and no shared services were changed or restarted. Extra probe files/configuration/logs are retained only under `/tmp`; no database cleanup was needed. Final source verification showed the requested HEAD and only `docs/images/l1-data.png` modified, consistent with the parent-owned screenshot work allowed during review. The earlier status was clean; I did not modify that image or any production source.

I did not rerun the unchanged Engine integration suites, vendor/TLS verification, full frontend suite, typecheck/build, live browser/WKWebView acceptance, screenshots, or remote CI. The parent's supplied results and previous reviewer probes were not counted as checks I executed. No additional Engine probe was needed because I established no candidate Engine defect in this frontend-focused round.

**Root UI/native acceptance, screenshots and CI gates remain owner-owned. Their pending status is not a code finding. Review work is stopped after this report.**
