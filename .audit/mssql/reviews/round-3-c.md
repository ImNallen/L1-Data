# Independent SQL Server review C, round 3

Revision: `24200b1bab2fdf49293b064d6bced18b2be2c262` (`main...HEAD`). Reviewed the integrated change against the stated SQL Server scope, `AGENTS.md`, the architecture load-bearing decisions and implementation reconciliation, README limits, `.audit/mssql/{implementation,verification,review,acceptance}.md`, and all six prior review reports. No source edits, commits, PRs, subagents, service restarts, provider-account calls, or seeded-row writes.

## Intent

Add SQL Server alongside PostgreSQL. Each saved connection must select the private driver and dialect across editor, execution, generated SQL, metadata, transactions, results, history, and assistant behavior, while preserving older PostgreSQL data and workflows. The initial SQL Server scope includes SQL login over TCP, encrypted VerifyFull/Require, GO batch scope, exact string-or-null values, bounded formatter and raw expressions, session retirement on cancellation, explicit refusal for raw money/CLR/sql_variant, and disabling model-written SQL before approval and execution.

## Verdict

**Changes required: 1 critical, 0 warnings, 0 nits.**

## Findings

### 1. [critical] Staged edits can silently corrupt values written to `varchar`/`char`/`text`

**Location:** `crates/engine/src/sql/mssql_value.rs:22-27`; parameter planning in `crates/engine/src/session/mssql/edits.rs:8-12, 33-38`; commit and row-count handling in `crates/engine/src/lib.rs:1181-1214`.

**Finding:** The SQL Server staged-edit path binds Rust strings as Unicode parameters, then returns the parameter unchanged for `varchar`, `char`, and `text` columns. SQL Server implicitly converts that Unicode value to the target non-Unicode code page. The engine does not check whether the text survives that conversion or whether it fits the target byte length. The only explicit capacity check in this branch covers `nchar` and `nvarchar`.

**Call chain:** A grid edit becomes `TableChanges` → `session::mssql::edits::parameter` records the string as a TDS `nvarchar` parameter and calls `input_sql` → the generated `UPDATE` assigns that parameter to the `varchar` column → SQL Server may replace unsupported characters or truncate overlength values → `execute_edit` reports the affected row count → `table_apply` sees one affected row and commits. It verifies that the row count is one, not that the stored text equals the requested text.

**Executed evidence:** I built a disposable harness against the pinned Engine source and the already-running local SQL Server, then exercised `Engine::table_apply` through its real public API. Both cases below returned `Ok(ApplySummary { committed: true })`:

- A disposable `varchar(20) COLLATE SQL_Latin1_General_CP1_CI_AS` cell set to `漢字` was stored as `??`, even with the default warning setting.
- On the same tab after `SET ANSI_WARNINGS OFF`, a disposable `varchar(3)` cell set to `abcdef` was stored as `abc`.

These settings persist on a tab session, so the second case is reachable after ordinary SQL execution and does not require manipulating the staged-edit request. The current overflow test covers default `ANSI_WARNINGS ON`, where SQL Server rejects the write; it does not cover this session setting or lossy Unicode conversion.

**Suggestion:** Validate the actual target-column conversion before committing. Reject a staged value if converting it under the target column's collation/code page and size does not preserve the intended value. Keep the validation independent of `ANSI_WARNINGS`; add real SQL Server regressions for unrepresentable Unicode and overlength `varchar` with warnings off. A successful one-row count must not be treated as proof that a text value survived conversion.

## Review notes and verification limits

- Independently traced the whole-batch Manual planner, transaction-only batches, module definitions, native transaction depth probes, and error/cancellation retirement. I found no additional reachable transaction escape in the integrated fixes during this pass; the parent-reported suite results are context, not independent verification.
- Inspected the SQL Server value converters, edit lowering, source-provenance checks, database identity resolution, saved JSON and history migration, PostgreSQL session adapter and assistant refusal at both policy and execution boundaries.
- The scratch harness source and build artifacts were kept in `/tmp/l1data-mssql-review-c-r3-probe`, outside the repository. It created only unique disposable tables in `shop`, dropped them successfully, and a final server query found zero matching rows in `shop.sys.tables` and `tempdb.sys.tables`.
- The repository working tree remained clean. I did not rerun workspace/database/frontend suites, drive browser or native UI acceptance, or verify remote CI. No seeded rows were changed.
