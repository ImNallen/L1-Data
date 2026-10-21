Current R5 correction in progress. Native console SQL probes described below are historical and must not ship. Protocol-only observation replaces them because they change @@ROWCOUNT and @@ERROR. See the R5 state design logs and final verification record.

# Driver probes

The temporary executable in `/tmp/l1data-mssql-probe` connects through Tiberius 0.13.0 to the real SQL Server test container. It compares raw money against a server-side exact decimal cast, returns multiple result sets, then cancels WAITFOR with an Attention request and reuses the session. The executable is outside the repository because it is a design probe.

The first native-tls attempt failed with `Tls("connection closed via error")`. The final adapter uses the successful rustls path below.

The rustls connection succeeded and returned both result sets. Raw money `922337203685477.5807` became `922337203685477.6` in F64, while the decimal cast and decimal(38,12) retained their exact values. Logs are `/tmp/l1data-mssql-probe-rustls.log`.

Canceling a WAITFOR while `simple_query` waited for its first metadata returned `Protocol("Never got a DONE token acknowledging the Attention signal.")`. Reusing that client then returned no row for SELECT 123. The implementation must treat cancellation acknowledgement failure as session loss, never return that session to the idle slot. Test cancellation both before metadata and during row streaming. The query did return early after sending Attention, but clean reuse was NOT VERIFIED.

The driver's `tds/codec/column_data/money.rs:221` returns F64 for both money variants. Large money values cannot be represented exactly that way. Do not claim exact money round trips from this representation. Preserve exactness server-side or return a clear unsupported error directing a cast to decimal.

SQL Server `SELECT @@TRANCOUNT, XACT_STATE()` can return depth 0 and state 1 during the implicit transaction of the probe itself. The adapter must resolve depth 0 as idle before it interprets XACT_STATE. A conversion error caught inside TRY/CATCH returned depth 1 and state -1 in a real-server probe, and rollback returned 0 and 0.

`Config::readonly(true)` is ApplicationIntent routing, not an enforced read-only session. It does not satisfy the assistant contract.


## R5 protocol correction

The native-state premise was tested directly after a guarded delete reproduced in the desktop app. An on-session SELECT, including EXEC, sp_executesql, RPC and local-procedure wrappers, overwrites @@ROWCOUNT. The selected replacement reads decoded TDS state without sending SQL. Microsoft documents that ENVCHANGE reports user-controlled outer transaction changes and omits nesting-only @@TRANCOUNT changes. The public contract therefore distinguishes Open with unknown depth and committability from PostgreSQL Active/Aborted. See [ENVCHANGE](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tds/2b3eb7e5-d43d-4d1b-bf4d-76b9e3afc791).

The scratch live protocol probe preserves ROWCOUNT 0 and 2 and ERROR 245. Login ENVCHANGE can echo SHOP while DB_NAME returns shop, so the metadata home session resolves that exact configured locator. Native console sessions receive the canonical database name and run no observation SQL. LOGINACK supplies a three-part server version and response headers supply SPID. Production integration, independent review and final native proof remain pending at this checkpoint.
