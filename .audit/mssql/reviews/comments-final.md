# Comment review R4 — 24200b1..2a21153

Repository: `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`. Reviewed the specified commit range at HEAD `2a21153`, following `comment-sicko.md` read in full. Scope: newly added source comments and suppressions, including tests and any changed dev scripts.

**Verdict: 0 deletions; 2 keeps; 0 MUST KILL flags; 0 added suppressions.** No deletion patch is needed. No repository files were edited.

## Keeps with exact proof

1. **`crates/engine/src/model.rs:563` — `TableScript::accepted_texts`**

   `/// Browse documents after trimming outer whitespace. Includes the canonical text.`

   Keep under the public API contract exception. This field defines how consumers recognize a browse document across IPC: trim its outer whitespace and compare it against the engine-provided accepted strings, which include `text`. The contract is observable across the Rust/frontend boundary, rather than narration of a private implementation.

   Proof at the reviewed revision:

   - `crates/engine/src/text.rs:421-424` builds canonical `text`, initializes `accepted_texts` with its clone, and adds only the PostgreSQL semicolon form.
   - `crates/engine/src/text.rs:232-246`, `postgres_table_templates_keep_saved_browse_tabs_recognizable`, asserts the canonical text and exact accepted forms for PostgreSQL and SQL Server.
   - `src/features/editor/runActions.ts:87-88`, `isBrowsing`, performs `acceptedTexts.includes(textOf(tab.id).trim())`.
   - `src/features/editor/runActions.test.ts:222-253` covers canonical/semicolon forms with outer spaces, tabs and newlines while preserving restored page/filter/order state; lines 276-287 cover arbitrary engine-provided forms and reject an extra semicolon the engine did not accept.

2. **`crates/engine/src/session/mssql/edits.rs:84` — `MsSession::prepare_edits`**

   `// A real scan takes the object lock even for an empty table. Sch-M cannot pass it before the writes finish.`

   Keep under the external vendor behavior exception. Empty-table lock acquisition and SQL Server's schema-modification lock compatibility are foreign behavior; names or Rust types cannot establish those guarantees. The comment explains why this particular scan is necessary before inspecting live metadata and issuing mutations.

   Proof at the reviewed revision:

   - `crates/engine/src/session/mssql/edits.rs:85` executes `SELECT TOP (1) 1 ... WITH (TABLOCKX, HOLDLOCK)` before the live metadata checks and before returning the edit plan.
   - `crates/engine/src/lib.rs:1178-1184` opens the edit transaction/savepoint before `prepare_edits`; lines 1191 and 1212-1213 execute edits before the optional commit. `crates/engine/src/session.rs:246-251` keeps an existing SQL Server transaction open with a savepoint, and commits a newly opened transaction only in AutoCommit mode.
   - `crates/engine/tests/mssql_edit_drift.rs:469-486`, `an_empty_table_is_locked_until_its_manual_transaction_finishes`, uses the real engine and SQL Server sessions. It asserts zero rows, queries `sys.dm_tran_locks` for the editor PID/database/object and a **granted OBJECT X lock** (477-478), requires concurrent ALTER TABLE to fail specifically with lock timeout **1222** (479-482), then proves the ALTER succeeds after rollback (483-485).
   - `crates/engine/tests/mssql_edit_drift.rs:490-517`, `concurrent_ddl_waits_for_the_checked_edit_and_auto_commit_releases_the_lock`, applies an actual manual edit (496), observes the competing DDL PID blocked by the editor PID with **`LCK_M_SCH_M`** (500-507), commits and requires successful DDL completion plus the edited value (508-511). It then applies an AutoCommit edit and proves subsequent ALTER succeeds under a 150 ms lock timeout (513-516).
   - `crates/engine/tests/mssql_edit_drift.rs:363-390` and `394-420` additionally exercise conversion/schema refusal inside transactions, nonrollbackable sequence sentinels proving no mutation was attempted, and preservation of prior outer-transaction work.

   These are live-database integration proof paths, not mocked lock assertions. This read-only review inspected their implementation; it did not rerun them or observe the parent's current test results. It makes no new live-pass claim.

## MUST KILL flags

None. Neither added comment conceals an internal-code surprise or an unproven workaround requiring a rename, extraction, type change or redesign.

## Suppressions audit

Scanned every added source line for Rust `allow`/`expect`/`ignore` attributes, ESLint disables, TypeScript ignore/expect-error/nocheck directives, formatter ignores, coverage ignores, `noqa`, `nolint` and `noinspection`. **None added.** No suppressed rule requires investigation. The new test module's `#![cfg(feature = "mssql-tests")]` is a feature gate, not a lint suppression. SQL fixture `SET ANSI_WARNINGS OFF` is intentional database test input, not a source-tool suppression.

## Reviewed files

Comment-bearing source files: `crates/engine/src/model.rs`, `crates/engine/src/session/mssql/edits.rs`.

Other changed source/test files scanned; no added source comments or suppressions:

- `crates/engine/src/console.rs`
- `crates/engine/src/lib.rs`
- `crates/engine/src/session.rs`
- `crates/engine/src/session/mssql.rs`
- `crates/engine/src/session/mssql/catalog.rs`
- `crates/engine/src/sql/mssql_value.rs`
- `crates/engine/src/text.rs`
- `crates/engine/tests/mssql.rs`
- `crates/engine/tests/mssql_edit_drift.rs`
- `crates/engine/tests/mssql_transactions.rs`
- `crates/engine/tests/pg.rs`
- `src-tauri/src/ipc.rs`
- `src/features/editor/runActions.test.ts`
- `src/features/editor/runActions.ts`
- `src/features/history/historyActions.test.ts`
- `src/shell/TopBar.test.tsx`
- `src/shell/TopBar.tsx`
- `src/state/databaseIdentity.test.ts`

No dev scripts changed in this range. Report artifact only: `/tmp/l1data-mssql-comments-r4.md`.

## Skips

- Documentation: `README.md`, `docs/architecture.md`.
- Generated bindings: `src/ipc/bindings.ts`, including the generated copy of the retained `TableScript` doc comment.
- SQL fixture text, including `src/features/editor/runActions.test.ts:258` (`-- changed` inside a test SQL string).
- Unchanged, previously audited comments, including `loadTableOnce`'s existing explanation in `src/features/editor/runActions.ts:294` and existing comments adjacent to the loss-state changes.
- General behavior/style review beyond deciding comment and suppression exceptions. No code edits, commits, subagents, services or PR operations were performed.
