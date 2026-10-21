# Independent review B, round 5

Pinned source: `98301df8f4d2c742931875f6e9e297f464989c9d`, reviewed against `main`.

Verdict: **Request changes.** Counts: **0 critical, 1 warning, 0 nit**. This is one reviewer's result, not a panel result or approval. The narrowed SQL Server first-release capability contract is accepted as the review target.

## W1 — warning: Hidden SQL Server definitions break all relation detail and assistant description

**Location:** `crates/engine/src/session/mssql/detail.rs:23–30`, especially `required(&r, 3)` at line 30 and `required(&r, 4)` at line 28. The analogous CHECK definition requirement is at line 70.

**Reachable paths:**

- Expanding a relation: `src/features/sidebar/ConnectionTree.tsx:328` → `useRelationDetail.ensure` → `Engine::relation_detail` (`crates/engine/src/lib.rs:958–970`) → `Session::detail` → `MsSession::detail` → `required` on a nullable server definition.
- Assistant schema tool: `assistant::tools::describe_tables` (`crates/engine/src/assistant/tools.rs:197–210`) calls that same detail reader before emitting the catalog's available column information. Its `?` propagates the missing-definition failure for the entire tool call.

**Observed evidence:** A disposable real Engine probe created a unique SQL login/user with only SELECT permission on a unique table:

```sql
CREATE TABLE dbo.[unique_table](
    id int PRIMARY KEY,
    n int DEFAULT(42),
    computed AS(id+1)
);
INSERT INTO dbo.[unique_table](id) VALUES(1);
GRANT SELECT ON OBJECT::dbo.[unique_table] TO [unique_user];
```

As this login, Engine connection and catalog refresh succeeded. `SELECT *` returned `[["1","42","2"]]`. The catalog contained all three columns, their types, primary-key flag, and the computed column's non-writable flag. Calling the public `Engine::relation_detail(db, oid)` then returned exactly:

```json
{"Err":{"kind":"Failed","message":"Missing SQL Server metadata field 3"}}
```

A preceding impersonation probe independently showed `dc.definition` and `cc.definition` are SQL NULL under these permissions. Hidden definition text is a legitimate server state, not a malformed response. An ordinary read account consequently loses available keys/indexes and cannot use the assistant's table-description path for tables with these defaults or computed columns. This violates the supported catalog/schema-tool behavior without involving SQL Server writes or Manual mode.

**Bounded suggestion:** Treat hidden default/computed/CHECK text as unavailable optional metadata, retaining the rest of the relation detail. Skip or explicitly mark the unavailable expression rather than requiring it or manufacturing SQL. Preserve visible definitions for owners. Add a real Engine regression using a SELECT-only login and a table with default/computed/CHECK expressions; assert useful detail remains available. Do not require broad metadata privileges merely to recover the already-readable columns and keys.

**Artifacts:** `/tmp/l1data-review-b-r5-metadata.rs`, `/tmp/l1data-review-b-r5-metadata.log`; preliminary visibility/native-state probe `/tmp/l1data-review-b-r5-probe.rs` and `.log`.

**Cleanup:** Both probes dropped their unique table and database user; the authenticated probe also dropped its unique server login after disconnecting its Engine. A final query counted matching objects, database principals and server principals and returned `0`. No seeded rows were changed.

## Independently checked scope

Read AGENTS.md, README limits, architecture load-bearing decisions and implementation reconciliation, the implementation document's final-scope override, and review decisions/history. Inspected the full feature diff inventory and traced capability boundaries, private drivers, streaming output, typed/canonical filter construction, persistence/history, assistant refusal paths, frontend target/mode changes, and named pending keys. This was a bounded source review, not exhaustive proof of every changed line.

Built the pinned engine with `cargo build -p engine --lib --message-format=json`. Disposable Rust probes linked the exact Cargo JSON artifact paths; no stale-library glob selection was used. Build succeeded. No source was edited.

Additional real Engine checks passed:

- Native explicit transaction nesting reported Active/depth 2; the Commit button API completed both levels and subsequent count/depth were `1/0`.
- `SET IMPLICIT_TRANSACTIONS ON` followed by a constant SELECT remained Idle/depth 0, with settings switched back afterward.
- In an active native transaction containing a temporary-table insert, requesting SQL Server Manual mode and calling `table_apply` against a nonexistent table both returned typed Unsupported. The full serialized ConsoleState was unchanged across both refusals. The existing row remained readable in the same transaction; Rollback then left the temporary table empty.
- Variables stayed in one batch; two SELECT results plus a later GO batch returned `11`, all 38 digits of `12345678901234567890123456789012345678`, and `12` separately.
- A later raw-money result failed explicitly while preserving an earlier result (`41`); a subsequent run returned `42` on a usable session.

Boundary check artifacts: `/tmp/l1data-review-b-r5-boundary.rs` and `.log`. Temporary tables were confined to these probe sessions and removed when their Engines disconnected. Temporary Engine data directories were scoped to each probe. No provider was invoked, nested agent dispatched, service restarted, or source/commit/PR changed.

No independent additional structural blocker was established. The private driver split and focused T-SQL submodules keep dialect behavior in the engine; the inspected frontend grid remains under 1,000 lines. The already-large lib/model files predate this change, so their size alone is not reported as a new threshold violation.

I did not rerun TLS/container checks, the complete suites, real-provider tests, native/browser UI verification, or remote CI. Parent-provided broad passes are context and are not counted as my own results. Root UI/screenshot and CI gates remain owner-owned. HEAD remained pinned; the only visible working-tree change at the final check was the root-owned main screenshot.
