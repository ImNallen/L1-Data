# Independent review A — round 6

Pinned production source: `7ebc2f07c2bbfec284317caebab4942078c36997` in `/Users/nallen/.t3/worktrees/L1Data/t3-a0f42d3d`.

**Verdict: request changes. Counts: 0 critical, 1 warning, 0 nit.** This is my independent review only. I did not dispatch reviewers, invoke a panel, edit production source, commit, or operate a real provider. The narrowed SQL Server Manual/staged-write scope is accepted as the review contract.

## Warning A6-1 — Hidden alias-type metadata breaks catalogs or silently removes readable columns

Locations:

- `crates/engine/src/session/mssql/catalog.rs:93`–`105`: the column query requires an inner join to the declared `sys.types` row before using the independently available system type.
- `crates/engine/src/session/mssql/catalog.rs:129`–`137`: the routine parameter query feeds nullable `TYPE_NAME(p.user_type_id)` to `required(&r, 2)`.
- Downstream consequence: `crates/engine/src/text.rs:476` builds the SQL Server browse projection from that incomplete column list. `crates/engine/src/console.rs:451` compares the resulting projection with the same incomplete catalog, so the omission passes the shape check.

SQL Server allows a role to SELECT a table and EXECUTE a procedure using an alias type without allowing that role to see the type's metadata. The current catalog assumes type metadata visibility follows access to its referencing object. That assumption fails in two reachable paths:

1. `Engine::catalog` → `Session::catalog` → `MsSession::catalog` → routine parameter construction: a visible executable routine with a hidden alias parameter makes the entire database catalog fail with `Missing SQL Server metadata field 2`.
2. With that routine hidden, `Engine::catalog` → column metadata query drops the readable alias-typed column. `Engine::console_run(TablePage)` then projects only the retained columns and reports success. The browse grid and its result omit data the same login can read with ordinary SQL.

### Reproduction and observed state

I compiled a disposable public-Engine probe against artifacts reported by Cargo JSON for this pinned tree. Its source is `/tmp/l1data-mssql-a-r6-metadata.rs`; the successful rerun is `/tmp/l1data-mssql-a-r6-metadata-verified.log`.

Using unique disposable names in `shop`, an administrator created:

```sql
CREATE TYPE dbo.<type> FROM nvarchar(20) NULL;
CREATE TABLE dbo.<table> (id int PRIMARY KEY, value dbo.<type>);
INSERT dbo.<table> VALUES (1, N'visible');
-- Separate CREATE PROCEDURE batch:
CREATE PROCEDURE dbo.<proc> @value dbo.<type> AS SELECT @value;
-- A new SQL login/user received SELECT on this table and EXECUTE on this procedure.
```

The restricted principal's native metadata query returned `@value`, user_type_id `257`, and SQL NULL for `TYPE_NAME`. The owner's equivalent query returned the actual alias name. Public Engine execution of the procedure returned `works`, with `RunOutcome::Ok` and an open reusable session. Public Engine catalog loading nevertheless returned:

```text
Err(Failed { message: "Missing SQL Server metadata field 2" })
```

After revoking only EXECUTE on the procedure, catalog refresh succeeded, but the table's catalog listed only `id`. Through the same public Engine session, ordinary SQL returned both `1` and `visible`. A TablePage run succeeded with only the `id` result column; `value` was absent. This is not a permission denial or stale-schema race.

As a diagnostic control, granting `VIEW DEFINITION` on the disposable type, and restoring the procedure grant, made the catalog refresh succeed with both columns and the routine's alias-typed argument. That extra grant is evidence of the cause, not the proposed product workaround.

### Bounded correction

Keep object/column/parameter identities independent of optional type visibility. Resolve a visible base/system type independently of the declared alias row (the column query should not require that declared row). If a type's descriptive name is genuinely unavailable, represent/omit that annotation honestly without discarding an otherwise readable column or failing unrelated catalog entries. Keep unsupported-value refusal at the established conversion boundary. Do not require broader database permissions just to use the catalog.

Add a real restricted-login Engine regression with an alias-typed table column and executable procedure parameter. Assert catalog success, retention of all readable columns, TablePage values matching an ordinary SELECT, and useful routine metadata; retain an owner/type-visible control.

### Cleanup and test limits

Both rounds' disposable procedures, types, users and logins, and the second round's table and its single row, were removed. The initial login drop raced connection teardown; the next probe explicitly removed it, waited for its own restricted sessions to close, and checked cleanup. An independent final read confirmed zero matching tables, procedures, types, users and logins: `/tmp/l1data-mssql-a-r6-cleanup-check.log`. No seeded/user rows were changed, and shared services were neither changed nor restarted.

The failure was reproduced through the real Engine catalog, native script and TablePage APIs. I did not drive the browser or invoke an assistant turn for this finding; their catalog dependency is source-traced, not claimed as a separately executed UI/model test.

## Other checks and bounded review coverage

- Read AGENTS.md, README capability limits, architecture load-bearing/reconciliation sections, the final-scope implementation paragraph and integrated review dispositions. Inspected changes across the private driver/facade, native query lifecycle, results, transaction/refresh handling, metadata, T-SQL construction, persistence/history, assistant refusal boundaries, and frontend target/edit provenance. This was a timeboxed review, not exhaustive proof of every full-feature path.
- Independently compared real Engine and raw Tiberius sessions using local temporary tables: startup rowcount/error, zero-row guarded writes across consecutive runs, NOCOUNT, implicit transactions, savepoint rollback, dynamic commit/replace behavior, error recovery and later transaction ENVCHANGE after a multi-packet response. No native-state discrepancy was found in these probes. Evidence: `/tmp/l1data-mssql-a-r6-probe.rs` and `/tmp/l1data-mssql-a-r6-probe.log`. Temporary tables were explicitly dropped and OBJECT_ID returned NULL on both sessions before closing them.
- `cargo test -p engine --features 'pg-tests mssql-tests' --test mssql_session_state --test mssql_refresh_outcomes`: **PASS, 14 tests** (7 each). This includes native-state/error/cancellation controls, committed DDL after rollback/loss/forced close, and PostgreSQL rollback/retirement controls. Log: `/tmp/l1data-mssql-a-r6-focused-tests.log`.
- `python3 vendor/tiberius/verify-upstream.py <cached tiberius-0.13.0.crate>`: **PASS**. Pinned archive SHA matched; 96 included upstream files were unchanged; the seven changed files, including the manifest, exactly matched the recorded 210-line patch. I reviewed that delta rather than treating the unchanged vendored implementation as new app code.
- No separate structural blocker was established. The private enum-backed driver facade and separated T-SQL expression/table modules keep the new behavior in its canonical layers. Existing large engine contract/facade files were already over 1,000 lines on main; the source inspection did not establish a new threshold-crossing blocker.
- The observed working-tree screenshot modification is owner-owned; production HEAD remained pinned. I made no source edits. Temporary probe/report artifacts remain in `/tmp` for the parent to inspect.

**Root UI/native acceptance, screenshots and remote CI remain owner-owned gates.** These focused passes do not approve those gates or erase this request-changes finding. I did not rerun the full frontend/workspace suites, TLS/container checks, or real providers.
