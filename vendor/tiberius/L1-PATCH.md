# Tiberius session observations

This directory contains the source of the published `tiberius` 0.13.0 crate.
The registry archive SHA-256 is
`e07324791de2bdaed058af4aa433a0b5ec5c0beac407a9797419c19805df2415`.
It matches the dependency checksum in L1 Data's lockfile before this patch.
The archive's `.cargo_vcs_info.json` records commit
`7fd75bb7070fe6ea41374698abf78928bb983bda` with `dirty: true`.
The archive checksum identifies the exact source, not that commit alone.
The original MIT and Apache licenses are included unchanged.

Only `src`, the two manifests, the upstream README, licenses, and VCS record
were copied. The normalized `Cargo.toml` omits test and example targets whose
files are not included. The crate is excluded from the application workspace.

`Client::session_state` reads existing server responses without sending SQL.
It exposes the latest database ENVCHANGE value, presence of the transaction
descriptor, packet SPID, and LOGINACK major, minor, and build version.
It rejects incomplete responses, missing login fields, and untracked
distributed or bound-session transaction changes. Cancellation safety is
unchanged. The caller must discard abandoned or cancelled clients.

The implementation changes only `src/client.rs`, `src/client/connection.rs`,
`src/lib.rs`, `src/tds/context.rs`, `src/tds/codec/header.rs`, and
`src/tds/stream/token.rs`. Protocol decoding and row conversion remain upstream.
The application regressions are in
`crates/engine/tests/mssql_session_state.rs`.

SQL Server does not set DONE_INXACT, as specified in
[MS-TDS Appendix A](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tds/135d0ebe-5c4c-4a94-99bf-1811eccb9f4a).
[ENVCHANGE](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-tds/2b3eb7e5-d43d-4d1b-bf4d-76b9e3afc791)
does not report nesting-only changes. This API cannot supply transaction depth
or committability. A login database alias may retain its supplied spelling.
LOGINACK does not contain the fourth ProductVersion revision component.

Run `python3 vendor/tiberius/verify-upstream.py /path/to/tiberius-0.13.0.crate`
to verify the archive checksum and compare every included upstream file.
The script requires the differences to equal `UPSTREAM.patch` byte for byte.
Cargo's registry cache contains the archive after installing this dependency.
