# Independent review C — R10

**Verdict: request changes.** Findings: 0 critical, 1 warning, 0 nit.

## Warning — routine insertion can cross database kinds

**Location and reachable path:** `src/features/sidebar/ConnectionTree.tsx:402-410` builds `FunctionRow` from a catalog's `FunctionInfo` and, on double-click, inserts `fn.insertText` into whichever workspace tab is active. The row is used by both the ordinary schema tree (`ConnectionTree.tsx:504-505`) and filtered tree (`:806`); those trees can show multiple connections (`:740-742`). The routine driver generates dialect-specific text in `crates/engine/src/session/mssql/catalog.rs:9-16` (for example, `EXEC [dbo].[p]`), while PostgreSQL catalog rows keep `name()` insertion text. `FunctionRow` receives no source `DatabaseRef` or kind and does not compare against the active tab's target kind before calling `insertAtCursor`.

**Observed behavior:** A temporary, source-external Vitest probe set the active editor tab's target to PostgreSQL, invoked the actual exported `FunctionRow` double-click handler with a SQL Server procedure row, and observed `EXEC [dbo].[p]` inserted into that PostgreSQL editor. The route is directly user reachable by expanding a SQL Server connection in the sidebar while a PostgreSQL tab remains active. Running that text uses the active tab's PostgreSQL dialect and fails to parse; the reverse direction also permits PostgreSQL `name()` text to be inserted into a SQL Server tab, where it may fail or resolve a different same-named routine in that database.

**Why it matters:** The saved connection kind is meant to select dialect-specific behavior across the editor and catalog. The routine template now carries its source dialect, but the insertion boundary does not ensure it matches the destination editor. The added `FunctionRow` tests cover verbatim insertion and no-active-tab behavior, but use no active tab with a saved connection kind, so they miss this cross-kind case.

**Bounded fix:** Pass the source database/kind to `FunctionRow` and refuse insertion into an active tab with a different kind, or direct insertion to a tab opened for the routine's database. Cover both ordinary and filtered tree paths with mixed PostgreSQL/SQL Server tabs. Do not silently change the active tab's saved target as part of insertion.

## Checks and limits

- `cargo test -p engine --features mssql-tests --test mssql_counts -- --test-threads=1` — passed, 6/6. The tests exercise native settings/page-count agreement, uncommitted rows, Busy/cancel, abandonment, close/disconnect, target changes, and refresh settlement. Each fixture uses a UUID-named table and cleanup verifies it is absent.
- `cargo test -p engine --features mssql-tests --test routine_insertion sql_server_routine_templates_execute_with_quoted_names_and_structured_arguments -- --exact --nocapture` — passed. The live Engine returned executable scalar, procedure, TVF, typed parameter and TVP templates with bracket-quoted names. Cleanup queried all 11 disposable object identities and each was `NULL`.
- `pnpm exec vitest run src/features/editor/runActions.test.ts src/features/sidebar/FunctionRow.test.tsx src/shell/TopBar.test.tsx` — passed, 61/61. The temporary mixed-kind probe also passed after confirming the mismatched insertion described above; its temporary test/config were removed.
- No shared seeded rows were written, and no shared service was started, stopped, or restarted. SQL Server fixture objects were unique and their absence was asserted by test cleanup.
- I found no additional count ownership/queue, same-kind routine generation, or maintainability finding in this bounded review. This is not an approval of root-owned native UI acceptance or remote CI; those gates remain with the root reviewer.
