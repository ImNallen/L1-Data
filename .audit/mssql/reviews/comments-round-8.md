# Round 8 comment audit

The pinned production additions contain no comments or suppressions. Proposed deletion count is 0 comment blocks and 0 comment lines across 0 source files. There are no MUST KILL flags.

## Scope and method

- Worktree reviewed at `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`.
- Base is `8b78498cf55bbc8710194daaac48e472621d75aa`.
- Head is `c4e6fa4bfaf5409274fb0f7d077d33e02488dc56`.
- Read the repository `AGENTS.md`, global model overrides, `poteto-mode/SKILL.md`, its Codex mapping, `references/agents/comment-sicko.md`, and `no-comments/SKILL.md` in full.
- Inspected `git diff --no-ext-diff --unified=12 <base> <head>` and nearby source with `git show <head>:<path>`. Working-tree content did not supply review evidence.
- Reviewed every added production line. A second line-number scan of the zero-context pinned diff excluded inline test modules and checked comment delimiters and suppression markers.
- Applied Prove It Works by checking the pinned diff and source directly, then reading back this report and checking the patch size. This is a static comment audit, with no runtime correctness verdict.

## Files and deletion counts

Line numbers refer to the pinned head. Added-line counts include blank lines and closing braces.

| Production file | Scope | Added lines reviewed | Comment blocks deleted | Comment lines deleted |
| --- | --- | ---: | ---: | ---: |
| `crates/engine/src/assistant/system_prompt_mssql.md` | Entire runtime prompt, lines 1-13 | 13 | 0 | 0 |
| `crates/engine/src/assistant/turn.rs` | Production additions to `system_prompt`, lines 676-680 | 5 | 0 | 0 |
| `crates/engine/src/lib.rs` | Additions to `Shared::schedule_refreshes` and `Shared::schedule_slot_catalog_refresh`, lines 408-416 and 425-428 | 13 | 0 | 0 |
| `crates/engine/src/sql/tsql.rs` | `selects_into`, lines 370-387, and its call in `invalidates`, line 395 | 19 | 0 | 0 |
| Total | | 50 | 0 | 0 |

No application file was edited.

## Deletion patch

`/tmp/l1data-mssql-comments-r8.patch` is an empty, zero-byte patch. It contains no hunks because the scoped additions contain nothing eligible for comment deletion. No patch needs application.

## Keeps proof

Kept comment blocks are 0. No legal, public-contract, foreign-platform, issue-link, or suppression exception was needed.

The prose in `system_prompt_mssql.md` is executable assistant input, rather than source commentary. At the pinned head, `turn.rs:677` loads it through `include_str!`, and `turn.rs:250` obtains the database-specific prompt through `system_prompt`. Keeping this runtime content requires no comment exception. The SQL Server read-only statement in that prompt was not independently validated against a database in this audit.

No unchanged comment was reconsidered or counted as a keep.

## MUST KILL flags

None. There is no scoped added comment describing an internal workaround or code surprise, and no scoped added correctness or safety suppression. No refactoring target is invented from uncommented implementation code.

## Suppressions

No additions contain `eslint-disable`, `@ts-ignore`, `@ts-expect-error`, `@ts-nocheck`, `prettier-ignore`, Rust `#[allow(...)]` or `#[expect(...)]`, `noqa`, or `nolint`. No rule lookup, suppression deletion, or encoding offer is needed.

## Skips and limits

- Unchanged comments, public and protocol comments, and suppressions audited in earlier rounds were skipped.
- `crates/engine/src/assistant/mod.rs` has only test deletions in the pinned diff and no production additions.
- The added inline test modules and cases in `turn.rs` and `tsql.rs` were excluded. That excludes 60 and 26 added lines respectively, including the SQL string containing `/* INTO dbo.t */`.
- The integration-test changes in `mssql_cross_database_refresh.rs`, `mssql_opaque_refresh.rs`, and `mssql_refresh_outcomes.rs` were skipped.
- Documentation, generated bindings, and upstream vendor code were excluded. The runtime prompt was reviewed because it is explicitly in scope, despite its Markdown extension.
- The pre-existing working-tree modification to `docs/architecture.md` was not reviewed, read as evidence, or edited.
- No source edits, commits, subagents, service operations, database operations, real-provider calls, or PR operations occurred.
- The caller explicitly limited this role to reporting. The parent owns acceptance, any future source edits, and fresh review. No restoration, rerun, architecture sketch, implementation fix, or unenforced comment constraint remains from this pass.
