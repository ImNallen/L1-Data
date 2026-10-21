# Independent review B, round 7

Reviewed source pin: `8b78498cf55bbc8710194daaac48e472621d75aa`.

**Verdict: request changes. Counts: 0 critical, 1 warning, 0 nit.** This is one reviewer's bounded verdict, not a panel result or release approval. Root UI/native acceptance and remote CI gates remain owner-owned.

## W1 — warning: Cross-database SQL Server DDL refreshes the calling database instead of the changed catalog

**Location and cause:** `crates/engine/src/console.rs:70–81` records every catalog obligation against `self.config.db()`. `crates/engine/src/lib.rs:406–412` subsequently refreshes only that one catalog. Even `Invalidates::Both` does not broaden the catalog scope: its database-list worker at `lib.rs:433–441` reconciles slots without reloading already-ready catalogs.

**Reachable path:** public `Engine::console_run` on a SQL Server Auto-commit tab → `console::run_task` at `console.rs:362–365` → `tsql::invalidates` → `ConsoleInner::invalidate` → native execution → `finish_refreshes` → `Shared::schedule_refreshes`. R6 correctly recognizes opaque execution, but the obligation's single-database model still assumes the changed catalog is the session's database. Supported SQL Server native execution can change another database without `USE`, connection retargeting, hidden triggers, or elevated helper code.

**Independent real Engine reproduction:** With one connection, an Auto-commit console on `shop`, another on `reporting`, and both catalogs loaded, run either of these from the `shop` console (each probe uses its own unique empty table):

```sql
CREATE TABLE [reporting].[dbo].[unique_name] (id int);
```

```sql
EXEC [reporting].[sys].[sp_executesql]
    N'CREATE TABLE [dbo].[unique_name] (id int)';
```

Both runs finish `Ok`. An observer console actually targeting `reporting` reads `sys.tables` and confirms the table exists. After waiting one second for background refresh, both reproductions observe:

- `reporting` catalog version stays `2 → 2`, with the created relation absent.
- `shop` catalog version advances `1 → 3`.
- Explicit `Engine::refresh_catalog(reporting)` advances its version to 4 and reveals the relation.

The second case exercises precisely the new opaque EXEC invalidation path: recording both flags still leaves the changed catalog stale. The sidebar, completion and assistant schema context derive from that unchanged catalog/version, so they keep omitting the successfully created object until a manual refresh. The probes also submit a table-page request before manual refresh; it is accepted. **I do not claim table browsing failed or that data was corrupted.** The established defect is automatic catalog convergence.

This becomes reachable with the new SQL Server driver and native cross-database SQL. PostgreSQL cannot execute this equivalent three-part cross-database DDL, so this is not merely an unchanged PostgreSQL baseline issue. It does not challenge the intentional exclusions of SQL Server Manual mode or staged writes.

**Bounded suggestion:** Represent catalog refresh scope independently of the calling `DatabaseRef`. A conservative SQL Server policy can refresh the connection's currently tracked catalogs for metadata-changing native batches and opaque calls; retain per-slot coalescing and existing native-end settlement. Explicit targets could narrow this later, but do not rewrite/wrap/refuse otherwise-supported native SQL to avoid invalidation. Preserve PostgreSQL's existing policy. Update the architecture's console-only refresh assumption with the implemented scope.

**Regression test:** Through the public Engine, preload `shop` and `reporting`, execute both forms above from `shop`, and require automatic version/relation convergence in `reporting` without `refresh_catalog`. Include a plain SELECT control to retain the no-obligation behavior. Existing same-database opaque-call tests do not cover this scope error.

**Evidence and reproducibility:** `/tmp/l1data-review-b-r7-probe.rs` and `/tmp/l1data-review-b-r7-exec-probe.rs`; outputs in matching `.log` files. `cargo build -p engine --lib --message-format=json` passed and wrote `/tmp/l1data-review-b-r7-artifacts.jsonl`. `/tmp/l1data-review-b-r7-build-probe.py` compiles each temporary probe using exact Cargo JSON artifact paths and runs it. No guessed/stale rlib glob was used. An initial compiler invocation omitted the dependency search directory and failed before running any probe; the corrected reusable script selects it from the Tokio artifact directory.

**Cleanup:** Both cases create only unique empty tables, use a temporary Engine store/MemoryKeyStore, drop their own table through the `reporting` console, and observe native object count zero. They close both consoles, disconnect and shut down the Engine. No seeded rows, existing permissions, real provider, service lifecycle, source file, commit or PR was touched. Final HEAD remains pinned and `git status --short` is empty.

## Review bounds

Read AGENTS.md, README capability limits, architecture load-bearing decisions and reconciliation, the implementation final-scope/protocol overrides and review dispositions. Reviewed the full `main...HEAD` diff inventory and selectively traced private-driver/protocol state, conversion and query draining, T-SQL picking/classification/browse/filter construction, connection identity and persistence/history, restricted metadata and refresh lifecycle paths. Read the narrow vendor patch rather than treating unchanged upstream source as new application code.

No additional independently established correctness/security or structural blocker was found within this timebox. This is not an exhaustive claim about every changed line. I did not rerun the expensive full suites, TLS/container checks, browser/native UI acceptance or CI, and do not count parent-provided passes as my own verification. No nested agent/workflow was invoked. **Root UI/native and CI gates remain owner-owned.**
