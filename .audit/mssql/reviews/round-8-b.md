# Independent review B, round 8

Reviewed source pin: `63acd96127fab0e0178bb61232f71e06afb79d0e`.

**Verdict: approve code within this bounded review. No findings: 0 critical, 0 warning, 0 nit.** This is my independent result, not a panel result or release approval. **Root UI/native acceptance and remote CI gates remain owner-owned.**

## Scope and source review

Read AGENTS.md, README capability limits, architecture load-bearing decisions and implementation reconciliation, `.audit/mssql/implementation.md` final-scope/protocol overrides, and `.audit/mssql/review.md` dispositions. Reviewed the complete `main...HEAD` change inventory and selectively traced the changed execution/session facade, private SQL Server driver, conversion, metadata, SQL construction, classification, refresh settlement, connection-kind guards, persisted/history/result kinds, and frontend target/browse integration. I reviewed the narrow vendor patch and relevant upstream response-draining context, not 38k unchanged upstream lines as new application code.

Particular attention went to the R5–R7 corrections:

- Native console execution observes protocol state without follow-up SQL. Conversion failures consume the original response; cancellation and abandonment retain the client-retirement boundary.
- Refresh obligations retain the original database and native-end policy. SQL Server catalog obligations fan out through existing tracked-slot gates on the original connection. `SELECT INTO` and opaque invocation recognition preserve SQL text rather than wrapping or rewriting execution.
- Optional type/expression description remains distinct from readable object identity. The alias joins and routine fallbacks retain structure while preserving unsupported-value refusal.
- Manual configuration/execution and staged SQL Server writes refuse at engine boundaries; PostgreSQL keeps managed execution and original named pending key cells.

I found no additional actionable correctness/security finding or demonstrated structural blocker in the inspected paths. The private-driver/dialect decomposition and removal of unsupported write machinery avoid a second SQL Server write implementation. The large `lib.rs` predates this feature (1,650 lines on main); the change does not newly cross the 1,000-line threshold. This observation is not a claim that every existing orchestration path is ideal.

## Independent real Engine checks

`cargo build -p engine --lib --message-format=json` succeeded. `/tmp/l1data-review-b-r8-build-probe.py` compiled the temporary public-API probe using exact Cargo JSON artifact paths from `/tmp/l1data-review-b-r8-artifacts.jsonl`; no stale rlib glob was used.

`/tmp/l1data-review-b-r8-probe.rs` passed against the shared SQL Server fixture, with an isolated temporary Engine store and MemoryKeyStore:

1. A `shop` console created a unique empty table through a three-part `reporting` name, then opened a native transaction in the same batch. The transaction was `Open`. The Rollback button ended that transaction; an observer confirmed the earlier table creation remained committed. Both tracked catalogs refreshed automatically, and the reporting catalog contained the relation (reporting version `2 → 4`, shop `1 → 3` on the final run).
2. A zero-row UPDATE of a probe-owned temporary table followed by a separate Run containing `IF @@ROWCOUNT>0 DELETE` retained both rows. Native rowcount-dependent branching was preserved.
3. `RAISERROR(...,16,1)` followed by BEGIN, an UPDATE of that temporary table and a SELECT finished as `Failed`, returned the later rows, retained a usable session and reported `Open`. Rollback restored the original temporary values. The expected error did not suppress the native batch tail or fabricate an idle transaction.
4. Manual configuration and a staged insert into the unique reporting table both returned `Unsupported`. Console revision and Auto-commit mode remained unchanged, and an observer confirmed the table still had zero rows.

The final probe exits zero. Its observed output is `/tmp/l1data-review-b-r8-probe.log`. An earlier run of the same probe before adding the boundary assertions also passed and completed cleanup.

The local archive verification also passed: `python3 vendor/tiberius/verify-upstream.py /Users/nallen/.cargo/registry/cache/index.crates.io-1949cf8c6b5b557f/tiberius-0.13.0.crate`. It verified the SHA-256, 96 unchanged files, and the exact seven-file, 210-line declared patch. Output: `/tmp/l1data-review-b-r8-vendor.log`.

## Cleanup and limits

Both probe runs dropped their own unique reporting table and observed native object count zero. The temporary table was dropped; both consoles closed, the connection disconnected, and the Engine shut down. Only temporary scoped probe/build/report files were written. No seeded rows, existing permissions, source files, services, real providers, commits or PRs were touched. Final HEAD remained pinned and `git status --short` was empty.

This was a bounded adversarial review, not an exhaustive audit of every changed line. I did not rerun the full workspace/database suites, TLS containers, browser/native UI acceptance or remote CI. Parent-provided results are context, not independently claimed checks. Restricted-login scenarios were reviewed in source and existing regressions, not newly reproduced here. No nested agents or review workflow were invoked. **Root UI/native and CI gates remain owner-owned.**
