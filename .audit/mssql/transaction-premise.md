Premise under test, written before any proposed fix.

An accepted ordinary SQL Server DML or module-definition batch wrapped in BEGIN remains rollbackable until the app commits it, and a final active XACT_STATE with unchanged @@TRANCOUNT proves the wrapper survived. The same wrapper makes all staged edits atomic.

Falsifier. A disposable trigger ends the original transaction and opens a replacement with the same observable depth. The public Engine reports a successful Manual run and rollback, but a separate native observer still reads the changed target. Repeat through staged edits and test indirect cascade invocation. Inspect transaction identity only as a diagnostic, never as prevention.

Alternatives to test. Reject opaque invoked code under a stable metadata/permission/locking guard; disable app-managed Manual and staged edits for SQL Server; or find a documented and tested server transaction mode that prevents invoked code ending the caller's transaction. Include direct triggers, cascade targets, DDL triggers, hidden metadata, and concurrent trigger creation. No production changes in this investigation.

Data shape for probes. One private Engine store, one saved SQL Server connection, separate Auto observer and Manual tabs, uniquely named disposable objects, assertions on public ConsoleState/ApplyError plus independently observed values, and verified cleanup. Raw-driver probes use separate sessions only when Engine preflight would prevent testing native options.

throughput checkpoint: n/a, read-only investigation
