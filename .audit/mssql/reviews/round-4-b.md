# SQL Server adversarial review B, round 4

Verdict: request changes. 2 critical findings, 1 warning, 0 nits.

Reviewed pinned b81b4e4c839dd813bbf8b4d0fa8995f615f4a6fd against main. The repository was clean before and after review. No source edits, commits, subagents, PR operations, provider calls, service/config changes, or seeded-row writes were performed. All probe writes used uniquely named disposable objects. Every probe dropped its objects, observed COUNT_BIG(*) = 0 for the names, shut down its private Engine, and removed its temporary Engine store.

Throughput checkpoint: n/a, read-only investigation. Applied poteto-mode as a single-agent investigation under the explicit no-subagents restriction. Prove It Works led to public Engine probes and observer reads. Model the Domain and Type System Discipline led to the identity/outcome fixes proposed below. Comment-audit delegation and edits were skipped under the review restrictions.

## Critical B4-1. Catalog refresh relabels staged keys and the live guard accepts a different row

Locations: src/features/editor/runActions.ts:345-369, src/state/tableEdits.ts:10-14 and :67-85, crates/engine/src/lib.rs:1143-1183, crates/engine/src/session/mssql/edits.rs:96-120.

Reachable chain: ResultGrid stages RowId.values under the current PK ordering. TableEdits stores only a positional CellValue[] for the key. A catalog refresh does not invalidate those staged edits. submitEdits -> applyEdits -> tablePrimaryKey reads the NEW catalog, and toChanges pairs the OLD positional values with the NEW PK names. Engine::table_apply resolves that same NEW catalog, full_key passes, and prepare_edits compares live metadata to the NEW relation. The native round trip verifies the value under the NEW key type. No part carries or checks the identity of the key originally selected by the user.

Real proof used a disposable table with columns id int, alternate int, note varchar(30), original PRIMARY KEY(id), and rows (1,2,'first row'), (2,1,'second row'). A real TablePage returned both original rows. The staged first-row key was ['1']. After changing the PK to alternate and refreshing the catalog, the actual frontend toChanges helper relabelled that key as alternate='1'. The exact resulting request passed the public Engine table_apply in Manual mode with Ok(ApplySummary { committed: false }). After Commit, an observer saw id=1 unchanged and id=2 changed to 'edit intended for id=1'. Both keys are exact integers; conversion loss and affected-row count do not explain or prevent this wrong-row update. The positional frontend representation predates this feature; the finding is that the new SQL Server identity guard cannot enforce its promised original identity because that identity never reaches it.

Evidence: /tmp/l1data-mssql-review-b-r4-key-identity.rs and .log; /tmp/l1data-mssql-review-b-r4-frontend-key.log. The frontend log was produced by executing the repository's actual setCell and toChanges exports after stripping types, without modifying them.

Bounded fix: capture named key cells and the original table/PK identity with the staged edit, and carry that identity through TableChanges. Validate it against live metadata under the edit lock. At minimum refuse a submit when the originating PK identity differs, instead of remapping positional values. Retain explicit provenance for already-staged edits even when a refresh makes the grid non-editable. A regression must refresh the catalog after PK replacement before Submit; the existing identity-drift test checks the stale-catalog case only.

## Critical B4-2. An opaque trigger can commit Manual work and replace the transaction at the same depth

Locations: crates/engine/src/sql/tsql/transaction.rs:31-39, crates/engine/src/console.rs:298-323, crates/engine/src/session/mssql/edits.rs:81-120. The related staged-edit path is crates/engine/src/lib.rs:1182-1227.

Reachable chain: the Manual planner accepts UPDATE as BeginIfIdle and refuses opaque EXEC only when it appears in selected text. It does not account for the enabled DML trigger invoked by that UPDATE. run_task opens BEGIN, executes UPDATE, and trusts the final observed depth/state. The edit guard likewise holds an ordinary transaction lock that server-side trigger code can release by ending that transaction.

Real proof used an AFTER UPDATE trigger on a disposable table, with this body:

```sql
BEGIN
  WHILE @@TRANCOUNT > 0 COMMIT TRANSACTION;
  BEGIN TRANSACTION;
  BEGIN TRANSACTION;
  UPDATE dbo.[disposable_audit] SET n=99 WHERE id=1;
END
```

A normal Manual UPDATE of the target from n=1 to n=3 passed preflight, finished RunOutcome::Ok, and reported TxnState::Active. console_rollback returned Ok. An independent Auto-commit observer then saw target n=3 still persisted and audit n=1. The original update was committed, while Rollback undid only the replacement transaction. The final nesting probe cannot distinguish these transactions.

The same trigger through table_apply committed target n=4 before returning a native count-mismatch error. Its subsequent undo did not reverse that committed target update. A simpler trigger that executes ROLLBACK and then updates the audit table also committed the audit write after the wrapper ended. Those facts are not fixed by a table X lock or exact affected-row capture.

Evidence: /tmp/l1data-mssql-review-b-r4-reopen.rs and .log contain the silent successful Manual-run case and staged-edit case. /tmp/l1data-mssql-review-b-r4-probe.rs and .log contain the rollback-then-write variant. Each probe completed and verified cleanup.

This is distinct from the documented refusal of explicit mixed scripts or opaque EXEC. The submitted text is an ordinary UPDATE that the supported Manual path accepts. README promises ordinary Manual batches are wrapped, and the task requires them to remain rollbackable. There is no documented trigger exception. The prerequisite is a pre-existing trigger whose body controls transactions, not arbitrary transaction control in the selected editor text.

Bounded fix: include invoked server-side code in the supported Manual/staged-edit boundary. Refuse relevant opaque trigger paths before executing any selected mutation unless their transaction behavior can be established under a stable guard. Keep arbitrary native behavior available through Auto-commit. Checking only post-execution @@TRANCOUNT or XACT_STATE cannot fix this case because the replacement transaction has the same depth and active state. If trigger-driven transaction control is intentionally outside the feature's guarantees, that is an explicit contract change requiring owner acceptance; it is not covered by the current documented refusal.

## Warning B4-3. An unknown or committed staged-edit outcome is displayed as "Nothing was saved"

Locations: crates/engine/src/lib.rs:1218-1222, src/ipc/errors.ts:66-68, src/features/editor/runActions.ts:369-374.

Reachable chain: table_apply detects a failed undo and returns ApplyError::Transaction with 'Commit outcome may be unknown'. submitEdits passes it to applyErrorText, which unconditionally prefixes both Transaction and Failed with 'Nothing was saved.' The wire type has no outcome discriminator to force the frontend to distinguish a confirmed rollback from an unknown result.

Real proof: the rollback-trigger Engine probe returned Transaction with 'rollback also failed ... Commit outcome may be unknown' while an observer saw audit n=99 persisted. Executing the actual applyErrorText export produced:

> Nothing was saved. Staged edits failed; rollback also failed: The ROLLBACK TRANSACTION request has no corresponding BEGIN TRANSACTION.. The session was retired. Commit outcome may be unknown.

The commit/reopen variant also returned Failed even though the target edit itself persisted. The wording can encourage a retry of non-idempotent inserts or trigger effects.

Evidence: /tmp/l1data-mssql-review-b-r4-error-text.log and the Engine logs above.

Bounded fix: represent confirmed rollback versus unknown outcome in ApplyError and render it exhaustively. At least remove the unconditional 'Nothing was saved' prefix for outcomes that have not established rollback. Preserve staged data for review, but tell the user to verify persisted state before retrying.

## Independent verification and limits

Read AGENTS.md, architecture load-bearing decisions and SQL Server reconciliation, README limits, and .audit/mssql/{implementation,verification,review}.md. Traced immutable-kind persistence, the PostgreSQL native query/source path, assistant refusal at policy and execution, table provenance, edit conversion/locking, Manual planning and cancellation retirement. The private driver split and focused T-SQL modules do not earn an additional structural finding merely for their size.

Ran current-source cargo build --locked -p engine --message-format=json. Every disposable Rust probe was compiled with the exact Engine, Tokio, futures-util and UUID artifacts from that Cargo JSON output, rather than guessed older rlibs.

Passed independently:

- cargo test -p engine --features mssql-tests --test mssql_edit_drift -- --test-threads=1 --nocapture. All 14 cases passed, including stale numeric/temporal keys, codepages, warning-disabled truncation, validation-before-write, identity drift with a stale catalog, live locks, concurrent DDL, and cancellation releasing the edit lock.
- cargo test -p engine --features pg-tests --test pg table_page_preserves_nested_domain_native_metadata_and_primary_key -- --exact --nocapture. Passed. Creates and drops its own schema and domains.
- cargo test -p engine --features mssql-tests --test mssql cancelling_waitfor_retires_the_session_and_stops_the_server_request -- --exact --nocapture. Passed.
- Public Engine trigger probes, including a successful ordinary Manual control update/rollback before installing the trigger, and observer reads after the adversarial paths.
- Public Engine PK-replacement/catalog-refresh probe and actual frontend staging/serialization helper execution.
- Bounded procedure-prefix probes. BEGIN/IF/WHILE wrappers around omitted-EXEC calls failed with native syntax errors; labels and leading-semicolon bare calls were refused. No bypass claimed for those inputs. Artifacts are /tmp/l1data-mssql-review-b-r4-prefix.rs and .log, with verified cleanup.

Did not rerun full suites, browser/native UI, real providers, remote CI, or TLS smoke. The frontend helper proofs are execution of the actual functions, not a complete UI automation pass. The SQL Server trigger prerequisites are deliberately adversarial database objects, and their cleanup was verified. Existing passing suites are not treated as proof of these untested properties. No additional code-quality nits were forced.
