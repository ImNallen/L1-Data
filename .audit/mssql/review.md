# SQL Server review decisions

Final application pin is`f70c2a9dff4d372eebd181e2aa7b4142b39d7565`. R1 through R10 findings have accepted fixes or explicit capability removal. All three fresh R11 reviewers approve their bounded scopes with zero findings. Root browser and rebuilt WKWebView insertion acceptance pass, the screenshot is regenerated, and UI fixtures are verified absent. All reviewers and workers are explicitly stopped before final commit/push. Remote CI remains open. The sections below retain each earlier verdict and response as history.

The feature adds SQL Server alongside PostgreSQL. A saved connection kind selects its driver and dialect. Existing PostgreSQL persistence, database-scoped catalogs, exclusive sessions, engine-owned SQL and rows, revision snapshots, and assistant read-only guarantees remain required.

## Final routine-target correction and R11 review

R10 C's mixed-kind template insertion warning was accepted and independently reproduced in the actual browser. Commits8435836/f70c2a9 pass the source DatabaseRef through both normal and filtered tree paths. The shared handler checks current saved source and destination kinds before any editor transaction. Missing targets or connections and kind mismatches show an error. Matching kinds retain verbatim templates, including insertion across separate connections/databases. No target changes or SQL rewriting occur in this boundary. Red/green behavior, undo preservation, full453-test frontend pass and actual browser/native acceptance are recorded in routine-target-implementation.txt and evidence/routine-target-ui-f70c2a9.txt.

Fresh R11 Astra, Sol and Luna reviewers each received the original brief, all prior findings/responses, the full feature context and current pin. A independently passed85 focused tests and five handler probes. B passed81 focused tests and five probes. C passed20 insertion tests. These overlapping counts are not added into a coverage total. Each reports0 critical/0 warning/0 nit, with bounded scope and explicit limits. The root does not infer approval of every line from these reviews. All three were explicitly stopped after their reports.

The first independent panel reviewed `184289f5ca8c980ab60d81a426eb5d95999edb26` against `main`.

| Reviewer | Model | Findings |
|---|---|---|
| A | gpt-6-astra | One critical and three warnings. |
| B | gpt-6.1-sol | Four warnings. |
| C | gpt-6-luna | Three warnings. |

## Act on

| Finding | Reviewers | Reason and response |
|---|---|---|
| Temporal staged keys depend on DATEFORMAT. | A | A real committed update changed the wrong date-keyed row. Use explicit canonical conversions and real round-trip regressions. |
| Structured binary filtering compares displayed hex as Unicode text. | A, C | Filtering by a selected cell can return no matching row. Resolve column metadata and share typed canonical conversion with staged writes. |
| Rowversion primary keys are rejected as write values. | B | Generated columns must remain non-writable, while their existing values may identify rows for update/delete. |
| Accepted database spelling diverges from the home identity. | A, B, C | Initialization and restored/default targets must use the server-authoritative home. Resolve only the exact locator accepted by that connected instance. |
| Older PostgreSQL table templates stop browsing after restore. | A | Preserve the earlier persisted template spelling in the engine. Commit `3cfd3d0` has the Rust contract regression and frontend page/filter regression. The live UI restored page 2 with its filters and sort. |
| Case-variant sort names duplicate primary-key tiebreakers. | B | SQL Server refuses otherwise valid paging on case-insensitive databases. Preserve deterministic paging without assuming identifier case equivalence. |
| One batch classification loses catalog refresh obligations. | B | Database-list and catalog refreshes are independent obligations. Request both conservatively for database-level DDL in a batch. |
| T-SQL combines several behavior surfaces in one oversized module. | C | Split shared lexing, expression/formatting, and table SQL into focused modules while preserving behavior. |

## Consider, noted, and dismissed

No reported finding was dismissed. C's bracket-qualified assistant describe-tool suspicion was explicitly unverified and is not counted as a finding. A's pre-existing configuration-lock race was outside this feature's accepted change boundary and was not promoted to a new defect.

## Agreement map

All three reviewers identified the database home-name boundary. A and C independently identified binary filtering. The temporal wrong-row defect and the four other behavior gaps were single-reviewer findings backed by concrete probes or source call chains. Passing suites at the reviewed revision were insufficient evidence for those cases. The review gate remains closed until the fixes and focused independent review pass.

## Second review at d32b08b

Fresh A/B/C tasks reviewed the integrated first-round fixes. A reported one critical and two warnings. B reported two warnings. C reported one warning.

Act on A's proved Manual-mode auto-commit defect. Transaction tokens inside unexecuted module definitions or conditional branches incorrectly suppress implicit BEGIN. Model whole-batch transaction behavior and refuse ambiguous mixed or opaque execution in Manual mode before any selected batch runs.

Act on A/B's supported text/ntext/XML equality failure and B's temporal text-search mismatch. Assignment conversion is not a predicate capability. Normalize comparable values and searchable canonical displays in Rust, and verify both pages and counts against actual returned cells.

Act on A's additive header-sort identity failure. A valid raw `ID DESC` can become duplicate `[ID] DESC, [id] ASC` on a CI database. Supply the table context and use exact catalog identities for header edits. Refuse unresolved additive names without rewriting the user's text. Preserve distinct CS `ID` and `id` columns.

Dismiss C's claimed legacy `binary` template mismatch. The old frontend reserved set contains `binary`, and executing the actual old function returns `"binary"`. An actual comparison of all 101 legacy keyword templates against the current Rust IPC service found no differences. `.audit/mssql/legacy-keyword-templates.json` records the expected outputs. A independently compared the old and new keyword sets and found no additions or omissions. C's native temporary-table probe established that quoting is needed; it did not establish that the old generator omitted it.

The second-round fixes are integrated in `3675a8e`, `ccca7bb`, `39e9e69` and `24200b1`. All affected Engine regressions and the full combined local gates passed. The original UI failures were exercised again in the browser and native app after the fixes.

## Third review at 24200b1

Fresh A/B/C tasks received the full feature brief, every prior finding and response, and the same rubric. A found one critical and two warnings. B found two warnings. C found one critical. The merged set has two critical findings and three warnings; all are accepted. No new Manual-planner defect was established.

| Finding | Reviewers | Response |
|---|---|---|
| A stale decimal key type rounds a selected key to another existing row after external DDL. | A | Reject cached type plus affected-row count as proof of identity. Validate live object/type/PK metadata and lossless keys under a lock retained through the writes. Implemented in `2a21153`; real Engine regressions and full integrated suites pass. |
| SQL Server non-Unicode assignments can replace characters or truncate with ANSI_WARNINGS OFF. | C | Validate real codepage, collation and capacity conversion under the same staged-edit guard before any mutation. Implemented in `2a21153`; real Engine regressions and full integrated suites pass. |
| Stable PostgreSQL nested domains fail the new shared metadata check. | A | `f30f169` keeps PostgreSQL's native result types and sources; synthetic table provenance applies only to SQL Server. The real TablePage/edit regression and full combined suite pass. |
| Restored PostgreSQL browse text with whitespace or one trailing semicolon stops browsing. | A, B | `f30f169` preserves the earlier accepted forms through engine-owned `TableScript.acceptedTexts`. Saved-tab page/filter/order tests and stale-setup guards pass; saved-tab UI recheck passes with page/filter/order and editing retained. |
| SQL Server cancellation claims that previously committed work rolled back. | B | `0b48968` replaces the loss boolean with an explicit unknown outcome. Real persistence, toolbar tests and the browser recheck pass; native recheck now passes. `f288312` updates cancellation notices. |

The root-cause premise failed in two independent real Engine probes: cached types and one affected row do not establish that a write targeted the displayed key or preserved the entered value. Fresh validation must remain authoritative through execution, including external DDL and session options. The edit worker's before-fix Engine tests reproduce decimal wrong-row updates, Unicode replacement and warning-disabled truncation. No final approval is claimed. Reports are `/tmp/l1data-mssql-review-{a,b,c}-r3.md`; final dispositions and repeatable regression evidence will be added after integration.


## Integrated third-round fixes

`2a21153` adds live table identity/type/PK validation, native original-key round trips and codepage/capacity checks inside the existing edit transaction, under a table lock held through writes. All 14 dedicated cases pass. They reproduce the stale decimal and temporal wrong-row failures before the fix, then prove refusal before any mutation, including nonrollbackable sequence sentinels. They cover refreshed-catalog old keys, Unicode/codepages, warnings-off truncation, native-family positives, outer-transaction preservation, empty-table locks and waiting DDL. `b81b4e4` additionally proves cancellation releases the edit lock and rolls back its still-open write while retaining earlier committed work. The blocking cost and savepoint lock retention are explicit in README and architecture.

The coordinator reran the combined workspace suites (PG47, MSSQL52), all 393 frontend tests, typecheck, production/native builds, both bridge smokes and both desktop self-checks. All passed. The actual browser refused an unrepresentable grid value without changing its persisted row; native Submit saved a representable value on that same disposable table. Native cancellation displays session reset and unknown-outcome guidance. The fixture was dropped and absence verified. The main screenshot was regenerated and inspected.

`05c5097` adds a rerunnable public-Engine TLS probe. The coordinator reran it: trusted localhost VerifyFull passes; an IP name mismatch and unrelated CA fail as TLS certificate errors; Require controls pass. Process-local roots and a disposable isolated SQL Server leave shared services and user trust stores untouched. `01ff210` adds this check to Linux CI. No remote result is claimed yet.

The final application-code review is reopened on `b81b4e4`. Earlier reviews are not counted as approvals. The focused comment audits accepted the public comparison contract and native lock constraint, found no suppressions or reshape flags, and deleted one narrating TLS-helper docstring. The independent trail review found four evidence/tradeoff flags; its dispositions are recorded separately. Fresh code review, final trail reconciliation and remote CI remain open.

## Round 4 reopens the write contract

Pinned b81b4e4 received independent request-changes reports. A and B independently reproduced the same critical positional-key relabel after a primary-key replacement and catalog refresh. Parent then reproduced it in the actual browser, staged on id=1, clicked Submit after the refresh, and observed id=2 changed. The named-key fix retains original column/value cells and refuses stale keys without discarding pending data. The final SQL Server read-only grid boundary does not replace that PostgreSQL safety correction.

B also proved a critical ordinary Manual UPDATE invoking a trigger that commits the original transaction and opens a replacement. Run finished Ok/Active and Rollback succeeded, but an independent observer saw the update persisted. The same path defeated atomic table_apply. B's warning showed the frontend saying Nothing was saved for a genuinely unknown or already committed outcome. 70b5806 adds OutcomeUnknown, removes the unconditional claim, and tells the user to verify saved data before retrying. A parent current-source probe observed OutcomeUnknown with the trigger's audit value persisted and verified cleanup.

C's bounded finding concerns inherited XACT_ABORT ON: an edit constraint failure can erase earlier Manual work rather than preserving it through a savepoint. Its report includes an invalid description of a nested model panel. That paragraph is excluded from review evidence and contributes no approvals. The parent stopped C and any children. The strongest-judgment boundary investigator independently reproduced the XACT_ABORT behavior and the trigger findings through current-source Engine probes.

The investigated premise was that wrapping selected SQL and checking final depth/state guarantees rollbackability. It is false. A replacement transaction can have the same depth and Active state. Cascades invoke a child trigger even when the directly updated table has none. Database DDL triggers also commit wrapped module definitions. Restricted roles can execute triggers while metadata visibility returns none. SERIALIZABLE, XACT_ABORT ON and savepoints do not prevent the commit/reopen path. The accepted fix is to remove app-managed SQL Server Manual mode and staged grid writes from the first-release capabilities. Engine boundaries refuse these requests before mutation. Auto-native scripts and explicit transaction commands remain supported. PostgreSQL retains managed transactions and staged writes with original named keys.

These findings remain open until the narrowed implementation, integrated checks and fresh independent review complete. Prior broad passes do not approve the changed contract.

## Round 5 results

All three independent reviewers requested changes at98301df. Astra found one critical native-semantics issue. Sol found one metadata warning. Luna found one stale-catalog warning. There were no reported structural nits or code-quality blockers. The panel accepted the narrowed SQL Server capability contract; these findings concern retained native execution and metadata support.

Act on A. The on-session transaction SELECT overwrites @@ROWCOUNT between GO batches or separate runs. A zero-row UPDATE followed by a guarded DELETE executes that DELETE through Engine while the raw driver preserves the row. Parent independently reproduced surviving_rows=0 in the actual native SQL editor, then dropped the local temp table and observed OBJECT_ID NULL. The private observation boundary must preserve native session semantics. A driver/protocol investigation and fix are isolated; wrappers are not assumed sufficient because the reviewer's wrapper/RPC controls still returned1.

Act on B. SELECT-only SQL Server logins can read a table while native default/computed/CHECK definitions are NULL. Requiring these fields fails all table detail and assistant describe. Treat legitimate hidden expression text as unavailable metadata while retaining useful relation information and visible owner definitions. A focused driver-boundary fix and restricted-login regression are isolated.

Act on C. Owed SQL Server DDL invalidations can include earlier committed native work, so clearing them after Rollback or session loss can leave a committed object absent from the catalog. Conservatively refresh accumulated obligations when native transactions end or retire, keeping PostgreSQL confirmed-rollback behavior. A focused lifecycle fix and observer/version regressions are isolated.

No finding is dismissed or counted as approved. The broad tests and UI acceptance above remain historical evidence at their pins. Fresh integrated checks and independent review must follow these corrections before the PR.


## R5 fixes and fresh review gate

All R5 findings are accepted. c465ddc/a451ab0 preserve available restricted-login metadata while omitting hidden expression text. c751f0c captures each pending refresh's original database and PG commit-only versus SQL Server native-end policy. 4145a52 replaces native startup/per-request SQL probes with a protocol-only driver getter; 7ebc2f0 integrates Open/unknown depth, metadata-only canonical DB_NAME lookup, canonical console parameters, and a reproduced PG typed-ROLLBACK invalidation correction. The busy release signal retains actual-client cleanup lifetime. No SQL/RPC wrapper or lexical refusal substitutes for native state preservation.

The final comment audit deleted two lines. Its internal lifecycle naming flag is addressed by naming the Busy watch session_release. The shared watch remains the actual release signal, and the detached Attention test proves its sender outlives the client. No extra lifetime API or workaround comment was added. Vendor archive verification proves 96 included upstream files byte-identical and all seven modified files exactly recorded by UPSTREAM.patch.

The combined workspace/PG/MSSQL suite, frontend types/build, both bridge smokes, both native self-checks and five live TLS cases pass. Final desktop proof and fresh independent A/B/C review at 7ebc2f0 remain gates. Earlier passing runs do not approve this revision.


R6 C reports one proved warning. EXEC can create an object without refreshing the cached catalog. Accepted. An isolated worker is adding conservative catalog and database-list obligations for opaque explicit and implicit procedure execution, without changing native SQL. Other R6 reviews continue on their original pin. The gate remains closed pending the fix and a fresh review of the final source.


R6 A and B report the same hidden-alias-type metadata root cause. Accepted. A readable table can lose a column through the required declared-type join, and an executable procedure parameter can make the whole catalog fail on nullable TYPE_NAME. A fresh isolated worker owns independent native/base type fallback and identity preservation, with restricted/owner Engine controls. B's sp_rename warning is the same opaque execution invalidation gap already accepted from C and is included in that worker's regressions. The merged R6 set is two warnings and no critical findings; all are being fixed.


417fd8b integrates the opaque execution correction. One significant-token predicate recognizes explicit EXEC/EXECUTE and implicit first-statement procedure calls. It reuses the existing statement registry, records both refresh obligations and keeps write history consistent. The six public Engine cases reproduced stale metadata before the fix and pass afterward, including dynamic execution, database-list changes, native commit/rollback, and table/column sp_rename with successful TablePage reads. The extra work for read-only opaque calls is conservative; hidden trigger/UDF effects without an opaque invocation are not newly inferred. No SQL is rewritten and no observation query is added. Alias metadata correction and fresh final review remain open.


9620e5a integrates the hidden-alias metadata correction. Columns do not depend on visibility of their declared type row. Routine annotations preserve available alias names, otherwise use the authoritative system name or explicit unknown. Two public restricted-login regressions reproduced the prior missing columns/catalog failure and now pass with owner controls and exact TablePage/native values. Unsupported conversion boundaries and IPC shape are unchanged.

The full combined workspace/PG/MSSQL suite passes after both R6 corrections. The final comment audit reports zero added prose/suppressions/flags.8b78498 reconciles the architecture and final screenshot. All original R6 reviewers are stopped. Fresh independent A/B/C reviews are requested for the final pin; no earlier requestchanges is counted as approval.


R7 B establishes a remaining invalidation-scope premise error. SQL Server three-part DDL and EXEC in another database can change a tracked catalog other than the calling DatabaseRef. Accepted. A scoped worker is moving SQL Server catalog invalidation to the connection's existing tracked slots, preserving per-slot coalescing, native-end settlement and PostgreSQL's current database scope. No global database opening, SQL rewrite or lexical ban is proposed.

R7 C reports no blocking findings and one maintainability nit. The current MSSQL prompt is correct but is derived by fragile exact prose replacements. Accepted for a bounded explicit-resource selection cleanup with rendered-dialect tests. It does not reopen model-query execution. R7 A continues against its original pin; neither report is counted as final source approval after these changes.


R7 A adds one accepted warning. SELECT INTO creates catalog objects but previously recorded no refresh obligation. b2e97e6 records this effect through the existing pure dialect predicate, retaining nested SELECT scope while excluding INSERT/OUTPUT INTO. The new unit and both public Engine tests failed before the fix. All eight opaque/SELECT INTO Engine cases pass afterward, with native commit/rollback, TablePage values and ordinary-read controls. Cleanup independently verifies no disposable objects remain.

Integrated R7 corrections are 2647770 (explicit static prompts), b2e97e6 (SELECT INTO), and e1bb43d/c4e6fa4 (tracked-slot refresh scope). The cross-database worker reproduced all three failures before fixing them, then passed direct and opaque CREATE/ALTER/DROP, retirement/retargeting, second-connection isolation, untracked-database and PostgreSQL controls. The root reviewed each production diff. An initial worker outcome test timed out once; its isolated rerun and two complete subsequent outcome runs passed, with cause unestablished. Full integrated gates and fresh independent review remain open.


R8 B (gpt-6.1-sol) approves within its bounded review at 63acd96 with zero findings. The reviewer independently proved preserved native rowcount branching, native batch continuation after RAISERROR, Open transaction handling, post-rollback cross-database catalog convergence, and pre-mutation capability refusals. Vendor archive/delta verification and probe cleanup passed. The root read the full report and explicitly stopped the reviewer. A and C remain pending; no panel or release approval is claimed.


R8 C (gpt-6-luna) requests changes for one accepted warning. SQL Server TablePage runs in the tab session, but exact filtered counts run on metadata and can disagree under DATEFIRST. A public Engine reproduction returned the Monday row with DATEFIRST1 and count0. The root traced exact_row_count, Last page, session ownership and the generated command callers. Refinement uses the existing moved-exclusive-session contract for SQL Server counts, with required tab identity plus expected DatabaseRef guard. PostgreSQL metadata counts stay unchanged. Rejecting raw WHERE counts would hide one symptom while missing other session context; introducing a new LastPage run request would unnecessarily change result/pagination contracts. An isolated worker owns the count-context migration and behavioral proof. R8 A still reads unchanged 63acd96.


R8 A (gpt-6-astra) independently approves within its bounded review at 63acd96 with zero findings. It ran 25 focused integrations and two additional Engine probes with verified cleanup. The root read the complete report and explicitly stopped all three R8 reviewers. The panel result is two bounded approvals plus C's accepted count-context warning. It is not final approval for the forthcoming count correction.


The R8 count correction is integrated in a3b1add. The root inspected the production/session/IPC/frontend changes before cherry-picking the clean isolated commit. SQL Server exact counts now borrow the tab's existing exclusive session and preserve settings, uncommitted rows, prior results and history. The task follows the existing Busy/orphan/return-session ownership contract, including caller abandonment and native-end refresh settlement. The required TabId plus expected DatabaseRef guards setup races. All callers and generated bindings migrated together. PostgreSQL retains metadata-session semantics. Last's captured context guards the reply through subsequent page setup and acceptance. Worker checks pass 315 workspace, 17 targeted SQL Server, four PostgreSQL controls, 418 frontend, bindings/types/build. Root integrated gates, actual Last-button acceptance and fresh review are underway. The code is not yet approved after this change.


R9 C (gpt-6-luna), the model that found the count mismatch in R8, reports zero findings against a3b1add. It reran all six real count integrations with verified cleanup and traced the new ownership/context paths. The root read the complete report and explicitly stopped the reviewer. Root actual browser and native WKWebView Last-button acceptance now pass with exact total1001. A/B remain pending and remote CI is separate.


R9 A and B independently reproduce the same count-activity gap. The moved count owns Busy while its snapshot still exposes the prior Finished run, so Run remains visible, Esc dispatches no cancellation, and later page actions bypass the queue. Direct Engine cancellation works. Accepted. The refinement derives typed active work from SessionSlot::Busy into the revisioned snapshot, preserving old results/history while making Count cancellable and queue-aware. It also records latest pending navigation intent and prevents Cancel from draining stale queued work. No frontend timing guess or fake RunState is allowed.

R9 B additionally proves SQL Server routine rows still reach the old PostgreSQL-only bare name() insertion. Parameterless scalar/procedure controls execute only with schema qualification and native EXEC syntax. Accepted. A separate isolated slice gives FunctionInfo Rust-owned insertion text from structured catalog identity/parameters and native quoting, consumed in both normal and filtered FunctionRow paths. PostgreSQL behavior is retained. Templates with arguments require user-supplied values; insertion does not execute SQL. This removes frontend SQL construction without adding a second command lane. Both fixes are isolated at a3b1add; all R9 shared reviewers are explicitly stopped.


The routine slice 2d455b7 is accepted after root production-diff inspection. It adds one required nonpersisted FunctionInfo.insertText field, computes schema-quoted SQL Server expression/EXEC templates in the private catalog driver from ordered parameter rows, retains PostgreSQL name() behavior and has the shared FunctionRow insert the supplied text. The private object-id map preserves actual schema/row identity. TVP arguments require declared variables; NULL placeholders and output capture remain editable user input, not execution guarantees. Engine controls return42/43/44, quoted names and same-named routines in another schema are covered, and actual component/editor assertions fail before/pass after. The extra two disposable types needed for parameter coverage were a bounded test-fixture scope deviation; all identities were verified absent. Root integrates this slice while count-activity remains isolated, and will regenerate the authoritative bindings after both.


Count activity 5874b28 is integrated as 7829557 after root inspection. Busy owns required ConsoleWork (Run/Count/Transaction/Edit/Probe), snapshots derive optional work from that slot, and all five acquisition callers select their operation. The toolbar and Escape cancel only native Count or ordinary Run; competing controls use actual Busy. Prior Finished results remain. The existing per-tab page queue now carries request identity and an outstanding count request, including the interval before Busy publishes. Newer intent invalidates an older Last result, accepted pages alone persist, and Cancel/session loss/close clear queued work. Real blocked Engine snapshots fed into the real frontend fail before/pass after with actual cancellation and verified cleanup. Root reviewed the queue diff and regenerated bindings after merging both new wire fields. Generated output is clean. The only test-scope deviation is the routine slice's two disposable types, both verified absent.


The R10 comment review proposes two comment-line deletions and one naming/structure correction on the page queue. Both comments are removed. Pending native count ownership now uses immutable symbol identities, the field is pendingNativeCount, and the helper is deferPageForPendingWork. These names expose the pending/native-session ordering without prose. All 56 targeted editor/toolbar tests and typecheck pass after this internal refinement. No wire or execution behavior changed.

The first integrated Rust run hit SQL Server deadlock1205 during concurrent count-fixture catalog setup, before the failing case's count operation. Its cleanup verified the unique table absent. The six independent fixtures share shop system catalogs, so the harness now holds one async fixture guard across each case; each case's intentional blocker/count/cancel concurrency remains intact. No production retry, error suppression or skipped assertion was added. The refined combined run is recorded separately from the failed log.
