# Independent SQL Server review A, round 3

Pinned revision `24200b1bab2fdf49293b064d6bced18b2be2c262`. Reviewed the full `main...HEAD` feature scope, with focused tracing of the integrated transaction, filter, header, value, identity, persistence and PostgreSQL paths. Read AGENTS.md, architecture load-bearing decisions and reconciliation, README limits, `.audit/mssql/{implementation,verification,review}.md`, and all six prior review reports. Prior findings and supplied suite results were context, not approvals.

Verdict: request changes. One critical finding, two warnings, zero nits. The critical finding committed a wrong-row update in an isolated probe. These are distinct from the corrected earlier findings and the disproved legacy `binary` premise.

## Findings

### 1. [critical] Stale SQL Server column types can round a displayed primary key and commit an update to another row

**Locations:** `crates/engine/src/sql/mssql_value.rs:43-44`; `crates/engine/src/session/mssql/edits.rs:8-17`; `crates/engine/src/console.rs:393-400`; `crates/engine/src/lib.rs:1143-1151`.

**Failure:** Staged predicates convert the original grid key to the type and scale in a cached catalog. The table-page shape check compares only names and broad categories, then overwrites returned metadata with that cached type. An external schema change can preserve both names and `Numeric` while changing the key's scale. The page is accepted as editable, but submitting its exact key rounds it to a different existing key. The affected-row check sees one row and commits.

**Reachable call chain:** Another saved connection or external client alters a table, without refreshing the first connection's catalog. `Engine::console_run(TablePage)` takes the first catalog's `Relation`. `run_outputs` accepts identical column names/categories and attaches its cached provenance. `ResultGrid.rowIdOf` retains the actual returned key string. `toChanges` sends it unchanged. `Engine::table_apply` obtains the same cached relation, `mssql::edits::key` calls `parameter`, and `input_sql` emits `CONVERT(decimal(8,2), @P...)`. `lib.rs:1207` accepts the one affected row and `:1211` commits.

**Executed Engine evidence:** Two independent saved connections targeted `tempdb`. The first created a uniquely named global temporary table with `id decimal(8,2) PRIMARY KEY` and `(1.23, 'original row')`, then loaded catalog version 4. The second dropped the PK, changed the column to `decimal(8,3)`, recreated the PK, and inserted `(1.234, 'new precision row')`. No refresh was requested on the first connection.

The first connection's actual TablePage succeeded and returned:

```text
1.230 | original row
1.234 | new precision row
```

Its result metadata still advertised `decimal(8,2)`, and its catalog remained version 4. Submitting an update using the returned key `1.234` produced:

```text
Apply Ok(ApplySummary { committed: true })
1.230 | edited new precision row
1.234 | new precision row
```

The same predicate construction is used for deletes. A wrong-row delete was not separately executed. All modified rows belonged to the disposable table. It was dropped and `OBJECT_ID` returned NULL afterward.

**Bounded fix:** Original keys must never undergo a lossy conversion. At minimum, refuse a key that cannot be represented losslessly by the cached type. Revalidate live relation/type/PK identity at staged-write execution, with a concurrency-safe relationship to the write, instead of treating category equality as proof that cached metadata is safe. Preserve enough native type precision/scale to detect this drift. Add a regression with two connections, scale-2 to scale-3 DDL, and both `1.230` and `1.234`, asserting that an edit either changes only the selected row or refuses before any write. A general refresh hint after the write cannot repair this failure.

Evidence files `/tmp/l1data-mssql-review-a-r3-stale.rs` and `/tmp/l1data-mssql-review-a-r3-stale.log`.

### 2. [warning] The new shared table-shape check rejects unchanged PostgreSQL tables with nested domains

**Location:** `crates/engine/src/console.rs:393-400`. Supporting mismatch in `crates/engine/src/session/postgres/catalog.rs:137-152` and `session/postgres/types.rs::catalog_category`.

**Failure:** The PostgreSQL catalog unwraps only one domain level. A domain over another domain therefore has catalog category `Other`, while PostgreSQL's prepared result metadata correctly reports its underlying integer category. The newly added shared table-page check treats this as changed schema and fails every browse, even immediately after a refresh. Main did not impose this category equality on PG table results.

**Reachable call chain:** Sidebar table open or restored browse tab sends `TablePage`. `Engine::console_run` attaches the cached relation to `RunPlan`. The PG driver prepares the query and emits native integer metadata. `run_outputs` compares it with the catalog's `Other` and fails before creating a result set. Refreshing repeats the same mismatch.

**Executed Engine evidence:** Created a unique disposable PostgreSQL schema containing:

```sql
CREATE DOMAIN <schema>.first_level AS int;
CREATE DOMAIN <schema>.second_level AS <schema>.first_level;
CREATE TABLE <schema>.items(id int PRIMARY KEY, value <schema>.second_level);
INSERT INTO <schema>.items VALUES (1, 42);
```

After an explicit catalog refresh, `value` had category `Other`. A normal query returned `1, 42` with integer metadata and valid native column sources. The real TablePage request failed with `Table shape changed. Refresh the catalog before browsing again.` It returned no result set. The schema was dropped with all its probe-owned objects, and a namespace count confirmed zero remaining.

**Bounded fix:** Preserve PostgreSQL's native result type/source information instead of imposing SQL Server's synthetic provenance assumptions on it. If PG category comparison is retained, recursively resolve domain bases so both sides describe the same type, and cover nested domains through the real TablePage path. The shared guard must not reject stable metadata that PG itself reports correctly.

Evidence files `/tmp/l1data-mssql-review-a-r3-pg.rs` and `/tmp/l1data-mssql-review-a-r3-pg.log`.

### 3. [warning] Previously recognized PostgreSQL browse text with a semicolon or whitespace loses browsing after upgrade

**Location:** `src/features/editor/runActions.ts:85-86`; downstream guards at `:134` and `:293-300`; persistence at `src/state/workspace.ts:103-119` and restoration at `src/boot.ts:49-56`.

**Failure:** Main's `isBrowsing` applies `trim().replace(/;$/, "")` before comparing with its template. HEAD compares the document byte-for-byte. Existing table tabs whose text is `SELECT * FROM public.orders;`, or has surrounding whitespace, were valid browse tabs before this change. Workspace persistence keeps that exact text, and boot preserves non-null saved text. They now silently become script tabs, losing paging, filters and staged editing. The initial page load exits before IPC; a later manual Run executes the plain SELECT and ignores the saved table query.

**Evidence:** Loaded the actual HEAD frontend modules with Jiti, without a listening server or UI service. Supplied the confirmed real Rust `table_script` output, `SELECT * FROM public.orders`, at the IPC boundary. Seeded table tabs with page 2 and saved filters/order and called the actual `isBrowsing` and `loadTableOnce`. Results were:

```text
SELECT * FROM public.orders       old=true new=true
SELECT * FROM public.orders;      old=true new=false, page-load IPC calls=[]
\n SELECT * FROM public.orders\n   old=true new=false, page-load IPC calls=[]
```

The old predicate is taken from `git show main:src/features/editor/runActions.ts`. This finding concerns removed normalization, not identifier quoting. The earlier `binary` allegation remains dismissed.

**Bounded fix:** Preserve the prior accepted generated-template forms when restoring and recognizing PostgreSQL table tabs. Keep SQL interpretation in Rust if normalization moves to the engine. Add a saved-tab regression for one trailing semicolon and surrounding whitespace, retaining page/filter/order settings and asserting a TablePage request rather than a script or no request.

Evidence files `/tmp/l1data-mssql-review-a-r3-ui.mjs` and `/tmp/l1data-mssql-review-a-r3-ui.log`. This is an executable frontend-logic probe with mocked IPC, not browser/native acceptance.

## Integrated fixes and verification

- Independently ran `cargo test -p engine --features mssql-tests --test mssql_transactions --test mssql_filters --test mssql_header_order`. All 14 cases passed. They cover whole-selection refusal before writes, native and inherited transaction state, all module families and trailing controls, cancellation rollback, canonical filter/count behavior and additive header identity.
- Independently ran `cargo test -p engine --features mssql-tests --test mssql_identity --test mssql_values --test mssql_refresh_paging`. All 10 cases passed. Log `/tmp/l1data-mssql-review-a-r3-checks.log`.
- Traced `txn::plan` before run/session mutation, the native state probe and moved-session/lease lifecycle, canonical connection aliases, independent catalog invalidation obligations, structured value conversions, persisted connection/context/history fallbacks, and SQL Server assistant refusal at policy and execution. No new finding is claimed in the reviewed Manual-mode planning fixes. Passing those cases does not establish safety of unrelated schema drift or all native scripts.
- The real Engine probes linked the existing pinned `libengine-72507556f0ff8bc0.rlib` and its matching `libtokio-f8caa9b061cc8de4.rlib`. An initial PG harness attempt used a mismatched Tokio artifact and failed before connecting or creating database objects. It was corrected and the completed probe is the evidence above.
- Probes used isolated temporary SQLite storage, `MemoryKeyStore`, and an empty fixed provider path. The PostgreSQL schema and SQL Server table were uniquely named and explicitly removed, with absence verified. No seeded rows were changed. No source edits, commits, subagents, PR operations, service launches/restarts, or real provider calls occurred. HEAD remained pinned and `git status --short` was empty.
- The Type System Discipline principle shaped the provenance review. A broad `TypeCategory` cannot prove the exact conversion contract for a primary key, so the review checked actual key values through the write boundary rather than accepting the category guard.

## Verification limits and decision trail

No browser/native acceptance or remote CI was performed in this review. Parent-reported broad workspace, PG46, frontend379, bridge and native results remain attributed context. The new PostgreSQL probe establishes a case outside those passing checks. No security or structural blocker beyond the concrete findings above was established. Documented bounded formatter/expression/Manual refusals, raw-money/CLR/variant refusals, TLS scope, cancellation reset and unavailable informational output are not counted as defects.

The audit records are behind HEAD. `verification.md` still lists the second-round fixes as remaining gates and its latest completed section is `d32b08b`; `review.md` still says those fixes are pending. README documents the implemented bounded Manual/header/filter policy accurately. Update the audit records with the new commits, current acceptance evidence and this review before treating the decision trail as final. This bookkeeping observation is not added to the finding counts.
