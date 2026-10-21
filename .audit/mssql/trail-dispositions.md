# Decision-trail review dispositions

Final application source isf70c2a9. It passes a fresh full453-test frontend run, typecheck/build and rebuilt native acceptance. The unchanged engine retains its0812b40 combined456-test pass and both native self-checks. All three R11 reviewers report zero findings within bounded scopes. Actual mixed-kind refusal/matching insertion and all-four-object cleanup are recorded in evidence/routine-target-ui-f70c2a9.txt. The final screenshot is regenerated and inspected. Remote CI remains open. The revision-labelled checkpoints below are history; the earlier438 full frontend pass precedes the queue refinement and its56 focused passes follow that refinement.

Independent gpt-6-astra [R10 audit](trail-review-r10.md) confirmed the sampled behavior and closure of all three R6 record flags. Its two documentary flags are corrected here and by append-only TSV clarifications: frontend full/focused run provenance is explicit, and checkout-relative review/UI evidence paths are complete. This audit supplies no code approval.

The independent trail reviewer was gpt-6-astra; the coordinator used gpt-6.1-sol. These are the configured OpenAI models, with reduced model-family diversity. The review is retained in [trail-review.md](trail-review.md). It audited the record and did not approve the code.

| Flag | Disposition | Evidence |
|---|---|---|
| VerifyFull had only source inspection and Require transport proof. | Closed for the installed macOS/Rustls path. A trusted certificate succeeds; wrong hostname and unrelated issuer fail as TLS certificate errors. Require controls succeed. Linux coverage is added to CI; its remote result remains pending. | `dev/mssql-tls-smoke.py`, `dev/mssql-tls-probe.rs`, [TLS output](evidence/tls.log), `05c5097`, `01ff210`. |
| The borrowed QueryStream pivot was missing from the canonical TSV. | An append records why the lease/drop contract was accepted instead of the sink sketch, with retirement and sticky-cancellation proof paths. The original design row remains. | `.audit/mssql.tsv`, `crates/engine/src/session/mssql.rs::Lease`, `dropped_streams_and_metadata_futures_retire_the_client_and_stop_native_work`, `cancel_before_the_lazy_query_subscribes_is_sticky`. |
| A reused temporary log no longer showed the historical 21-test checkpoint. | An append points to the preserved root transcript activity 405, and distinguishes the later 22-test empty-binary run at activities 503–508. | `.audit/mssql.tsv`, `9f7af6f`, root T3 thread `c3c913fc-c3ae-47f3-8755-6084ee4b3ac4`. |
| The live edit lock's blocking cost needed an explicit record. | Superseded by round 4. A trigger can end the checked transaction, so the lock did not establish atomicity. The SQL Server staged-edit driver, live-lock validation and Manual planner are removed. Current Engine refusals run before edit catalog/session/lock work. These tests describe the rejected historical contract. | Historical `2a21153` and `b81b4e4`; current `54df00b`, README/architecture and SQL Server refusal regressions. |

Selected design, review and before-fix probe evidence is retained beside this record. Dependency build chatter is omitted from the two before-fix log excerpts; actual persisted-value mismatches remain. Local paths in older TSV cells are historical run records. The current committed tests and scripts are the repeatable verification paths.

Fresh code review and remote CI remain separate gates. A later passing suite does not turn an earlier request for changes into an approval. The current user-visible limit is that SQL Server app-managed Manual mode and staged grid writes are unavailable. Native Auto scripts and explicit transaction control remain supported.


## R6 audit reconciliation

Independent gpt-6-astra audit [trail-review-r6.md](trail-review-r6.md) confirmed the principal evidence and identified three record gaps. The implementation contract now explicitly supersedes every rejected observation-SQL/depth instruction. Both native proofs have immutable revision-labelled paths, and verification.md corrects the overwritten generic pointer. A pinned completed7ebc2f0 verification row is appended to the canonical TSV without changing its earlier running checkpoint. The two R6 metadata fixes, fresh code approval and remote Linux/CI results remain open. This audit is not code approval.


## Earlier checkpoint at 63acd96

Both R6 metadata defects and both R7 refresh defects are fixed. The current verification record distinguishes the passing 447-test source pin, revision-labelled unchanged frontend/TLS/native evidence, and the pending R8 independent code review. A browser check also proves SELECT INTO in reporting from a shop tab, automatic sidebar convergence, readable exact rows and verified cleanup. The R6 audit's three documentary flags are corrected; they do not confer code approval. Fresh R8 review and remote CI remain separate open gates.
