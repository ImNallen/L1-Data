# Comment-sicko review, round 6

Scope: `98301df` through current tracked working tree at `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`. Read-only source review; only this report was written. No application edits, commits, providers, services, databases, or agents were run.

## Verdict and counts

Delete **2 comment/docstring blocks, 2 lines, in 2 files**:

- `crates/engine/src/console.rs:272`: own-code lifetime/refresh coordination disguised as a prose requirement.
- `vendor/tiberius/verify-upstream.py:2`: module narration repeats the script name, checksum check, and patch comparison. It defines no public callable API contract. Keep the executable shebang.

Keep **8 blocks, 13 lines**: one engine public contract, one external SQL Server behavior, and six vendored public API contracts. No new lint/type/correctness suppressions found in the scoped additions. Ordinary `.expect(...)` calls are not suppressions.

## MUST KILL

- `Console::drop_session` / `SessionSlot::Busy.closed` (`crates/engine/src/console.rs:267–274`): make the session-release wait explicit in a named operation or lifecycle signal; the `closed` receiver carries closure reasons but this loop actually waits for all senders to disappear, so internal lifetime ordering should be expressed by code rather than the deleted comment. `Lease::drop` (`crates/engine/src/session/mssql.rs:58–62`) deliberately retains a sender until after dropping the client; this is the concrete producer/consumer coupling behind the flag. Report-only: this patch deletes prose and does not perform that refactor.

## Keeps with exact source proof

All line numbers refer to the frozen working tree. These are source-path checks today; database tests were inspected as evidence, not executed in this pass.

- `crates/engine/src/model.rs:442`, `TxnState`: public IPC contract for `Aborted` versus the new `Open`. `TxnState` is `pub`, derives `Serialize` and `Type` at line 441; `MsSession::transaction` (`session/mssql.rs:280–281`) maps only transaction presence to `Open`, and `MsSession::txn` (142–143) withholds depth while non-idle. `txn::txn_after` (40, 48–49) preserves unknown `Open` and failed PostgreSQL `Aborted`; `Engine::apply_edits`'s transaction gate (`lib.rs:1194–1195`) rejects `Aborted`. This is caller-visible state meaning, not implementation narration.
- `crates/engine/src/console.rs:87`, `ConsoleInner::finish_refreshes`: external SQL Server batch behavior explains why rollback/retirement can still owe a refresh. The live path selects `NativeEnd` only for SQL Server (75–77), records invalidation before execution (363–368), then finishes it after observed transaction state (377–393), even when `committed` is false. The concrete database-backed proof encoded in `rollback_button_refreshes_ddl_committed_before_native_begin` (`crates/engine/tests/mssql_refresh_outcomes.rs:189–202`) runs `CREATE TABLE ...; BEGIN TRANSACTION`, checks visibility through another session before rollback, then checks the refresh after rollback. The external batch boundary cannot be reshaped by an internal rename. No claim that this test was rerun here.
- `vendor/tiberius/src/client.rs:84`, `SessionState`: public result contract, re-exported at `vendor/tiberius/src/lib.rs:236`; its construction (`client.rs:104–117`) only reads already decoded context. Production callers are `MsSession::open` (`crates/engine/src/session/mssql.rs:123–129`) and `observe` (271–277).
- `vendor/tiberius/src/client.rs:87`, `SessionState::database`: public field provenance and alias limitation. `get_env_change` (`tds/stream/token.rs:374–375`) copies `new` directly; `session_state` (`client.rs:113`) borrows that exact text without canonicalization. The engine retains it at `session/mssql.rs:128`. The alias check is also represented by `a_login_alias_still_retires_a_session_that_changes_database` in that file; not executed here.
- `vendor/tiberius/src/client.rs:89–90`, `SessionState::has_transaction`: public field meaning and limits. `client.rs:114` compares the descriptor with zero; `tds/stream/token.rs:390–396` sets/clears it on transaction ENVCHANGE. It does not decode depth or committability. Production `transaction` and `txn` return `Open`/unknown depth (engine `session/mssql.rs:142–143,280–281`).
- `vendor/tiberius/src/client.rs:92`, `SessionState::server_process_id`: public field provenance. `client/connection.rs:1213–1215` reads nonzero `packet.header.spid()` into context; `client.rs:115` exposes it; the engine uses it as `backend_pid` at `session/mssql.rs:124`.
- `vendor/tiberius/src/client.rs:94`, `SessionState::server_version`: public tuple ordering/provenance. `get_login_ack` (`tds/stream/token.rs:416–420`) converts LOGINACK into `(major, minor, build)`; `client.rs:116` exposes it; engine `session/mssql.rs:125–127` consumes that ordering.
- `vendor/tiberius/src/client.rs:99–103`, `Client::session_state`: public preconditions, no-I/O guarantee, errors, and reuse limit. Its body (`104–117`) performs only local reads; it rejects `!connection.is_eof()` and unobservable state. `tds/stream/token.rs:398–401` marks unsupported transaction changes unobservable. The production query path restores the lease only after `observe` succeeds (`engine session/mssql.rs:196–205`), and cancellation retires the lease instead (`184–188`), so the API warning describes an enforced live boundary.

The six vendored blocks above are the only actual source-added comments in `UPSTREAM.patch` (the `+///` lines in its `src/client.rs` hunk). Context comments in that patch were not classified as additions. No vendored source or patch bytes need changing.

## Concrete deletion patch

Apply from the scope root. This only removes the two condemned lines; the parent owns acceptance and any code refactor.

```diff
--- a/crates/engine/src/console.rs
+++ b/crates/engine/src/console.rs
@@ -269,7 +269,6 @@
                 shared.spawn(async move {
                     cancel.cancel().await;
                     if let Some((db, owed)) = due {
-                        // The orphaned task must release its session before metadata can see its final DDL.
                         while closed.changed().await.is_ok() {}
                         worker.schedule_refreshes(db, owed);
                     }
--- a/vendor/tiberius/verify-upstream.py
+++ b/vendor/tiberius/verify-upstream.py
@@ -1,5 +1,4 @@
 #!/usr/bin/env python3
-"""Verify the pinned archive and compare the local delta with UPSTREAM.patch."""
 import difflib
 import hashlib
 from pathlib import Path
```

## Skipped scope

- Tests, including `crates/engine/tests/*`, test modules inside production files, `src/shell/TopBar.test.tsx`, and the test-only `crates/engine/src/session/mssql/mssql_metadata_visibility.rs`. A few assertions were read solely to locate evidence for keeps.
- Docs, README, screenshot, generated `src/ipc/bindings.ts`, Cargo manifests/lock, and license headers.
- All unchanged upstream comments/licenses and the 96 included upstream files already proved unchanged by the parent's archive verification. No reformatting, regeneration, or archive re-verification was performed.
- `vendor/tiberius/UPSTREAM.patch` context and removed lines; only actual added source comments were reviewed.
- `dev/mssql-smoke.py`, `dev/mssql-tls-smoke.py`, `crates/engine/src/lib.rs`, `session.rs`, `session/mssql.rs`, `session/mssql/detail.rs`, `txn.rs`, and `src/shell/TopBar.tsx` have no additional new production comments/suppressions to delete in this range. Existing comments beside modified signatures were preserved.

Reviewed comment-bearing files: `crates/engine/src/console.rs`, `crates/engine/src/model.rs`, the `vendor/tiberius/src/client.rs` addition hunk in `vendor/tiberius/UPSTREAM.patch`, and `vendor/tiberius/verify-upstream.py`.
