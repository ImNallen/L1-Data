# Independent review C — R11

Reviewed pin: `f70c2a9dff4d372eebd181e2aa7b4142b39d7565` (`main...HEAD`).

**Verdict: no findings in this bounded review. Counts: 0 critical, 0 warning, 0 nit.** This is my own result, not a parent-panel verdict, release approval, or a substitute for the owner-owned UI and CI gates.

## Scope and trace

I read the repository instructions, README capability limits, architecture load-bearing decisions and current reconciliation, `.audit/mssql/implementation.md`, `.audit/mssql/review.md`, and the R10 reports. I concentrated on the accepted R10 routine-insertion finding and its interactions with saved connection kind, the active tab, both sidebar tree paths, and the preceding catalog-template change.

The reachable path is `ConnectionTree` / filtered-tree catalog rows → `SchemaNode` or `FilteredSchemaNode` → `FunctionRow` → `Row.onDoubleClick` → `insertAtCursor(active, fn.insertText)`. Both tree paths pass the source `DatabaseRef`. At click time, the handler reads the current workspace active tab, its current target, and current saved configs. It refuses a missing tab/target/config or missing source config before insertion, then compares source and destination kinds. Existing saved config kinds are immutable in Engine; the legacy PostgreSQL fallback is therefore used only for an existing config without a kind field. A kind mismatch produces a toast and leaves the editor path untouched. Same-kind insertion remains verbatim and can target another database or connection of that same kind, as documented.

I found no reachable mismatch in this boundary or regression in the documented same-kind behavior. The additional exports of `SchemaNode` and `FilteredSchemaNode` let the tests inspect the real prop wiring into `FunctionRow`; they do not add runtime behavior.

## Independent check

- `pnpm exec vitest run src/features/sidebar/FunctionRow.test.tsx` — passed, 20/20 tests. The tests exercise actual row double-click handlers and editor state for both mismatch directions, both tree paths, removed or unresolved source/target, a target changed after row creation, and same-kind insertions across databases/connections. Refusal assertions cover document text, selection, other-tab text, workspace target, undo history, and the visible toast.
- I did not run an Engine/database probe: the reviewed correction is a synchronous frontend insertion boundary and changes no Engine or SQL execution path. No database objects or seeded rows were touched, and no service lifecycle was changed.
- HEAD remained pinned at the requested revision; the review did not edit tracked source, commit, open a PR, call providers, or delegate work.

I did not independently run a browser/WKWebView acceptance flow, native bundle, screenshot, full suites, or remote CI. **Those UI/native acceptance and CI gates remain owner-owned and are not claimed as passed here.**
