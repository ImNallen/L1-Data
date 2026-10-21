SQL Server support crosses the saved connection contract, editor services, SQL previews, history, and test infrastructure. There is currently no database-engine or dialect discriminator. CodeMirror already includes `MSSQL`; the larger work is carrying the correct dialect through every database-specific path.

Read-only exploration completed. No edits, subagents, PR, or tests run.

1. Saved connections and compatibility.

   - [`ConnectionConfig` and `ConnectionDraft`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/model.rs:40) contain host, port, database, user, PostgreSQL `SslMode`, CA path, password state, and color. Neither identifies the database engine.
   - [`Engine::save_connection`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/lib.rs:655) validates fields, handles the encrypted password, constructs `ConnectionConfig`, and persists it.
   - [`Store::connections`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/store.rs:134) deserializes every saved connection JSON and collects into one `Result`. A newly required field without a fallback fails the entire collection. [`Engine::boot`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/lib.rs:635) then logs the failure and returns an empty connection list.
   - Minimum contract change is a typed database-engine discriminator on configuration and draft. Existing saved connections must deserialize as PostgreSQL. Follow the hand-written fallback pattern in [`Layout::deserialize`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/model.rs:983), keeping the exported field required. This follows the Type System Discipline principle.
   - [`DatabaseRef`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/model.rs:89) already identifies connection plus database. Tabs store that target, so vendor-appropriate dialect selection need not add another independently mutable tab field.

2. Connection form flow.

   [`initialFields`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/connections/connectionForm.ts:10) supplies PostgreSQL defaults. [`ConnectionForm`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/connections/ConnectionDialog.tsx:192) holds form state and follows this flow:

   - URL edits call `connectionParseUrl`, discard stale responses, then copy parsed fields and password into the form.
   - Test builds a `ConnectionDraft` and calls `connectionTest`.
   - Save calls `connectionSave`, updates the workspace configuration, and automatically connects newly created connections.

   Minimum frontend changes are an engine selector, engine-specific defaults, draft/URL propagation, dirty-state coverage, TLS/authentication controls matching the backend contract, and engine-aware success text. The current URL placeholder and “Connected to PostgreSQL” message are hard-coded. CA paths are preserved from existing configuration but have no editable form control. See [`parseUrl` and `draft`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/connections/ConnectionDialog.tsx:228).

   URL parsing is entirely PostgreSQL-specific through `tokio_postgres::Config::from_str` in [`draft_from_url`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/connection.rs:100). An engine selection must either reach that command or be inferred unambiguously from the supported connection-string formats.

3. Editor, completion, highlights, cards, and history.

   - [`SqlEditor`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/editor/SqlEditor.tsx:7) resolves the tab’s `DatabaseRef`, retrieves its catalog, and calls `configureSql`. [`sqlLanguage`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/editor/editorStates.ts:173) hard-codes PostgreSQL twice, for syntax/schema completion and custom qualified-table completion. Pass the resolved dialect independently of catalog availability and reconfigure when the target or engine changes. The existing `Compartment` preserves each tab’s document and undo history.
   - [`buildCatalogView`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/state/catalog.ts:20) builds schema/table/column completion and explicit qualified insertion text. That text uses PostgreSQL [`sqlIdent`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/editor/sqlIdent.ts:12), including PostgreSQL reserved words and lowercase rules. Changing only CodeMirror’s dialect leaves this insertion path unchanged.
   - [`SqlHighlight.sqlSpans`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/assistant/SqlHighlight.tsx:9) directly uses the PostgreSQL parser. It needs a dialect argument and matching memo dependencies.
   - [`SqlCard`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/assistant/SqlCard.tsx:11) already receives a `DatabaseRef`. [`Transcript.Turn`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/assistant/Transcript.tsx:92) derives that target from the historical user entry, which is the correct source for that turn.
   - [`HistorySql`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/history/HistorySql.tsx:7) creates a separate read-only PostgreSQL editor and accepts only text. [`HistoryPalette`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/history/HistoryPalette.tsx:183) must supply the saved query’s dialect.
   - History opening already preserves the recorded database through [`openHistoryQuery`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/history/historyActions.ts:6). Inserting history or assistant SQL intentionally inserts into the active tab, whose dialect may differ.
   - The installed CodeMirror package exports `MSSQL` and configures bracket identifiers and `@` variables. No new editor-language dependency is required. See [`MSSQL`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/node_modules/@codemirror/lang-sql/dist/index.js:835).

4. Minimum IPC changes and hidden coupling.

   - `script_outline(text, cursor)` and `editor_format(text, target)` currently have no connection, database, or dialect input. Their engine methods call global SQL functions. Add dialect context to both, and use the same resolved dialect during execution. Update [`ipc.rs`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src-tauri/src/ipc.rs:100), [`dispatch.rs`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/dev-bridge/src/dispatch.rs:116), and generated bindings together.
   - Frontend outline callers are the statement chooser and run-target marker. Formatting is another caller. Their stale-response checks compare document, cursor, and active tab but do not compare database target. A dialect change without a text change must invalidate pending work. See [`runOrChoose`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/editor/runActions.ts:150) and [`markOnce`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/editor/runActions.ts:426).
   - [`browseQuery` and `isBrowsing`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/editor/runActions.ts:67) generate PostgreSQL identifier text and compare it with the editor document. That comparison selects structured `TablePage` execution rather than script execution. Changing quoting can silently change tab behavior. Backend-generated preview text would remove this duplication while respecting AGENTS.md.
   - [`parseOrderBy`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/features/results/tableQuery.ts:31) accepts double-quoted identifiers, lowercases unquoted names, and cannot parse bracket identifiers. `formatOrderBy` also uses PostgreSQL quoting. This affects grid-header sorting, not just display.
   - History survives connection deletion. Its SQLite rows currently have no dialect, and [`HistoryGroupSummary`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/model.rs:813) carries only connection/database identity. Persist the dialect with executions, default existing rows to PostgreSQL, and expose it in history responses. Grouping currently uses SQL, connection ID, and database; include dialect if an existing connection may change engine. Write classification and title/excerpt generation also call global SQL functions in [`store.rs`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/store.rs:216) and [`history.rs`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/history.rs:43).
   - Historical chat context similarly lacks dialect. Adding it to persisted [`SentContext`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/model.rs:1695) requires backward-compatible deserialization. Otherwise old transcript entries can become `Unreadable`.
   - Assistant SQL fences recognize only `sql`, `postgresql`, and `pgsql`. Relation extraction and vetting are global PostgreSQL logic in [`reply.rs`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/assistant/reply.rs:7). The [`system prompt`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/assistant/system_prompt.md:1) also identifies PostgreSQL and recommends `LIMIT`.
   - Engine changes on existing connections need an explicit lifecycle rule. Saving configuration does not reset console sessions, and [`console_configure`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/lib.rs:943) detects target changes only by connection ID/database. Inference: changing the engine under the same ID could make frontend dialect and an already-open session disagree.
   - [`PgBadge`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/ui/primitives.tsx:15) is reused by the connection dialog, sidebar, and history. All need engine-aware labels.

5. Bridge setup and relevant verification.

   [`pnpm bridge`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/package.json:10) starts the real engine HTTP bridge on `7077` and Vite on `5173`. [`bridgeShim.ts`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/src/dev/bridgeShim.ts:26) forwards generated IPC calls and SSE events unchanged. The bridge uses a temporary data directory unless `L1_DATA_DATA_DIR` is set and always uses `MemoryKeyStore`. See [`main.rs`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/dev-bridge/src/main.rs:84).

   The current Docker service is PostgreSQL 16 only. [`bridge-smoke.sh`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/dev/bridge-smoke.sh:28) accepts only PostgreSQL URLs and assumes seeded PostgreSQL databases, schemas, notices, and result data. Add SQL Server provisioning, seed data, and a dedicated engine/bridge integration path.

   Existing regression anchors are:

   - Connection form defaults and dirty state in `connectionForm.test.ts`.
   - Alias/default-schema completion in `editorStates.test.ts`.
   - Qualified insertion and source-column identity in `state/catalog.test.ts`.
   - Browse detection, formatting undo, and database switching in `runActions.test.ts`.
   - Static SQL highlighting and vetted-card actions in `SqlCard.test.tsx`.
   - Recorded-database execution and deleted connections in `historyActions.test.ts`.
   - Persistence reopening in `store.rs`; legacy workspace compatibility in `model.rs`.
   - History grouping, paging, and migration backfill in `history.rs`.
   - [`dispatch_serves_exactly_the_tauri_commands`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/dev-bridge/src/dispatch.rs:173), bindings drift, and the desktop sync-command runtime check in `src-tauri/src/main.rs`.

   Add mixed PostgreSQL/SQL Server tab tests, dialect switching before catalog load, bracket-name completion/sorting, historical rendering after deletion, and target changes during outline/format requests. AGENTS.md requires real UI exercise through the bridge, plus desktop checks for changed sync commands.

6. Boundaries and remaining questions.

   Full support also requires backend work beyond this frontend slice. [`Session`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/session.rs:13) directly owns a PostgreSQL client, cancel token, TLS connector, notices, and PostgreSQL session initialization. Execution splits/classifies globally and builds table SQL in [`console_run`](/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d/crates/engine/src/lib.rs:966). The architecture’s text-value, session, SQL parsing, and catalog decisions must be reconciled in the same feature.

   “Dialect per connected database” has two possible scopes. Vendor-appropriate dialect per tab can derive from its existing connection/database target. An independently configurable dialect override for each database needs an additional persisted mapping and resolved IPC field. No such override currently exists. SQL Server authentication breadth, batch handling such as `GO`, and compatibility-level behavior were not settled by this exploration.

Files inspected included the requested skills and AGENTS.md; architecture and package configuration; engine `model`, `store`, `view`, `connection`, `session`, `text`, `history`, `lib`, assistant context/reply/prompt, and PostgreSQL tests; Tauri IPC/main; bridge main/dispatch/shim/smoke/Docker setup; frontend workspace/catalog, connection form/actions/dialog, editor state/run/theme/identifier helpers, assistant actions/transcript/cards/highlighter, history actions/viewer/palette, table-query helpers, shared badges, fixtures, and the relevant tests listed above.
