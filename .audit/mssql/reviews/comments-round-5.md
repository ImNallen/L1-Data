# SQL Server comment review, round 5

Reviewed `b81b4e4..98301df8f4d2c742931875f6e9e297f464989c9d` in `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d` as one comment-sicko reviewer. Read `AGENTS.md` and the complete `poteto-mode/references/agents/comment-sicko.md`. No parent playbook or agents were run.

## Result

- Exact deletions: none (0 comments, 0 lines).
- Exact keeps: 1 changed public API doc-comment line, below.
- MUST KILL: none.
- New or changed suppressions: none.
- Checkout files touched: none. Only this report was written; no patch is needed.
- Dirty source: none; `git status --short` was empty at inspection.

## Keep and proof

Keep `crates/engine/src/lib.rs:1140`, on `Engine::table_apply`:

```rust
    /// Applies PostgreSQL staged table edits on the tab's own session, all or nothing. With no transaction open, auto-commit
```

Exception: doc comment defining a public API contract. The changed word is `PostgreSQL`; the rest of this line and the continuation at lines 1141–1142 predate this range. This explicitly narrows the method's supported database kind. Static proof in the current implementation: lines 1148–1150 refuse SQL Server before connection/catalog work; lines 1172–1176 recheck the current configuration and session kind before taking the session. Lines 1196–1200 select PostgreSQL transaction/savepoint statements. This is a public method contract, not a workaround sermon or an external-platform exception claim. Unchanged transaction guarantees were not reopened under this narrow review.

No uncertain foreign-dependency claim needed a how/why investigation. No runtime proof or tests were run, as requested; proof here is inspection of the actual diff and current method.

## Audited source paths

Compared added/modified comment lines in every non-document, non-generated source path in the range. Only the keep above was present. The typed unknown-outcome, immutable named-key, Engine refusal, removed SQL Server managed-write implementation, and UI capability changes add no other source comments.

- `crates/engine/src/lib.rs`
- `crates/engine/src/model.rs`
- `crates/engine/src/session.rs`
- `crates/engine/src/session/mssql.rs`
- `crates/engine/src/session/mssql/edits.rs` (deleted)
- `crates/engine/src/sql/mssql_value.rs`
- `crates/engine/src/sql/tsql.rs`
- `crates/engine/src/sql/tsql/transaction.rs` (deleted)
- `crates/engine/src/txn.rs`
- `dev/mssql-smoke.py`
- `src/features/editor/runActions.ts`
- `src/features/results/ResultGrid.tsx`
- `src/features/results/ResultsPane.tsx`
- `src/ipc/errors.ts`
- `src/shell/TopBar.tsx`
- `src/state/tableEdits.ts`

## Suppressions

Scanned changed surviving source files, including tests, for ESLint disable directives, TypeScript ignore/expect-error/nocheck directives, Prettier ignores, Rust allow/expect attributes, rustfmt skips, Python noqa/type-ignore/nosec directives, coverage ignores, and inspection/suppression annotations. No matches were present. Thus no suppression survives by exception, no rule lookup is required, and no suppression-driven MUST KILL target exists. Rust derive/cfg/test attributes and test `.expect(...)` calls are not suppressions.

## Skips

- Unchanged comments and earlier feature comments, including unchanged lines 1141–1142 of the public API block.
- `README.md`, `docs/architecture.md`, and generated `src/ipc/bindings.ts`.
- Test commentary and implementation except suppression scanning: `crates/engine/tests/mssql.rs`, `crates/engine/tests/mssql_edit_drift.rs` (deleted), `crates/engine/tests/mssql_transactions.rs`, `crates/engine/tests/mssql_values.rs`, `crates/engine/tests/pg_key_identity.rs`, `src/features/editor/runActions.test.ts`, `src/features/results/ResultGrid.test.tsx`, `src/ipc/errors.test.ts`, `src/state/tableEdits.test.ts`, and inline engine test modules.
- Fixture SQL literals; protocol text, URLs, user-visible UI messages, and error strings are not source comments.
- Already deleted comments in removed implementations: no remaining deletion to recommend.
- Application correctness, broader design review, edits, commits, PRs, providers, services, test execution, and independent review panels remain outside this assignment.
