# Independent review B — round 11

Reviewed source: `f70c2a9dff4d372eebd181e2aa7b4142b39d7565`, with `main...HEAD` as feature context and concentration on `0812b40..HEAD` (8435836/f70c2a9).

**Verdict: approve within this bounded independent code review. No findings: 0 critical, 0 warning, 0 nit.** This is my own fresh verdict at this revision. It does not reuse a prior approval, claim other reviewers' results, or approve release readiness.

## Scope and reasoning

I read AGENTS.md, the README limits, architecture load-bearing decisions and relevant reconciliation, the final capability/protocol overrides in `.audit/mssql/implementation.md`, the review decision history, and all three R10 reports. The narrowed SQL Server capability contract is accepted. I inspected the full feature's changed-file inventory and traced the insertion fix through its surrounding shared feature paths; this is not a claim of exhaustive re-review of every main...HEAD line. Only README, architecture, ConnectionTree and its FunctionRow tests changed after 0812b40. Engine, generated IPC contracts, count ownership and queue production code are unchanged by this insertion correction.

The ordinary route is `ConnectionNode` → `DatabaseNode` → `SchemaNode` → `FunctionRow`; the filtered route is `filterTree` → `FilteredTree` → `FilteredSchemaNode` → that same `FunctionRow`. Both now pass the actual source `DatabaseRef` independently of display paths. I checked the source's propagation, not just direct row invocation.

At `src/features/sidebar/ConnectionTree.tsx:403`, the double-click handler reads the current active tab, tab map and saved configs. `tabTarget` resolves that tab's console target, including table tabs. Missing active tabs, null targets and missing saved destination/source configs refuse before `insertAtCursor`. Both config existence checks precede `databaseKindFor`, so its legacy PostgreSQL fallback does not authorize an unresolved connection here. The kinds come from saved connection configs; the engine's save boundary in `crates/engine/src/lib.rs:712` refuses changing an existing connection's kind. The handler has no await between validation and the editor operation.

Successful insertion still consumes the Rust catalog's `insertText` verbatim. The SQL Server private catalog builds separately quoted schema/routine names and structured argument placeholders; the PostgreSQL catalog preserves its existing insertion text. The guard neither constructs SQL nor retargets a tab. Same-kind insertion across saved connections/databases remains intentional and tested.

No actionable structural regression emerged. One required `DatabaseRef` reuses the existing domain model, and one handler owns the boundary check for both callers. It adds no mirrored kind state, parser, asynchronous authorization sequence, new abstraction or compatibility lane. ConnectionTree remains below the rubric's file-size threshold at 897 lines. The two schema exports provide direct test access without replacing the production row wiring or adding hook mocks.

## Independently executed checks

- `pnpm exec vitest run src/features/sidebar/FunctionRow.test.tsx src/features/sidebar/sidebarSearch.test.tsx src/features/sidebar/treeFilter.test.ts src/features/editor/runActions.test.ts src/state/workspace.test.ts` — **PASS, 81 tests across 5 files**. Evidence: `/tmp/l1data-review-b-r11-context.log`. The actual row/editor checks cover both cross-kind directions, both tree callers, unavailable targets/sources, same-kind templates, separate connections/databases, cursor placement, existing undo history, and unchanged other editor/target state on refusal. The neighboring runActions checks passed; this does not represent a new live count/engine acceptance run.
- Additional source-external Vitest probe calling the actual FunctionRow handler and real workspace/editor stores — **PASS, 5 tests**. Evidence: `/tmp/l1data-review-b-r11-probe.log`; source `/tmp/l1data-review-b-r11-probe.test.tsx`, config `/tmp/l1data-review-b-r11-probe.config.mts`. A row created before an active-tab switch inserts only into the newly active matching tab; switching from a matching to a different-kind tab refuses. Removing a destination config after rendering refuses without changing either EditorState object. Removing the source/current connection after rendering also refuses. A table tab with a mismatched console kind keeps the identical EditorState and tabs-map objects. Refusals produce a toast. These probes add no application mocks or SQL execution.
- An initial focused run of FunctionRow and workspace tests also passed, 23 tests across 2 files; it overlaps the 81-test run and is not counted as additional coverage. Evidence: `/tmp/l1data-review-b-r11-focused.log`.
- `git diff 0812b40..HEAD --check` — **PASS**. Final source check retained the requested HEAD. The only observed working-tree change was `docs/images/l1-data.png`, within the parent-authorized screenshot work; this review did not alter it. Production source remained pinned.

## Cleanup and limits

The temporary frontend probe resets the workspace and drops both disposable editor states after every case. Its files and logs remain only under `/tmp` as review evidence. I changed no source, seeded data or database objects, invoked no providers or agents, made no commits/PRs, and started/stopped/restarted no services.

No new live Engine/database probe was needed or run for this synchronous frontend insertion boundary; no candidate finding requiring database reproduction emerged. I did not rerun full Rust suites, TLS, builds, actual browser/WKWebView interactions, screenshots or remote CI, and I do not independently attest the parent's supplied results for those gates. **Root UI/native acceptance and CI gates remain owner-owned.** Their pending status is not a code finding.

I have stopped review work after writing this report.
