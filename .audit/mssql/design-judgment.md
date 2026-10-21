# Independent cross-judge

Recommend **B as the base, conditional on the corrections below**. Neither candidate is ready for implementation unchanged. A timed out before producing a design and is recorded as a dropout, not scored.

Reviewed both complete candidate designs, parent grounding, acceptance and driver evidence, poteto-mode, and architect design red flags. This is a read-only design judgment using supplied probe evidence; no tests were rerun.

Scores are 1–5, with 5 strongest.

| Criterion | B | C |
|---|---:|---:|
| One kind controls compatible driver/dialect | 5 | 5 |
| Private driver details | 5 | 4 |
| Batches, multiple results, cancellation and transaction safety | 4 | 2 |
| Exact text and database-scoped provenance | 3 | 3 |
| PostgreSQL compatibility and assistant protection | 5 | 3 |
| Smallest extensible API | 4 | 3 |
| Total | **26/30** | **20/30** |

B's opaque session enum provides a real database boundary. It hides native clients, metadata SQL, edit atomicity and recovery while retaining PostgreSQL's working protocol. Its independent unit/result identities, partial-output states, bounded run budgets and unknown command counts fit TDS. Retiring every interrupted MSSQL client matches the failed cancellation/reuse probe. Its dedicated assistant entry point preserves the fresh PostgreSQL READ ONLY session and refuses MSSQL execution at both gates.

C also has a coherent backend boundary. Traits are not inherently a red flag, but three interfaces plus a borrowed event stream do not buy a demonstrated capability here. Its argument that enums scatter matches through Engine does not address B's private facade. C's ordinary-session `assistant_query(&mut self, ...)` permits precisely the wrong session lifetime unless the implementation secretly opens another connection. Its error paths lack a mandatory transaction/health outcome, and dropping a stream is not specified to poison the underlying session. These are safety gaps, not stylistic objections.

Required corrections before implementation:

1. **Lossless money.** B proposes integer-and-scale money conversion that stock Tiberius cannot provide; C's generic exact-or-error promise does not identify the trap. The probe decoded `922337203685477.5807` as `922337203685477.6`. Detect money/smallmoney from trustworthy native metadata and reject raw decoding with a clear decimal-cast instruction. Engine-generated browse projections may explicitly cast money to an exact decimal, with original type/provenance retained. Do not rewrite arbitrary user batches. If metadata cannot distinguish lossy money from legitimate float, the adapter needs a driver correction or must reject the ambiguous representation. Cover grid, full-cell retrieval, export and edit round trips.
2. **Tested TLS selection.** Both must commit to the rustls path that connected successfully. Native TLS failed. Successful rustls transport alone does not establish VerifyFull hostname/CA semantics; prove advertised modes and reject unsupported CA configuration.
3. **Transaction observation.** B maps XACT_STATE directly; C leaves the mapping unspecified. The supplied probe observed depth 0/state 1. Resolve `@@TRANCOUNT == 0` to Idle first; only then interpret XACT_STATE for nonzero depth. Define failed probes, uncommittable transactions, nested Commit behavior and implicit-transaction settings without fabricating a reusable session.
4. **Cancellation and abandoned futures.** Adopt B's bounded attention attempt followed by unconditional retirement, including cancellation before first metadata and during streaming. The acknowledgement and subsequent reuse failed in the probe. Require the same lease/drop discipline for metadata and edits. Verify server termination separately from client responsiveness, and preserve unknown write outcomes. C's stream-returning API must guarantee retirement when dropped and terminal health after errors before it can replace B's sink contract.
5. **C-specific blockers.** Move assistant execution off ordinary sessions into a fresh-session-only entry point. Specify exact edit counts despite triggers/NOCOUNT, selected-text execution without expanding to unselected batches, context-change detection for USE/dynamic SQL, and provenance invalidation after schema/context drift. B already supplies substantially stronger contracts for these cases.

Graft C's **pre-cutover driver-contract gate** into B's migration sequence. Incorporate the probe results already obtained rather than repeating discovery. Adopt C's explicit unsupported outcome for variant/CLR values unless lossless support is actually proved; B's broad variant/UDT rendering promise is unnecessary. Retain B's exact selection semantics, count-capture strategy, saved dialects, stale-response guards, generated-page shape validation and frozen assistant target.

Reject C's GO repetition support for this release. It adds execution multiplicity without acceptance need; B's explicit rejection is sufficient. Defer C's wholesale string-ID migration because database-scoped existing IDs already serve both engines with checked conversions. Remove B's new home-session heartbeat unless a concrete existing liveness requirement demands it. Its bounded formatting subset and expression validation are justified safety boundaries, but a comprehensive T-SQL formatter/parser is not a prerequisite for backend support. A third adapter should extend the private facade, not trigger an engine-wide framework rewrite.
