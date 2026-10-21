# SQL Server acceptance checks

## Contract

The connection owns the database kind. The editor dialect, SQL interpretation, metadata adapter and transport derive from that kind. Tabs keep their existing `DatabaseRef`. There is no independent dialect override that could disagree with a live session.

Existing saved connection JSON loads as PostgreSQL. Required generated TypeScript fields remain required. An existing connection cannot change database kind while any session can retain its previous kind. Reject that change or allocate a new connection identity.

## Observable checks

1. Open a legacy saved PostgreSQL connection. Connect, browse orders, execute a query and retain the existing transaction and assistant tests.
2. Create SQL Server connection in the real UI. Test, save, connect and reopen its saved form with the correct database kind, port and authentication details.
3. Discover shop and reporting independently. Load dbo and lab, their tables, columns, primary/foreign keys, indexes, constraints, defaults and routines. Do not mix object IDs between databases.
4. Execute a batch containing DECLARE, variable assignment, bracket identifiers, strings containing semicolons and GO text, and two SELECT result sets. Split only real GO separators and preserve UTF-16 editor offsets.
5. Read precise decimal(38,12), bigint, Unicode, null, binary, date/time, datetimeoffset and long cells. Read a truncated cell through result_cell and export exact values through CSV.
6. Page, sort, filter and count a table whose identifiers require brackets. Values contain quotes, Unicode, percent, underscore and brackets. SQL Server staged writes refuse before mutation and grid controls remain read-only. PostgreSQL pending named keys must survive catalog refresh without relabelling.
7. In SQL Server Auto-commit, issue explicit native BEGIN TRANSACTION, write a disposable row, roll back and prove the row is absent. Repeat with Commit and prove persistence. Refuse app-managed Manual requests before state changes, including restored requests. Refuse staged writes without disturbing an existing native transaction or inherited XACT_ABORT setting. Keep PostgreSQL Manual and staged-edit regressions.
8. Start WAITFOR, cancel and prove the server stops. Verify a subsequent query on the session, or show a truthful session-lost state if cancellation recovery fails. Stop must remain responsive while the server is busy.
9. Alternate PostgreSQL and SQL Server tabs without an available catalog. Syntax, completion quoting, formatting, statement selection, filters, SQL cards and history follow the tab or recorded query's database kind.
10. SQL Server assistant tools may inspect schema and suggest SQL. Model-written SQL must be refused before execution unless server-enforced read-only protection is proved. Approval rechecking remains intact.

## Verification sequence

Build a shared contract and pure dialect checks. Verify compatibility and generated bindings before migrating callers. Implement and verify database execution before UI end-to-end checks. Run workspace, PostgreSQL and SQL Server integration suites, frontend tests, typecheck and production bundle. Drive the mixed-database UI through the real bridge and check the desktop for changed sync IPC commands. Update README and the architecture reconciliation, regenerate the main-window screenshot, then obtain independent correctness and maintainability review.

## Throughput checkpoint

The dominant work is replacing PostgreSQL-specific session calls and adding SQL Server metadata, not adding editor keywords. Separate that work from frontend dialect propagation only after a shared contract is committed. Use one engine implementation worker and one frontend worker in separate worktrees. The coordinator owns shared contract integration, the real database fixtures, verification, docs and review. Reassess after the first SQL Server query streams through Engine. Avoid independent workers editing lib.rs or model.rs concurrently.

## Design rubric

- One database kind selects compatible transport and dialect with no mutable duplicate.
- Driver details stay out of console, application services and the IPC model.
- GO batches, multiple results, cancellation and real transaction state fit without session sharing.
- Text cells preserve precision and database-scoped provenance without frontend decoding.
- PostgreSQL compatibility and assistant read-only safety remain demonstrable.
- The public interface is small enough that a third adapter can be added without scattered engine checks.
