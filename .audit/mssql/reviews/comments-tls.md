# Scoped comment review, round 4

Reviewed `2a21153..05c5097` only in `dev/mssql-tls-smoke.py` and `dev/mssql-tls-probe.rs`, using the comment-sicko instructions. No repository edits or commits were made.

Counts: **1 deletion** (one docstring, one source line); **1 keep** (one source comment); **0 MUST KILL flags**; **0 suppressions found, 0 suppression flags**. One shebang was skipped as an interpreter directive.

- DELETE `dev/mssql-tls-smoke.py:2`: the module docstring narrates the script's purpose. It does not define a public API contract or meet another keep exception.
- KEEP `dev/mssql-tls-smoke.py:63`: explains the external container user's need to read the mounted private key despite the host's private temporary parent directory. Nearby code creates that private temporary directory, mounts only its TLS child, and grants key readability. `/tmp/l1data-mssql-tls-smoke.log` records this live fixture reaching successful verified TLS login (`trusted localhost`, SQL Server 16.0.4305.5), rejecting the wrong hostname and unrelated issuer, passing both Require controls, and removing the container and temporary directory. This supports keeping the foreign file-access constraint on an exercised path; it does not establish that mode 0644 is the only possible permission arrangement.
- `dev/mssql-tls-probe.rs`: no comments, docstrings, or suppressions to judge.

Proposed deletion patch (not applied):

```diff
diff --git a/dev/mssql-tls-smoke.py b/dev/mssql-tls-smoke.py
--- a/dev/mssql-tls-smoke.py
+++ b/dev/mssql-tls-smoke.py
@@ -1,4 +1,3 @@
 #!/usr/bin/env python3
-"""Prove the public Engine SQL Server TLS modes against a disposable server."""
 
 import json
```

Skips: interpreter shebang at `dev/mssql-tls-smoke.py:1`; docs, generated bindings, unchanged prior comments, and all other files excluded by scope. Runtime strings and executable source logic are outside this comment-only review. No services, test runs, subagents, shared edits, commits, or PR actions were started; the parent owns live execution and the final review.
