# Independent SQL Server review A, round 4

Reviewed pinned revision `b81b4e4c839dd813bbf8b4d0fa8995f615f4a6fd` against `main`. The source checkout was clean before and after review and remained at that revision.

Verdict: **request changes**. **1 critical, 0 warnings, 0 nits.**

## Finding

### [critical] Refreshing the catalog after a primary-key change can retarget a pending edit to another row

**Locations in the changed implementation:** `crates/engine/src/session/mssql/edits.rs:96-114`, `crates/engine/src/lib.rs:1143-1147`, and `src/features/results/ResultGrid.tsx:60-66`.

**Supporting call sites:** `src/features/editor/runActions.ts:346-369`; `src/state/tableEdits.ts:10-13,68-85`; `src/features/results/ResultGrid.tsx:274-278`.

The new SQL Server guard validates the current catalog against the live database, but it does not establish which primary-key columns the pending values originally belonged to. The frontend stores staged keys as positional value arrays. At submission, it attaches column names from the latest catalog. A catalog refresh after changing the primary key can therefore turn an original `id = 1` into `alternate = 1`. The guard sees a valid current primary key, a lossless conversion, and exactly one affected row. It commits a change to the wrong row.

This is an integration gap in the new SQL Server write-identity guarantee. The positional frontend container predates this PR; reusing it means the new driver check cannot establish the promised original identity after a catalog refresh. It is distinct from the earlier decimal-rounding defect. All types and conversions remain exact in this reproduction.

**Reachable path:**

1. Browse a table with `id int PRIMARY KEY`, `alternate int NOT NULL`, and a writable text column. The two rows are `(1,2,'selected')` and `(2,1,'other')`.
2. Stage a note edit on row `id = 1`. `rowIdOf` and `setCell` retain `["1"]`, without its originating column name.
3. Another connection drops the primary key on `id` and adds it on `alternate`. Refresh the first connection's catalog while retaining its old result and pending edit.
4. `gridEditing` still enables editing. The table OID, column IDs, column names, and result shape are unchanged, and the new PK column is present in the old result.
5. Submit calls `tablePrimaryKey(tab)` against the refreshed catalog. `toChanges` pairs the old value with the new name and sends `key: [{ column: "alternate", value: "1" }]`.
6. `Engine::table_apply` also reads the refreshed relation. `prepare_edits` acquires its X lock and compares live metadata with that refreshed relation. The PK check and native round trip pass. The generated predicate targets the row whose original `id` was 2, and the one-row count permits commit.

**Executed proof:** A public-Engine probe used two independently saved SQL Server connections to the existing fixture, an isolated temporary engine store, and a uniquely named disposable table. The Engine library and Tokio dependencies came from Cargo JSON artifact output for the current pinned source. It performed the actual TablePage, external PK DDL, catalog refresh, table_apply, and an observer query.

```text
original_page=[["1","2","selected"],["2","1","other"]]
current_pk=["alternate"]
apply=Ok(ApplySummary { committed: true })
observer=[["1","2","selected"],["2","1","edited"]]
cleanup_verified
```

The probe ran twice with different unique tables and reproduced the same committed wrong-row update. Its cleanup closes and rolls back probe sessions, drops the table, checks `OBJECT_ID` is NULL, shuts down the Engine, and removes its temporary store. No seeded row was written.

I separately loaded the actual frontend modules with Jiti and supplied the old result metadata and refreshed catalog captured from the real Engine probe. The actual `gridEditing` and `submitEdits` functions produced:

```text
grid_editing {"pkColumns":["alternate"],"enabled":true}
actual_submit_payload ... "updates":[{"key":[{"column":"alternate","value":"1"}],"set":[{"column":"note","value":"edited"}]}]
```

Only the frontend IPC sink was intercepted to capture its request. The database effect was independently verified through the real Engine using that same payload shape. This was module execution, not a browser interaction. The equivalent delete path uses the same positional rebinding, but a wrong-row delete was not separately executed.

**Bounded fix:** Preserve the column names with each original key when staging the edit. Do not reconstruct key identity from the current catalog at submission. Once the old named key remains `id = 1`, the Engine's current full-key validation can reject it after the PK moves to `alternate`. Alternatively, bind staged changes to immutable result/edit provenance and reject a changed primary-key mapping before mutation. Retain pending user edits on refusal. Add a regression that stages an edit, changes the PK to a different same-type column, refreshes the catalog, and invokes the actual submit path. Assert both rows remain unchanged on refusal. Cover deletes as well.

The existing `table_column_and_primary_key_identity_drift_refuse_the_entire_apply` test does not refresh after DDL. It proves comparison with stale cached metadata, but does not exercise the refreshed-catalog path above. The X lock correctly prevents concurrent DDL during validation; it cannot recover identity already lost before validation.

**Evidence artifacts:**

- `/tmp/l1data-review-a-r4-probe.rs` and `/tmp/l1data-review-a-r4-probe.log` contain the public-Engine reproduction and observed values.
- `/tmp/l1data-review-a-r4-artifacts.jsonl` records the exact Cargo artifacts used to compile that probe.
- `/tmp/l1data-review-a-r4-ui.mjs`, `/tmp/l1data-review-a-r4-ui-data.json`, and `/tmp/l1data-review-a-r4-ui.log` contain the real metadata, actual frontend execution, and captured submit payload.

## Independent checks and scope

Read AGENTS.md, the architecture's load-bearing decisions and implementation reconciliation, README limits, and `.audit/mssql/{implementation,verification,review}.md`. Traced the full-feature diff with particular attention to staged-write conversion and locking, original-key provenance, Manual planning and preflight, session leases and cancellation outcomes, PostgreSQL result metadata, persisted kind defaults/history migration, and assistant refusal at policy and execution. No additional independently established security or structural finding is reported.

Independently ran these bounded checks at the pinned revision:

- The new wrong-row Engine reproduction and actual frontend submit probe described above.
- `cargo test --locked -p engine --features pg-tests,mssql-tests --test pg table_page_preserves_nested_domain_native_metadata_and_primary_key -- --exact --nocapture`. Passed, 1 test. The disposable schema was removed and absence checked by the test. Log `/tmp/l1data-review-a-r4-pg.log`.
- `cargo test --locked -p engine --features pg-tests,mssql-tests --test mssql_edit_drift concurrent_ddl_waits_for_the_checked_edit_and_auto_commit_releases_the_lock -- --exact --nocapture`. Passed, 1 test. It observes blocked DDL, Manual and Auto lock release, and cancellation rolling back the still-open edit while preserving the earlier committed value. Log `/tmp/l1data-review-a-r4-mssql_edit_drift.log`.
- `cargo test --locked -p engine --lib database_compatibility_tests -- --nocapture`. Passed, 1 test covering older connection/context defaults and unknown-kind rejection. Log `/tmp/l1data-review-a-r4-compat.log`.

Pstack's Prove It Works principle led to checking the persisted observer values and the actual frontend payload, rather than inferring correctness from the lock or affected-row count. Type System Discipline informed the bounded fix of retaining named key identity instead of a positional array whose meaning can change.

This was a bounded review of approximately ten minutes, not exhaustive execution of the entire feature matrix. I did not rerun full workspace/frontend/database suites, TLS/container checks, native or browser acceptance, real provider calls, or remote CI. The supplied wider passes remain context. I made no source edits, commits, subagents, PRs, service/configuration changes, or seeded-row writes. This report does not approve the feature or close the remaining owner gates.
