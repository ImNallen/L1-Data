# SQL Server verification record

## Current source checkpoint

Final application source is`f70c2a9dff4d372eebd181e2aa7b4142b39d7565`. The R10 cross-kind insertion warning is corrected at one shared insertion boundary. The root ran the full453-test frontend suite, typecheck and production build at this revision; all pass. The native app bundle rebuilt successfully. Logs are`/tmp/l1data-r11-{frontend,types,build,bundle}.log`. The engine is unchanged from the0812b40 combined456-test/23-target pass, both HTTP smoke passes and both native self-check passes recorded below.

Root browser acceptance proves both mismatch directions and the filtered tree preserve the editor text and target while showing the expected error. Matching SQL Server insertion executes and returns43. The rebuilt WKWebView also proves normal and filtered mismatch refusal, followed by successful matching PostgreSQL insertion after an explicit target change. [The immutable final insertion observations](evidence/routine-target-ui-f70c2a9.txt) distinguish these actions from the earlier failing-before reproduction. Independent SQL cleanup returned all four owned object IDs as NULL.

All three fresh R11 reviewers report zero critical findings, warnings and nits in their bounded scopes. Their reports are [A](reviews/round-11-a.md), [B](reviews/round-11-b.md) and [C](reviews/round-11-c.md); each was explicitly stopped before final commit/push. None claims release approval or substitutes for root UI/CI work. The R11 comment audit found no added comments or suppressions. Independent Astra decision-trail audit R10 found two documentary flags, both corrected by explicit provenance and append-only evidence-path clarifications.

`pnpm screenshot` regenerated the1440x900 main-window PNG from the real desktop app at the final source and the image was inspected. Its isolated fixture container was removed; only the original PostgreSQL and SQL Server services remain. The root bridge/Vite and native acceptance processes are stopped. The tracked final screenshot and decision record follow the reviewed application source without further application changes. Remote CI, including Linux TLS, remains pending until the PR runs.

## Earlier source checkpoint at0812b40

Source pin `0812b40f2198d14c69ef7f5d1a5121ccd227923b` includes the R9 count-activity and routine-template corrections. The combined workspace suite with `engine/pg-tests engine/mssql-tests` passes 456 tests across 23 targets. The full438-test frontend suite passed before the queue identity/naming refinement; afterward, all56 affected editor/toolbar tests and typecheck passed. The refined source rebuilt the desktop bundle successfully. Both HTTP smokes and both current-source native self-checks pass. Logs `/tmp/l1data-r10-rust-refined.log`, `/tmp/l1data-r10-frontend.log`, `/tmp/l1data-r10-queue-rename.log`, `/tmp/l1data-r10-types-refined.log`, `/tmp/l1data-r10-bundle-final.log`, `/tmp/l1data-r10-{pg,mixed}-smoke.log` and `/tmp/l1data-r10-native-{pg,ms}.log` distinguish these runs. The regenerated screenshot remains pending.

The first R10 combined run failed with SQL Server error1205 during independent count-fixture catalog setup. No count assertion had run in that case. `0812b40` serializes those independent DDL fixtures while retaining concurrency within each case. The complete refined run passes; the failed run remains in `/tmp/l1data-r10-rust.log`. No production retry or skipped assertion was added.

The actual browser and native WKWebView both set DATEFIRST 1 on a table tab, applied a weekday filter to a disposable 1002-row table and used Last page. Both displayed row1001 and exact pager 1,001–1,001 of1,001. Cleanup was observed as 0 in the native UI and independent SQL observer. See [the immutable count UI observation](evidence/count-ui-a3b1add.txt). The earlier [cross-database SELECT INTO observation](evidence/refresh-ui-63acd96.txt) remains evidence for unchanged metadata paths.

The root now exercised a genuinely blocked Last-page count in Chromium and WKWebView. Both show Counting and a reachable Cancel control while preserving the original500-row result. Clicking browser Cancel and pressing Escape with the native editor focused retire the session, restore Run, retain the old result, and show the uncertain-outcome notice. Native sidebar insertion executes the quoted scalar, procedure and table-function templates and returns42,43,44 respectively. See [the immutable R10 UI observations](evidence/count-cancel-routine-ui-0812b40.txt).

The five macOS/Rustls TLS cases and native register-preservation check remain revision-labelled evidence for unchanged paths. Linux TLS execution awaits remote CI. R10 A/B approve their bounded scopes with zero findings. C reports one warning: a routine template can enter an active tab of the other database kind. Root reproduced it in the actual browser and undid the insertion. An isolated correction is underway. Fresh review of that correction remains pending. No earlier request-changes verdict, local check or self-review is counted as approval of a later source revision.

SQL Server first-release limits remain explicit. App-managed Manual mode, staged grid writes and assistant query execution are unavailable. Native scripts and transaction commands remain available. Protocol state reports transaction presence with unknown nesting and committability. Cancellation retires the session and reports uncertain persisted outcomes.

The sections below are chronological evidence. Older claims about managed SQL Server writes, transaction probes or known depth are superseded by [the current implementation contract](implementation.md).

The connection kind selects one private driver and one SQL dialect. PostgreSQL remains the default for older saved data. The implementation keeps exclusive tab sessions, database-scoped catalog identities, Rust-owned rows, UTF-16 offsets and revisioned snapshots.

## Completed checks

- Baseline workspace, PostgreSQL integration and frontend checks passed before the cutover. PostgreSQL had 45 integration cases and the frontend had 360 tests.
- The coordinator independently ran the PostgreSQL cutover through `cargo test --workspace` and `cargo test -p engine --features pg-tests`. Both passed, including all 45 PostgreSQL integration cases.
- The integrated frontend passed `pnpm typecheck`, all 371 tests in 42 files and `pnpm build` with the complete generated bindings.
- The coordinator ran all 22 pure T-SQL tests after accepting an empty `0x` binary literal, which SQL Server confirmed has zero bytes.
- Two pending-console-setup cases failed on the previous frontend code and passed after the target guard. The cases change the database or transaction mode while setup waits.
- SQL Server fixtures ran repeatedly without duplicating seed rows. The CI seeding command also ran against the local server.

## Live driver findings

- Rustls connected successfully. The native TLS probe failed. The adapter therefore uses the tested Rustls path.
- Stock Tiberius decoded large money through floating point and lost digits. Generated table pages cast money to exact decimal. Arbitrary raw money results fail with a cast instruction.
- Attention acknowledgement did not establish clean client reuse. SQL Server cancellation always retires the client. Engine tests separately observe that the server request terminates.
- A bounded text conversion truncated data before an insert could reject it. Staged writes now preserve target enforcement and check Unicode and binary capacity. The live overflow tests pass without applying rows.
- An inner join to base SQL types omitted CLR columns. Catalog loading now retains those columns, and browsing refuses their unsupported values explicitly.
- System views can have negative object IDs. The driver maps signed native IDs to the existing scoped opaque wire IDs without losing bits, then reverses that mapping for detail queries. The live system-schema check passed.
- Independent review proved a wrong-row staged update under `DATEFORMAT dmy`. It also found untyped binary filters, rowversion-key rejection, initial database canonicalization failure, duplicate case-variant sort keys, incomplete mixed-batch catalog invalidation, and legacy PostgreSQL browse-template drift. Commits `3cfd3d0`, `c02a0df`, `c98d8e1`, and `b6fe451` fix these with behavior regressions. `d32b08b` separates the T-SQL behavior modules. The original failures were reproduced before each fix. The first review requested these changes and is not counted as final approval.

## Integrated checks at 184289f

- `cargo test --workspace` passed.
- `cargo test -p engine --features pg-tests,mssql-tests` passed, including 46 PostgreSQL and 13 SQL Server integration cases.
- Both HTTP bridge smokes passed, including assistant refusal against SQL Server in Auto and Ask First modes. The mixed smoke also passed with IPv6 hosts.
- Browser UI checks passed against the real bridge for both connection kinds, T-SQL multiple outputs, formatting, table editing and rollback, cancellation/reset/reopen, generated-column protection, and immutable connection kind.
- Both native self-checks passed. They invoke the changed synchronous text services on a plain thread without a Tokio runtime. The bundled native app also passed SQL Server connection testing, formatting, execution, table browsing and header sorting through its actual WebKit UI.
- Independent comment passes completed before review. Three independent correctness/maintainability reviewers requested the fixes listed above.

## Remaining gates

- Obtain fresh independent code review of the integrated fixes and finish the trail-review dispositions.
- Remote CI after opening the feature PR.

## Final integrated local checks at d32b08b

- `cargo test --workspace` passed, including bindings drift, bridge coverage, and desktop sync-command structure checks.
- `cargo test -p engine --features pg-tests` passed, including 46 PostgreSQL integration cases and 49 assistant cases.
- `cargo test -p engine --features mssql-tests` passed, including 23 SQL Server integration cases and the native driver/count regressions. New cases cover all supported temporal families under `dmy` and `mdy`, wrong-row prevention, binary filters/counts/searches, rowversion keys, overflow rollback, canonical database identities, both refresh obligations, and CI/CS column paging.
- `pnpm typecheck`, all 377 frontend tests in 43 files, and `pnpm build` passed. The bundle reports the existing large-chunk advisory.
- `cargo build -p dev-bridge -p engine -p l1-data`, `dev/bridge-smoke.sh`, and `python3 dev/mssql-smoke.py` passed.
- `pnpm self-check` passed for PostgreSQL. It also passed with SQL Server URL database `SHOP`, returning canonical `shop`, both catalogs and the changed synchronous text services without an active runtime.
- `pnpm screenshot` regenerated `docs/images/l1-data.png` from the real desktop app. Both it and `docs/images/sql-server.png` were inspected.
- A fresh comment audit found no added prose comments or suppression flags in the review fixes. Earlier comment passes remain valid for unchanged code.
- Final browser and bundled-native rechecks passed for Test/Save/Connect with configured database `SHOP`. The sidebar, default/new queries, table pages, saved tabs, console snapshots and `DB_NAME()` results resolve to `shop`. The native grid also accepted `ID DESC` and returned row 2 before row 1 without a duplicate-column error. The saved configuration retains the accepted locator `SHOP`.

Logs are `/tmp/l1data-mssql-final-{workspace,postgres,sqlserver,frontend,typecheck,bundle,build,pg-smoke,mixed-smoke,native-pg,native-ms}.log` in this working environment. The commands and committed regression tests are the repeatable evidence; the temporary paths are local run records.

## Intentional limits

SQL Server initially supports SQL login over TCP, encrypted VerifyFull or Require connections, and no custom CA file. Cancelling resets the tab session. Raw money requires an explicit decimal cast. Unsupported CLR and sql_variant values fail. T-SQL formatting and raw table expressions use a bounded subset. GO repetition and sqlcmd directives are refused. Arbitrary batch affected counts and PRINT/INFO messages may be unavailable. Model-written SQL remains disabled for SQL Server; schema tools and suggestions are available.

## Integrated second-round checks at 24200b1

The coordinator ran `cargo test --workspace --features 'engine/pg-tests engine/mssql-tests'`. It passed 274 engine unit tests, 49 assistant cases, all 46 PostgreSQL integrations, all 37 SQL Server integrations, bridge coverage, bindings drift and desktop sync-command checks. The SQL Server cases include seven Manual transaction regressions, four canonical-filter regressions and three metadata-resolved header-sort regressions.

`pnpm typecheck`, all 379 frontend tests in 43 files, and `pnpm build` passed. The native app bundle rebuilt successfully. Both bridge smokes and both native self-checks passed. The SQL Server self-check used `mssql://.../SHOP?sslmode=require` and returned canonical `shop`. A first invocation used the wrong environment variable; another used unsupported URL options and correctly failed parsing. Neither is counted as a SQL Server pass.

The actual browser filter for displayed datetime text changed from zero rows before the filter fix to the expected one row afterward. A normal Manual query displayed an active transaction and rolled back. Conditional transaction control was refused with the old result preserved. The actual rebuilt native app passed Test/Save/Connect through `SHOP`, returned `shop` from `DB_NAME()`, browsed orders, preserved `ID DESC` and rows on a refused Shift-click, and replaced the order with `[id] ASC` on an ordinary click.

The final comment audit found zero added source comments, suppressions or reshape flags in `d32b08b..24200b1`. Earlier audits cover the unchanged feature code. The fresh third correctness panel subsequently requested changes; passing local gates are not counted as review approval.

Repeatable tests live in `crates/engine/tests/mssql_{filters,header_order,transactions}.rs` and `dev/{bridge-smoke.sh,mssql-smoke.py}`. Local logs are `/tmp/l1data-mssql-r3-{rust,typecheck,frontend,build,binaries,native-build,pg-smoke,mixed-smoke,native-pg,native-ms}.log`. UI events are recorded in this run's T3 transcript, `c3c913fc-c3ae-47f3-8755-6084ee4b3ac4`; the bridge log is `/tmp/l1data-mssql-ui-r3-final.log`.

Manual mode supports ordinary batches and stored module definitions under an explicit transaction. It refuses ambiguous transaction control, opaque procedure calls and transaction-changing or execution-suppressing options before any selected batch runs. Native scripts remain available in Auto-commit. Text/ntext/XML equality compares serialized text; float/real text searches and escaped patterns above 4000 UTF-16 units refuse explicitly.

## Third-round cancellation fix at 0b48968

A new real Engine regression failed on `24200b1` because cancelling after a committed update returned `txnLost: true`. The fix replaces that boolean with `TransactionLoss::{None, RolledBack, Unknown}`. SQL Server retirement reports an unknown outcome and displays **session reset**, preserving the fact that earlier commits remain committed. PostgreSQL retains its known rollback and idle-loss behavior.

The regression passes for both an earlier auto-committed batch and a native `COMMIT` inside the interrupted batch; an observer verifies the rows remain committed. Three toolbar rendering cases pass. The coordinator also reran the full combined workspace/database suite, frontend typecheck, all 382 frontend tests, and the production build; all passed. Logs are `/tmp/l1data-mssql-cancel-{before,after,workspace,ui,typecheck,frontend,build}-r3.log`. The other third-round fixes, final UI checks, fresh review and CI remain open.


## Final application checks after 2a21153

- Combined `cargo test --workspace --features 'engine/pg-tests engine/mssql-tests'` passed: 274 engine units, 49 assistant cases, all 47 PostgreSQL and all 52 SQL Server integrations, bridge coverage, generated-binding drift and native sync-command structure.
- `pnpm typecheck`, all 393 tests in 44 files, and `pnpm build` passed. Both the development binaries and bundled macOS app rebuilt. The existing bundle-size and bundle-identifier advisories remain.
- `dev/bridge-smoke.sh` and `python3 dev/mssql-smoke.py` passed against the integrated guard. Both native self-checks passed; SQL Server used configured `SHOP` and canonical `shop`. They exercise synchronous services on a plain thread without Tokio.
- The actual browser rejected `漢字` for a CP1252 varchar column with “Nothing was saved”; an observer confirmed key `1.234` still had `ok`. In the actual bundled native grid, a valid `new` submission succeeded and the observer confirmed it persisted. Cleanup dropped the table and checked zero matching objects. The native cancellation toolbar and notification both preserve earlier commits and report session reset/unknown outcome.
- The saved PostgreSQL whitespace/semicolon tab reloaded in the browser at page 2 with 500 rows, paid filter, total DESC and enabled editing. Its saved document was not rewritten.
- `pnpm screenshot` regenerated the main image from the running native app; it was visually inspected. The SQL Server multi-result image remains current for unchanged layout.
- The additional lock/cancellation regression in `b81b4e4` passed after the combined suite. It verifies the Manual edit's table lock releases after session retirement and the still-open update rolls back. This is a test-only addition; application behavior is unchanged after 2a21153.
- The coordinator ran `python3 dev/mssql-tls-smoke.py` on the integrated source. All five public Engine cases pass: trusted CA/hostname, name rejection, chain rejection, and two Require controls. Every success completes the server-info SELECT. Container, temporary credentials, certificates, keys and stores are removed and absence checked. This proves the installed macOS/Rustls path; Linux execution is added to remote CI and remains pending.

Logs are `/tmp/l1data-mssql-r4-{rust,typecheck,frontend,build,binaries,native-build,pg-smoke,mixed-smoke,native-pg,native-ms,screenshot,tls,lock-cancel}.log`. The app code is unchanged in later TLS-test/CI, comment, whitespace and image commits. These local logs are run records; committed tests and scripts are repeatable evidence. Fresh independent review remains a separate gate.

## Round 4 scope correction in progress

All results above describe their pinned historical revisions. Independent R4 findings reopened the correctness gate. The current release contract removes SQL Server app-managed Manual mode and staged grid writes, retaining Auto-native scripts and explicit transaction commands. The earlier lock/Manual acceptance cases are historical evidence of why that attempted contract was rejected, not advertised capabilities.

70b5806 adds typed unknown edit outcomes and removes the false Nothing was saved claim. A current-source public Engine rerun produced OutcomeUnknown with a trigger's committed audit value, then verified cleanup. 9898d0a disables SQL Server Manual selection and grid edits, keeps pending-data revert available after provenance loss, and updates README/architecture. Parent browser checks observed the disabled menu item, disabled add/submit controls, no cell editor after double-click, the read-only explanation, and retained table pager/filter controls. The named-key fix and authoritative Engine refusal/deletion are integrating in isolated worktrees. Fresh integrated tests, final native/browser checks and independent review remain required before the PR.

## Narrowed release checks at 98301df

Parent combined workspace checks passed with both database features enabled. The run includes 274 engine units, 49 fake-assistant integrations, all 47 original PostgreSQL integrations plus the new named-key integration, and 35 SQL Server integrations across seven files. Both bridge coverage and bindings drift/native sync structure checks pass. Typecheck, all 408 frontend tests in 44 files, production bundle, development binaries and debug macOS bundle pass. Logs use /tmp/l1data-mssql-r5-{rust,typecheck,frontend,build,binaries,native-build}.log.

Both real HTTP smokes pass, including PostgreSQL notices/SSE and mixed SQL Server native writes/results, both capability refusals, preservation of an explicit outer transaction with XACT_ABORT ON, cancellation, historical kind and fake-provider refusal before approval. Both native self-checks pass, including the text services on a plain thread. The SQL Server fixture self-check uses explicit sslmode=require; the first default VerifyFull attempt correctly rejected its outdated self-signed certificate. This was a fixture invocation correction, not a relaxed application default.

Final source comment audit found no deletions, suppressions or reshape flags. It retained the public PostgreSQL table_apply contract. The configured Astra, Sol and Luna reviewers are running a new round against the full feature and the narrowed contract. This is model variation within one provider family, so vendor diversity is reduced. Native UI/screenshots, the fresh panel verdict and remote CI remain pending. No application source change follows the pinned review head unless a finding requires it.

Final desktop acceptance passed on the rebuilt bundled app at 98301df. Native Test/Save/Connect through SHOP resolves to shop. Manual is visibly unavailable. Explicit BEGIN plus DECLARE/two SELECTs yields shop/7 and 世界 in separate results, shows transaction buttons, and toolbar Rollback returns Idle. Native table browsing has disabled write controls and a read-only explanation. Header sort produces [total] ASC with 42.00 before 125.50; WHERE status='paid' returns one125.50 row. WAITFOR cancellation shows Cancelled/session reset and the earlier-commit/unknown-outcome notice. The main screenshot was regenerated with pnpm screenshot and visually inspected at1440x900. No seeded rows or real providers were used for this native pass. See [revision98301df native evidence](evidence/native-ui-98301df.txt).

The final rebuilt bridge also reopens the legacy PostgreSQL whitespace/semicolon document at page2 with500rows, filters, ordering and paging intact. The actual browser named-key failure/refusal and cleanup evidence remains in evidence/key-ui-{before,after}.log. No source change followed the review pin; the PNG and audit evidence await the fresh verdict before commit.

Round5 reopened native-semantics and metadata gates. The tests/UI passes at98301df above do not establish safety of the transaction-state observation between batches. Parent independently reproduced A's guarded delete in the native editor on a local temporary table and verified DROP/OBJECT_ID NULL. One critical and two metadata/refresh warnings remain under isolated correction. No PR or remote CI has started.


R5 correction checkpoint. Restricted metadata regression is integrated in c465ddc and a451ab0 and passes through public relation_detail and the actual assistant description dispatcher. The native-state and refresh-lifecycle fixes are still being integrated. Earlier complete test and UI runs remain evidence for their pinned source only; the release gate remains closed.


## Integrated protocol revision 7ebc2f0

`cargo test --workspace --features 'engine/pg-tests engine/mssql-tests'` passes. Engine units277, fake assistant49, PostgreSQL48 plus the PostgreSQL refresh-outcome control, SQL Server48 across nine integration files, and bridge/bindings/native structure tests pass. The source-shared refresh file has seven cases, six SQL Server and one PostgreSQL. Frontend409 tests, typecheck and production build pass. Root reran the seven refresh cases after its lifecycle field rename.

Both HTTP smoke scripts pass sequentially. An initial parallel smoke invocation was refused because both scripts own port7077; no feature failure occurred. Native self-checks pass for both databases. The rebuilt WKWebView app passes the previously failing selected guarded-delete script with survivor1, compared with survivor0 before the fix. It shows open transaction presence without depth/doom, while user SQL reports actual nested depth2. Commit ends all nesting; local-temp cleanup returns OBJECT_ID NULL. Native read-only browsing, sort, filter, disabled Manual guidance and honest cancellation/session reset also pass. Browser restores the saved PostgreSQL semicolon/whitespace browse document with paid filter, total DESC and page1001–1500 intact; SQL Server bracket browse remains read-only.

The five TLS smoke cases pass again through the integrated vendored driver on macOS/Rustls, with cleanup verified. Successful Test calls now decode server identity from login protocol without a server-info SELECT. Historical earlier TLS records correctly describe their then-current implementation.

Vendor verification passes against the pinned registry archive, including missing-file/unrecorded-delta negative controls from the worker. All 96 retained unchanged files are byte-identical; seven modified files match the recorded210-line patch. The final comment audit removes2 lines and resolves the internal lifetime flag through the explicit session_release signal, with the actual-client lifetime regression.

A fresh independent A/B/C review at7ebc2f0 is running. No final approval or remote CI result is claimed here.


The coordinator reran pnpm screenshot on7ebc2f0 and inspected the resulting1440x900 image. It uses an isolated disposable PostgreSQL fixture and fake provider. Cleanup removed that fixture. R6 C has reopened the code gate for opaque procedure metadata refresh; native-state acceptance remains valid evidence for the unchanged native execution path, not final approval.


Evidence pointer correction after the R6 independent trail audit. The former native-ui-final.txt path was overwritten during acceptance at7ebc2f0. Both original proofs are now retained separately as evidence/native-ui-98301df.txt and evidence/native-ui-7ebc2f0.txt. The old generic path is only an index and establishes no acceptance result. The native claims above retain their original revision.


## R6 metadata corrections integrated at8b78498

The final combined workspace suite passes438 tests, including279 engine units,49 fake-assistant integrations,49 PostgreSQL integrations including its refresh-outcome control,56 SQL Server integrations, and5 bridge/native-binding checks. The unchanged frontend retains its409-test/typecheck/build passes. Both metadata corrections add no IPC/frontend/conversion changes.

The coordinator rebuilt the desktop bundle. Actual browser acceptance under a new restricted login with only SELECT/EXECUTE object grants shows every alias-typed table column and its readable value, plus the executable procedure in Functions. This runs through the real newly built engine and generated IPC. Cleanup returned zero objects/types/users/logins/sessions. See evidence/metadata-ui-8b78498.txt. The selected native guarded-delete proof remains at its immutable7ebc2f0 path for the unchanged native execution code. Root native self-checks are rerun after the metadata-query changes.

Fresh independent A/B/C review is running on8b78498. The two R6 root causes are fixed with failing-before Engine evidence; this checkpoint does not count worker checks or earlier requestchanges as approval. Remote CI remains pending.


R7 correction checkpoint at c4e6fa4 (architecture wording is the only tracked working-tree change). `cargo test --workspace --features 'engine/pg-tests engine/mssql-tests'` exits 0 with 447 passing tests across 21 targets. This includes explicit-prompt units, SELECT INTO unit/public Engine regressions, cross-database direct/opaque DDL and retirement tests, PostgreSQL database-scope controls, all existing session-state and restricted-metadata regressions, bridge parity and bindings drift. Log `/tmp/l1data-r8-rust.log`. No earlier review is treated as approval for these changes.

The optional whole-workspace `cargo fmt --all --check` reports existing widespread formatting differences, including unchanged Tauri command tests. No broad formatting change was applied. Scoped `git diff --check` passes.


At source pin 63acd96, the rebuilt dev-bridge and native Tauri binary pass both HTTP smoke scripts and both native self-checks. Native SQL Server logs in using SHOP and returns canonical shop. Logs `/tmp/l1data-r8-{native-build,pg-smoke,mixed-smoke,native-pg,native-ms}.log`. The actual browser UI ran cross-database SELECT INTO from shop, automatically added reporting's table, opened it and displayed id73/label cross-database value. It then ran DROP and automatically removed the tree entry. Independent sqlcmd confirmed the row and subsequent zero leftover objects. The temporary tabs were closed. Immutable observation `.audit/mssql/evidence/refresh-ui-63acd96.txt`. No Refresh button, seeded-row write or real provider was used in this UI check.

The R8 comment audit at c4e6fa4 reviewed all 50 added production lines since 8b78498 and found zero comments to delete, suppressions or MUST KILL flags. Its source pin differs from 63acd96 only by the architecture refresh-scope sentence. Three fresh independent reviewers read 63acd96. Their results and remote CI are still pending.
