# B9-W2 implementation report

Commit `2d455b7ede8e4921d0f1ec8b1b14bde361fc8221` on `t3/mssql-routine-insertion` starts at requested `a3b1add9b5d244a54193c76e19629c3b818ff8b3`. Worktree `/Users/nallen/.t3/worktrees/L1Data/t3-4a0b06db` is clean after the commit.

## Result

The Rust catalog now owns required `FunctionInfo.insert_text`, exposed as `insertText` in regenerated bindings. SQL Server templates bracket-quote literal schema and routine names separately. Functions insert expressions. Procedures insert EXEC statements without parentheses. The private routine map is keyed by object_id and carries schema/function indexes, keeping templates tied to their actual catalog row even when another schema has the same name.

Ordered sys.parameters rows, excluding parameter_id=0, generate ordinary editable NULL arguments. Read-only TVP parameters generate positional @table_argument_N variables requiring user declarations. The code never parses display args, interpolates metadata parameter names into SQL, or executes templates while building a catalog. The comma-bearing alias regression has display args `@first argument,type, @second int` but exactly two NULL placeholders.

Both normal and filtered tree paths keep the same FunctionRow. Its double-click callback inserts fn.insertText verbatim through the existing insertAtCursor action. No SQL interpretation, dialect check, target change, or new command was added to the frontend. PostgreSQL retains its exact old name() text for all kinds and overloads. The existing name-based PG row-path collision remains outside scope.

README changes only the function/routine double-click bullet. Architecture changes only add the routine-insertion reconciliation bullet. The docs explain required argument values, TVP declarations, output capture, and the limits of visible metadata and catalog permissions. No persisted type or migration changed. Count/activity files and types were not edited. Parent should regenerate bindings after integration with its separate count work.

## Failure before the fix

`/tmp/l1data-routine-insert-ui-before.log` records the actual FunctionRow callback operating on real CodeMirror editor state. Four assertions failed because supplied templates were replaced with `display name()`. For example, expected `SELECT [odd]]schema].[scalar]]name](); -- retain suffix`, received `SELECT display name(); -- retain suffix`. The no-active-tab case passed.

`/tmp/l1data-routine-insert-engine-before.log` records the public Engine catalog for unique quoted-schema routines, then execution of the exact old handler's emitted text. `SELECT scalar] name()` failed with native SQL Server error 102. `procedure] name()` failed with error 156. The qualified controls returned 42 and 43. The positive assertion then failed as intended. Cleanup still completed and all six OBJECT_ID/SCHEMA_ID controls returned NULL before the engine disconnected and shut down.

The Engine baseline used the handler's same literal name() construction. The frontend baseline separately invoked the actual callback. These were separate reproducible checks, not one browser-to-engine event trace.

## Passing proof and checks

All commands completed with exit code zero unless explicitly marked as the expected baseline failure.

| Command | Result | Log |
| --- | --- | --- |
| pnpm bindings | Pass; generated only the required insertText field | /tmp/l1data-routine-insert-bindings.log |
| cargo test --workspace | 315 tests pass; includes fake provider tests, bindings drift and bridge coverage | /tmp/l1data-routine-insert-workspace.log |
| cargo test -p engine --features pg-tests,mssql-tests --test routine_insertion -- --nocapture | 2 tests pass | /tmp/l1data-routine-insert-engine-after.log |
| cargo test -p engine --features pg-tests --test pg connect_reports_server_and_catalog_has_the_mockup_tables -- --nocapture | 1 test passes | /tmp/l1data-routine-insert-pg-catalog.log |
| cargo test -p engine --features mssql-tests --test mssql catalogs_details_browse_counts_and_exact_text_use_sql_server -- --nocapture | 1 test passes | /tmp/l1data-routine-insert-mssql-catalog.log |
| cargo test -p engine --features mssql-tests --test mssql_catalog_visibility -- --nocapture | 2 tests pass; hidden aliases preserve the owner's exact insertion template | /tmp/l1data-routine-insert-alias.log |
| pnpm typecheck | Pass | /tmp/l1data-routine-insert-typecheck.log |
| pnpm test src/features/sidebar/FunctionRow.test.tsx src/features/sidebar/sidebarSearch.test.tsx src/state/catalog.test.ts | 15 tests pass | /tmp/l1data-routine-insert-ui-after.log |
| pnpm test | 423 tests pass | /tmp/l1data-routine-insert-frontend.log |
| pnpm build | Pass; Vite reports its bundle chunk-size warning | /tmp/l1data-routine-insert-build.log |
| git diff --check and git diff --cached --check | Pass before commit | Checked directly |

The new Engine test executes the exact catalog text in matching context. Quoted zero-argument scalar returns 42 after SELECT. Quoted zero-argument procedure returns 43 as a standalone batch. Quoted table-valued function returns 44 after FROM. Structured scalar and procedure input templates run against fixtures that explicitly accept NULL. This does not establish that arbitrary procedures accept NULL. TVP text runs only after declaring the required table variable. A same-named scalar in another schema returns 45 with a different oid. PostgreSQL proves zero-argument execution, distinct overload oids and args, and unchanged procedure text.

The real FunctionRow callback tests cover supplied scalar, table-function, procedure, and PG text, cursor position, suffix preservation, another editor remaining unchanged, and no active tab. They invoke the component's returned Row callback and use real editor state. No browser or native mouse event acceptance is claimed.

## Cleanup, scope deviation, and limits

The routine integration catches assertion failures, attempts each cleanup, asserts object absence, closes its tab, disconnects, and shuts the Engine down before propagating the failure. Both the expected failing baseline and passing runs prove cleanup. The final MSSQL check returned eleven NULL identities. The PG check returned four NULL identities. The required alias-visibility suite separately reports no remaining disposable objects, types, users, logins, or sessions.

One object-set deviation was needed for the extra TVP and formatted-argument proof. The new integration created two disposable types inside its unique schema, a table type for the TVP and a scalar alias with a comma in its name. These extend the requested functions/procedures/schemas-only set. Both were removed and TYPE_ID was asserted NULL. The existing requested alias suite also creates its own disposable tables, types, user and login. No seeded row was written. No provider account was used, no service restarted, no parent checkout edited, no PR opened, and no nested agent spawned.

Browser/native insertion acceptance, screenshots, broader integration, fresh independent review, and PR work remain parent-owned. This report claims implementation verification only, not an independent review or release verdict.

## Design trail and skill decisions

`/tmp/l1data-routine-insert-sketch.md` contains the usage-first type sketch, architectural alternatives, source grounding, throughput checkpoint, and implemented reconciliation. It was written before implementation.

Model the Domain put the contract on FunctionInfo. Laziness Protocol kept it to one required field and one private helper rather than a new IPC command or parameter hierarchy. Test Behavior, Not Implementation selected public Engine execution and the real FunctionRow callback. Prove It Works required observed result values and explicit absence checks. Deslop inspection found no unnecessary comments, casts, saved-model fallback, frontend SQL builder, or unrelated scope in this change. Exploration was sequential under the explicit no-agents instruction.
