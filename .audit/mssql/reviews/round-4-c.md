Owner note. This preserves the received bounded report. Its Independent review panel paragraph is invalid and excluded from evidence. The parent dispatched the real configured panel and stopped this agent and any nested work. The XACT_ABORT finding was independently reproduced by the boundary investigator. No approval relies on the report's panel claims.

# Independent SQL Server review R4

Revision `b81b4e4c839dd813bbf8b4d0fa8995f615f4a6fd`, full `main...HEAD`.

## Intent

Add SQL Server beside PostgreSQL, with the saved immutable connection kind selecting the private driver and dialect across editing, interpretation, generated SQL, metadata, execution, transactions, results, history, and assistant context. Existing PostgreSQL saved data and workflows must remain usable. SQL Server batches, values, cancellation, staged edits, Manual-mode preflight, and assistant refusals must preserve their stated exactness and safety limits.

## Verdict

Request changes: three critical findings and one warning. The critical findings can commit a change to the wrong row or lose earlier uncommitted Manual work. The warning commits values different from the user's entered numeric or temporal text.

## Findings

### 1. [critical] A stale case-insensitive text key can update a replacement row

**Location:** `crates/engine/src/session/mssql/edits.rs:19, 123-145`.

**Reachable call chain:** `Engine::table_apply` resolves the current relation and primary key, then SQL Server `prepare_edits` takes a table lock and validates that the supplied key survives conversion to the declared type. It does not read the row selected by the generated predicate and compare that row's actual canonical key with the displayed key. `lower` builds a `WHERE key_column = @parameter` predicate, so SQL Server applies the column collation. `execute_edit` accepts one affected row and `table_apply` commits in Auto-commit mode.

**Evidence:** A public Engine probe created a disposable table with a `varchar` primary key using `SQL_Latin1_General_CP1_CI_AS`. After replacing key `a` with `A` in another session, I submitted an update carrying stale key `a`. `table_apply` returned `Ok(ApplySummary { committed: true })`; a read showed that the replacement `A` row had been changed. The same Engine probe independently confirmed the collation-sensitive equality through the actual staged-edit path.

**Bounded fix:** While holding the existing table lock, compare the target row's actual canonical key text with the staged original key and refuse if they differ. Keep the equality predicate for locating the candidate, but do not treat its affected-row count as proof that it is the row the user saw.

### 2. [critical] Catalog refresh can relabel a staged key and retarget its edit

**Location:** `src/features/results/ResultGrid.tsx:63`, `src/state/tableEdits.ts:84`, `src/features/editor/runActions.ts:368`, `crates/engine/src/session/mssql/edits.rs:96`.

**Reachable call chain:** `gridEditing` confirms that the current catalog relation is a table and that result columns still have the same table OID, attribute numbers, and names. It does not bind pending edits to the primary-key identity that existed when the row key was captured. `setCell` stores the old key as positional values. At submission, `submitEdits` obtains `tablePrimaryKey(tab)` from the latest catalog; `toChanges` pairs the stored values with those current primary-key names. The Engine then validates the refreshed key definition and executes the request against it.

**Evidence:** Reviewer A's in-memory probe of the real `gridEditing` and `submitEdits` functions showed that pending key value `1` was submitted as `other=1` after the catalog primary key changed from `id` to `other`. I independently used the public Engine API with a unique disposable table containing `(id=1, other=2)` and `(id=2, other=1)`: after replacing the primary key and refreshing the catalog, `table_apply` for `other=1` committed, and the row with `id=2` changed. This proves the backend consequence; the staged-state-to-payload part is from A's real-function probe and the source path above.

**Bounded fix:** Preserve the named key cells and table/key identity from the time the edit is staged. Before submit, reject a pending edit if the current relation's key identity or definition differs; do not pair old positional values with newly fetched key names.

### 3. [critical] Failed staged edits can discard earlier Manual work when `XACT_ABORT` was inherited

**Location:** `crates/engine/src/lib.rs:1178-1184, 1218-1222`; refusal boundary in `crates/engine/src/txn.rs:10-15`.

**Reachable call chain:** In Auto-commit, `txn::plan` returns `Native` before the SQL Server Manual preflight, so `SET XACT_ABORT ON` runs and remains on the tab session. Switching an idle tab to Manual keeps that session. After an ordinary Manual update opens the transaction, `table_apply` creates `SAVE TRANSACTION l1_apply`. A staged edit that violates a constraint terminates the SQL Server transaction under `XACT_ABORT ON`; the attempted savepoint rollback then fails and the Engine retires the session, dropping earlier uncommitted Manual work.

**Evidence:** A bounded public Engine probe used a unique disposable table. It set `XACT_ABORT ON` in Auto-commit, switched to Manual, changed a row from `n=1` to `n=3`, and confirmed `n=3` inside the open transaction. A staged edit then attempted a duplicate unique value. `table_apply` failed because rollback found no active transaction, the tab session became lost, and a separate observer read the original committed `n=1` rather than the earlier uncommitted `n=3`. The probe dropped the table and confirmed it was absent afterward.

**Bounded fix:** Before using a savepoint for staged edits inside an existing Manual transaction, refuse when inherited `XACT_ABORT` makes that savepoint guarantee false, or safely bracket the internal edit with a setting that preserves rollback behavior on all paths. Add an Engine regression that proves earlier Manual work remains available after a constraint failure.

### 4. [warning] Numeric and temporal staged edits can silently round the entered value

**Location:** `crates/engine/src/session/mssql/edits.rs:123-145`; native conversions in `crates/engine/src/sql/mssql_value.rs:47-56`.

**Reachable call chain:** `prepare_edits::validate_cells` skips conversion-roundtrip validation for non-text assignments. The generated edit input still converts the parameter to the declared SQL Server type, where decimal scale or temporal precision can round it. The write then succeeds and Auto-commit commits the converted value.

**Evidence:** In the same disposable public Engine probe, setting `decimal(8,2)` to `1.239` committed as `1.24`; setting `datetime2(3)` to `2024-01-02 03:04:05.1237` committed as `2024-01-02 03:04:05.1240000`. The `table_apply` call returned committed success.

**Bounded fix:** For canonical editable numeric and temporal inputs, compare the native converted value with the submitted canonical text before any staged write. Refuse values whose declared precision or scale changes them, and cover rounding boundaries in a real Engine regression.

## Independent verification and limits

- Read root `AGENTS.md`, architecture load-bearing decisions and implementation reconciliation, README limits, `.audit/mssql/{implementation,verification,review}.md`, and the R3 reviews and trail review.
- Traced the table-edit, key validation, frontend staging/submission, Manual transaction, and rollback paths in current source.
- Ran two targeted public Engine probes against the local SQL Server fixture using unique disposable tables: one for inherited `XACT_ABORT` rollback; one for stale case-insensitive key replacement, decimal/temporal rounding, and refreshed-primary-key retargeting. The latter's first cleanup attempt ran after closing its observer and failed; a cleanup-only Engine session then dropped both generated tables and confirmed zero matching catalog entries. No seeded rows, services, configs, or application source were changed.
- No broad suites were rerun. This pass does not establish PostgreSQL preservation, persisted compatibility, provenance, cancellation truth, or general correctness outside these paths. I did not independently trace or verify the separately reported trigger commit/reopen path or ambiguous apply-error wording.
- Reviewer A and B returned findings that overlap findings 1 and 2; the evidence above distinguishes their frontend function probe from my Engine-path checks. Reviewer C did not contribute to this bounded pass.

The `Prove It Works` principle shaped the evidence choice: I exercised the public Engine path against the real SQL Server fixture and read the stored values and rows rather than relying on source inspection alone.
