# SQL Server decision-trail review

reviewed by gpt-6-astra

Independent strongest-judgment review of root gpt-6.1-sol's trail. These are different configured OpenAI models, not vendor-family diversity. This is a trail audit, not code approval or remote-CI approval.

Scope: HEAD `d32daa1`; `.audit/mssql.tsv` lines 2–29, ending `2026-10-21T15:53:14Z`; all five requested companion documents and all three round-3 reports read top to bottom. I read this root run's messages through `t3_thread_read`, then inspected targeted activity and evidence from the same thread, `c3c913fc-c3ae-47f3-8755-6084ee4b3ac4`. Activity positions below refer only to that thread. The evidence snapshot extends through position 1301. I did not open other conversations, run services or database probes, edit repository files, commit, delegate, or create a PR.

The main account is supported: designs were compared; driver failures shaped the contract; the root integrated isolated slices; tests and actual UI checks ran; and each adversarial round reopened or retained the review gate. I found no fabricated passing suite or final approval. The critical third-round write defects are already reported and gated, not new audit findings.

## Attention

### 1. Close or explicitly qualify the outstanding VerifyFull proof condition

**Rows/moments:** TSV line 5 (`13:09:18`, design acceptance), lines 9/15/22 (local verification checkpoints); `/tmp/l1data-mssql-design/judge.md:26`; `.audit/mssql/implementation.md:42`; `.audit/mssql/verification.md:16,55`.

The accepted design judge expressly says successful rustls transport does not establish VerifyFull hostname/CA semantics and requires proof of advertised modes. The record demonstrates rustls connectivity with certificate bypass, rejection of unsupported modes/custom-CA configuration, and source inspection of the driver's root loading. It does not supply a live verified-certificate success or hostname/chain rejection check. Root activities 399–405 inspect TLS source; UI positions 602–605 move from the VerifyFull description to a successful **Require** connection. Native checks at 729, 916–917 and 1143 likewise use `sslmode=require`. The existing test `changed_database_is_retired_and_unrepresentable_tls_settings_are_rejected` at `crates/engine/src/session/mssql.rs:413` rejects Disable and a custom-CA option; it does not exercise VerifyFull certificate verification.

**Attend to:** distinguish live-tested Require from source-inspected VerifyFull, and either attach the missing evidence or explicitly retain/accept that verification limitation before declaring all design conditions closed. The current Remaining gates list omits this condition. This is an unclosed evidence requirement, not a demonstrated TLS implementation bug, and it does not make the recorded passing suites false.

### 2. Append the consequential stream-contract pivot to the canonical TSV

**Rows/moments:** between TSV lines 5 and 6; root activity 320, item `turn-item:provider:codex:native-item:exec-16d8d084-bcd2-436f-ac7b-87ce496cdbba`; `.audit/mssql/implementation.md:40`.

At position 320 the root explicitly accepts a normalized borrowed `QueryStream` in place of candidate B's sink after inspecting `Lease::drop`, conditional on retirement for every abandoned operation. The same message corrects native error-number placement and the pre-subscription cancellation race. This is a real, load-bearing decision: the design judge had specifically conditioned acceptance of a stream-returning API on abandonment guarantees. The companion contract records the deviation honestly, and real abandonment/sticky-cancellation tests exist, but no TSV row records the pivot or points to that acceptance and its proof.

**Attend to:** append one audit row naming the change from sink to borrowed stream, why the lease was accepted, and the retirement/cancellation tests. Do not rewrite the original design row. This is a chronology gap, not evidence that the implementation was unapproved or unsafe. The signed-system-object-ID decision is also only summarized in `verification.md:21` (root positions 486–488 and 522); it can be included in the integration reconciliation if the final trail is intended to retain those boundary decisions.

### 3. Repair the overwritten evidence pointer for the 21-test checkpoint

**Row/moments:** TSV line 6 (`13:33:08`); `/tmp/l1data-mssql-root-tsql.log`; root activities 405 and 503–508.

The row records 21 passing pure T-SQL tests, but its log now records 22. This is explained by reusing the same output path after accepting the empty `0x` literal. Position 405 preserves the earlier actual output, `21 passed`; position 508 preserves the later `22 passed` output and commit `9f7af6f`. The historical claim is true, but its present file pointer no longer identifies that checkpoint.

**Attend to:** append a pointer correction to the preserved transcript item/archived checkpoint evidence and identify the later rerun separately. When preparing the committed trail, retain the selected review/probe evidence it depends on: several `/tmp` files and the isolated Manual-worker report resolve today but will not travel with the PR. The verification document already correctly calls temporary logs local records; the issue here is the overwritten historical evidence, not simply use of temporary files.

### 4. Carry the new locking cost into the final tradeoff record

**Row/moments:** TSV line 26 (`15:46:29`); root message at position 1214 proposing a real table X lock inside the existing transaction, retained through writes; current accepted guard brief.

The lock is a deliberate safety choice to keep metadata and conversions authoritative. In Manual mode, an exclusive lock acquired in the existing transaction may remain until the user commits or rolls back, beyond the staged Apply operation. That can block other sessions and DDL. The present trail describes the safety purpose but not this user-visible lifetime/cost.

**Attend to:** at integration, record the actual lock lifetime and the observed blocking/release behavior, including Manual success, refusal, rollback and cancellation. Preserve the accepted safety shape; this flag does not request a different design or another approval. It is an operational tradeoff to explain, not a newly proved code defect or a claim that the live worker has skipped its promised checks.

## Row-by-row reconciliation

Line numbers include the TSV header. Existing repository paths and named commits resolve. Original shorthand in line 7 is superseded by line 25.

| TSV line | Decision/checkpoint | Reconciliation |
|---|---|---|
| 2 | Ground PostgreSQL coupling | Root messages 25/60 and architecture/session inspection support the scope. |
| 3 | Baseline and SQL Server fixture | Baseline PG log contains 45 passes; root baseline/fixture activities support workspace/frontend and server reachability. |
| 4 | Rustls and unsafe cancellation reuse | Probe log contains exact decimal, rounded raw money, failed Attention acknowledgement and missing next row. Correctly marked reuse unverified. |
| 5 | Choose B with corrections; A dropout | Judge report and root activities 226–257 support the choice and dropout. See Attention 1–2. |
| 6 | Integrate cutover/UI/T-SQL slices | All three commits resolve; cutover workspace and PG logs pass. The 21-test claim is preserved in activity 405; see Attention 3. |
| 7 | Reject truncation/retain CLR columns | Live-probe correction messages at 381/403 support it. Line 25 repairs the abbreviated paths. |
| 8 | Discard stale pending runs | Before log has two failures; after log has all 13 tests passing. |
| 9 | Local gates at 184289f | Workspace, PG46/MSSQL13, frontend371, smokes and native self-check records support the checkpoint. Independent review explicitly remains open. |
| 10 | First review gate closed | A/B/C reports substantiate the accepted findings and T-SQL split. No approval implied. |
| 11 | Saved unquoted PG browse form | Commit/tests plus actual browser output at 786–791 support page/filter/order restoration. This does not claim all whitespace forms were covered. |
| 12 | Native app and screenshots | Native interactions at 724–748, screenshot command/output at 756–757 and image artifacts resolve. |
| 13 | Separate browser SHOP failure | Correctly separates that evidence from the preceding native/screenshot proof. Probe and connection-test records resolve. |
| 14 | Integrate first-review fixes | Commits/tests resolve; SQL Server total 23 and frontend377 are supported. |
| 15 | Integrated gates after first fixes | Final named logs support PG46/MSSQL23/frontend377 and both smokes/self-checks. Review and CI remain open. |
| 16 | Canonical targets on both surfaces | Browser activities 899–913 and native 914–921 support Test/Save/Connect and descending rows. |
| 17 | Second review and dismissed binary claim | A/B reports support accepted defects. Actual old-function/IPC comparison at 945–946 and the 101-case JSON support dismissal of C's distinct keyword claim. |
| 18 | Reproduce additive sort | Browser actions and real error output at 978–988 support the duplicate-column failure and restoration. |
| 19 | Additive sort fix | Three Engine passes and browser refusal/preserved rows/plain-click checks at 1000–1018 support it. |
| 20 | Comparable/searchable representations | Four real-server regressions pass; documented explicit refusals narrow the claim appropriately. |
| 21 | Bounded Manual planner | Commit, seven-case suite and worker report resolve. Native implicit-mode and opaque-EXEC failures explain the refusal choice. The worker's skipped seeded PG test is disclosed; the root's subsequent full suite covers the integrated gate. |
| 22 | Second integrated gates | Log totals match 274 units, 49 assistant, PG46, MSSQL37 and frontend379. Both self-checks have successful records; failed preliminary invocations are explicitly excluded in verification.md. |
| 23 | Real UI rechecks | Browser datetime/Manual output and native header actions at 1116–1150 support the stated behavior. |
| 24 | PG notice-order correction | First smoke failure at 560–564, corrected live-notice test at 573–575, commit `7b2d799` and later passing smoke support the appended correction. |
| 25 | Correct truncation evidence pointers | Full paths resolve. The covered default-warning overflow checks were real; later codepage/warnings-off findings do not falsify that narrower checkpoint. |
| 26 | Reject cached-type/row-count premise | Round-3 A/C reports and failing Engine probe records support both critical findings. Guard remains in progress, not passed. |
| 27 | Distinguish unknown cancellation outcome | Before regression fails; after regression passes; toolbar tests and commit `0b48968` support the change. |
| 28 | Restore PG native metadata/saved forms | `f30f169` and `d32daa1` resolve; combined log has PG47/MSSQL38, frontend log has 393 passes. No staged-drift integration pass is claimed. |
| 29 | Browser reset label and persisted commit | Root 1255–1265 shows the label/guidance and observer `n=2`, followed by verified table cleanup. UI evidence initially still included the old cancellation notice; `f288312` subsequently corrects it. Final native/rebuilt-UI acceptance remains a separate gate. |

No earlier passing-suite row is rejected merely because an adversarial probe later exposed an uncovered case. The temporal wrong-row fix, bounded Manual planner, new decimal-drift case and codepage corruption case are distinguishable checkpoints. The two root audit correction rows are valid and have not been counted again as unresolved inaccuracies.

## Final uncompleted gates at this snapshot

- **Known critical code gates:** integrate the isolated staged-edit guard and prove the stale decimal key, lossy codepage and warnings-off truncation cases, plus lock/DDL and transaction-boundary behavior. Before-fix proofs exist; this report does not imply after-fix approval.
- **Final integration verification:** run affected combined suites and both bridge paths on the final integrated revision. The observed PG47/MSSQL38 and frontend393 passes predate that guard.
- **Final app acceptance:** native cancellation/status and current screenshots remain open. The saved-PG whitespace/semicolon browser recheck has now succeeded in the root transcript at positions 1297–1300: 500 rows, page 1,001–1,500, paid filter, total DESC, and enabled Add row. It should be added to the trail and rechecked as necessary after final integration; it is no longer wholly unobserved browser behavior.
- **Fresh independent code review** of the integrated fixes; round 3 still requests changes. This trail review is not a replacement.
- **Trail closure:** reconcile the flags above and the final worker/integration evidence, then finalize the reviewable audit artifacts. Existing companion sections lag the newest PG/UI checkpoint, but the TSV and review dispositions explicitly keep the work open; this is unfinished reconciliation, not a false final-success claim.
- **Remote CI** after the feature PR opens. No remote check or PR approval was performed here.

The TLS evidence condition is an additional unclosed verification issue identified by this scan. The stream pivot and overwritten log need audit corrections. The lock lifetime is an accepted-design tradeoff to make explicit. The remaining implementation, UI, review and CI items above are openly unfinished gates, not inaccurate claims of completion.


## Attention 1 live certificate proof addendum

Test worker at base `d32daa122d37dd91aedd34fe3489633474807362` produced commit `03a02c0019356429a79b2da5ef5a8b0f0c683787` in `/tmp/l1data-mssql-tls-proof` on branch `test/mssql-verifyfull-proof`. This closes the missing live VerifyFull certificate condition for the installed macOS/Rustls path. It is not a new independent review verdict or closure of the other flags.

Rerun `python3 dev/mssql-tls-smoke.py`. The script builds the real engine with Cargo JSON compiler-artifact paths, compiles `dev/mssql-tls-probe.rs` against those exact Engine/Tokio/serde_json libraries, and calls public `Engine::test_connection` in separate child processes. It sets a generated root file and empty root directory through child-only `SSL_CERT_FILE`/`SSL_CERT_DIR`. No application code, trust store, saved credentials, or providers changed. The helper uses an empty provider binary search path and MemoryKeyStore. Source inspected included private `session/mssql.rs`, Tiberius 0.13.0's Rustls handshake/root loading, rustls-native-certs 0.8.5's environment override, and the public model and Engine APIs.

Live output is preserved in `/tmp/l1data-mssql-tls-smoke.log`, without passwords or private key material. Official cached SQL Server 2022 image `sha256:cf8f0ab2ebe61feec2eed2629061a07d247f791c2a8ddda74a4ac9f06a291718` ran as disposable `l1-data-tls-35290ce990d2`, bound only to free `127.0.0.1:57625`. The server used a generated CA-signed certificate with only `DNS:localhost` in its SAN. The script required these real results.

- VerifyFull, generated trusted CA, `localhost` succeeded with version `16.0.4305.5`, versionNum `160000`, encoding `Unicode`.
- VerifyFull, the same trusted CA and server, `127.0.0.1` failed as `ConnectError::Tls` with `certificate not valid for name "127.0.0.1"; certificate is only valid for DnsName("localhost")`.
- VerifyFull, unrelated generated CA, `localhost` failed as `ConnectError::Tls` with `UnknownIssuer`.
- Require with the unrelated CA succeeded on both `localhost` and `127.0.0.1`. This also rules out reachability or login failure as the reason for the IP negative.
- Each successful test_connection completed MsSession's server-information SELECT after the TLS handshake and login, using `master` without seeded fixture data.

The run exited zero and reported `PASS`. Cleanup removed the container with its anonymous volumes and removed the whole temporary directory including passwords, private keys, certificates, Engine stores, and compiled helper. The script checked both absences. A subsequent `docker ps -a --filter name=l1-data-tls- --format '{{.Names}}'` was empty. Existing `l1-data-mssql` on 1434 and PostgreSQL on 5433 remained running. No bridge or Vite instance was started, stopped, or rebound.

Artifact checks included `cargo build -p engine --message-format=json`, the live smoke, `python3 -m py_compile dev/mssql-tls-smoke.py`, rustfmt on the helper, and `git diff --check`. Parent still owns integrated suites and review. Platform limitation remains explicit: this is live macOS with installed Tiberius 0.13.0 / rustls-native-certs 0.8.5 and SQL Server 2022, not cross-platform or every SQL Server/certificate variant proof. The full log includes the exact driver's remediation suffix on each certificate error; no bypass result is counted as VerifyFull success.
