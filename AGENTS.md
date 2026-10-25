# AGENTS.md

L1 Data is a desktop IDE for PostgreSQL and SQL Server. A Rust engine does the work, a Tauri 2 shell hosts it, and a React frontend draws it. [README.md](README.md) lists the features and the commands. [docs/architecture.md](docs/architecture.md) explains the design and records each place the code departs from it. Read the "Load-bearing decisions" section of that doc before you change how data moves between Rust and the webview.

## Keep the design rules

Each rule below keeps a class of bug out of the app. Break one only after you change the architecture doc in the same PR.

- `crates/engine` owns all behavior and has no Tauri dependency. Each command in `src-tauri/src/ipc.rs` is a one-line call into `Engine`.
- Every type that crosses IPC lives in `crates/engine/src/model.rs`. `src/ipc/bindings.ts` is generated from it. Do not edit `bindings.ts` by hand.
- Only Rust interprets SQL text. Statement splitting, classification, table-browse SQL, and the `INSERT`, `UPDATE`, and `DELETE` statements for staged edits are built in the engine with its own quoting. The frontend sends the document text and a cursor or selection, never SQL it built.
- Cell values cross IPC as `string | null`. PostgreSQL uses its own text output. Only the SQL Server driver converts TDS values into canonical text. Do not add frontend or shared-result decoders. Rendering keys off the column's `TypeCategory`. Never round exact database values through floating point.
- Rust owns result rows. The grid pulls windows with `result_page`, and `state/rowCache.ts` caches them. Do not send whole results to the webview.
- Offsets that cross IPC are UTF-16 code units, which is CodeMirror's unit. The engine converts them once, at the boundary, in `text.rs`.
- The backend emits whole `ConsoleState` and `ConnectionState` snapshots with a rising `rev`. `state/live.ts` keeps the higher `rev` and nothing else writes to it. Only the output log uses deltas.
- A connection is a database instance, and catalogs are per database. Key catalogs, catalog versions, primary-key lookups, and object-ID maps by `DatabaseRef { connectionId, database }`, never by `ConnectionId` alone, because object IDs repeat across databases. `ConnectionState` is the only source of catalog versions. `ConnectionConfig.databaseKind` selects both the private session driver and SQL dialect. An existing connection ID cannot change kind.
- A run moves the tab's session into the run task. Commit, rollback, or a second run during a run returns `Busy`. Keep that shape so concurrent use of one session stays impossible.
- SQL the model writes runs only through `assistant::tools::run_query`. It opens its own session, runs one statement in a `READ ONLY` transaction, and drops the session without a commit. On a tab's session the query could read or change the tab's open transaction, its `SET`s, and its temp tables. On a metadata session a `set_config` or an advisory lock would outlive the call. A structure test in `assistant/mod.rs` fails if a file under `assistant/` other than `tools.rs` calls a session's client.
- A provider's protocol stays in `crates/engine/src/provider/<name>.rs`, behind the `Provider` and sans-IO `Codec` traits in `provider/mod.rs`. Keep its message shapes and flags out of `assistant/` and `model.rs`, so that a CLI update breaks one codec instead of the chat code and the contract. The same holds for the usage-limit reads: Claude's `get_usage` stays in `claude.rs`, and Codex's app-server protocol stays in `codex.rs`, each a sans-IO `Exchange` from `provider/agent.rs`. A structure test fails if a file under `assistant/` names a provider or its wire format.
- A tab's file is read and written only by `crates/engine/src/files.rs`, through `file_open` and `file_save`. Text that crosses IPC is LF with no byte-order mark, which is what CodeMirror holds. The engine restores the BOM and CRLF from `FileFormat`, replaces the file atomically, and compares the disk with the binding's `FileStamp` where it writes, so a save cannot clobber an outside change it did not show the user. The frontend stores a `FileBinding` whole and reads only its `path`. `TabSnapshot.file` is parsed leniently, so a shape this build cannot read drops the binding and keeps the tab.
- An approval is checked again where the tool runs. The turn asks the approval ledger (`Chat::gate` in `assistant/chat.rs`) at `can_use_tool` and again at the MCP `tools/call`, and a call runs only on a recorded `Run` whose input equals the approved input. A check at the ask alone would let a CLI that skips the ask, or that calls with other arguments, run a query nobody approved.

## Add or change an IPC command

1. Add or change the types in `crates/engine/src/model.rs`. Follow the wire casing in the architecture doc's "Implementation reconciliation" section. Struct fields are camelCase, and enum tags keep the Rust variant name.
2. Implement the behavior as an `Engine` method in `crates/engine`.
3. Add a one-line `#[tauri::command]` in `src-tauri/src/ipc.rs` and register it in `collect_commands!` in `src-tauri/src/main.rs`.
4. Add the same command to the dispatch table in `crates/dev-bridge/src/dispatch.rs`.
5. Run `pnpm bindings` to rewrite `src/ipc/bindings.ts`, and commit that file.
6. Run `cargo test --workspace`. The bindings drift test fails if `bindings.ts` is stale. The bridge coverage test fails if the bridge and the Tauri registration list different commands or argument names.

Tauri runs a sync command on the main thread, where no tokio runtime is active, and a bare `tokio::spawn` there aborts the app. Start engine background work with `Shared::spawn` in `crates/engine/src/lib.rs`, which uses the runtime handle the engine stores at open. The test `sync_commands_never_reach_a_bare_tokio_call` in `src-tauri/src/main.rs` fails if a sync command reaches a bare tokio call. The dev-bridge and the pg tests run inside tokio, so they cannot catch this.

The app saves `Workspace`, `Settings`, each connection's `ConnectionView` (the sidebar's database and schema choices, in `crates/engine/src/view.rs`), and each chat (`ChatRecord` in `store.rs`, and one `Entry` per transcript row) as JSON in SQLite. A type that is saved must still load data written by an older build. Data a nightly saves must also still load in the previous stable, because switching from Nightly to Stable installs that older build over the same data. Serde skips a field it does not know, but a new enum variant, or a renamed or removed field that the stable build requires, fails that load. To add a field to such a type, give it a fallback in a hand-written `Deserialize`, as `Layout` does in `model.rs` and `ConnectionView` does in `view.rs`. A stored entry that fails to parse loads as `Entry::Unreadable`, so a field added to an `Entry` variant without a fallback hides every older entry of that kind. Do not use `#[serde(default)]` on the struct, because specta then marks every field optional in `bindings.ts`.

## Run the checks

Run the checks that cover your change before you open a PR. Docker is needed only for the database-backed checks.

| Command | Covers | Needs |
|---|---|---|
| `cargo test --workspace` | engine units, SSH tunnels against an in-process server, the assistant against `fake-claude`, bindings drift, bridge coverage | nothing |
| `cargo test -p engine --features pg-tests` | engine against PostgreSQL | `pnpm db:up` |
| `cargo test -p engine --features mssql-tests` | engine against SQL Server | `pnpm db:mssql:up` |
| `cargo test -p engine --features ssh-tests` | engine through the `dev/sshd` bastion: first-use host-key trust, each auth method, a catalog, a run and its cancel, the assistant's read-only query, a refused forward, and SQL Server when it is up | `pnpm db:up` and `pnpm db:ssh:up`, plus `pnpm db:mssql:up` for SQL Server |
| `pnpm lint` | oxlint's correctness rules on the frontend and `scripts/` | nothing |
| `pnpm fmt:check` | oxfmt formatting of the frontend, `scripts/`, and the root config files. `pnpm fmt` rewrites them. | nothing |
| `pnpm typecheck` | the frontend types | nothing |
| `pnpm test` | vitest units for state, formatting, and shell, then the release scripts in `scripts/` | nothing |
| `pnpm build` | the production bundle | nothing |
| `pnpm self-check` | the engine booted inside the Tauri binary | `pnpm db:up` |
| `python3 dev/mssql-smoke.py` | mixed PostgreSQL and SQL Server bridge flow | both databases and `cargo build -p dev-bridge -p engine`; a free `L1_DATA_BRIDGE_PORT`, default `:7077` |
| `dev/bridge-smoke.sh` | the dev-bridge over HTTP, end to end | `pnpm db:up` and `cargo build -p dev-bridge` first. The script starts its own bridge. |
| `cargo test -p engine --features claude-smoke --test claude_smoke -- --nocapture` | the assistant through the installed `claude`: its argv and environment, a resumed session, and an approval held for 75 s | `pnpm db:up` and a signed-in `claude`. It spends three Haiku turns on that account. |

`pnpm db:up` starts Postgres 16 on `localhost:5433` with the seeded `shop` and `reporting` databases from `dev/seed.sql`. The `lab.type_zoo` table in `shop` holds one column per awkward type, for value-rendering tests. The `public.orders` table in `shop` has defaults, keys, indexes, and triggers, for the sidebar's table detail. Docker runs the seed only on a fresh volume, so after a change to `dev/seed.sql`, run `docker compose -f dev/docker-compose.yml down -v && pnpm db:up`.

## Prove UI changes in the running app

A passing typecheck does not show that a UI change works. Drive the real UI against the real engine:

1. Run `pnpm db:up`, then `pnpm bridge`. The bridge serves the engine on `:7077` and vite serves the UI on `:5173`. To run more than one bridge at once, give each its own pair of ports, as in `L1_DATA_BRIDGE_PORT=7177 L1_DATA_VITE_PORT=5273 L1_DATA_DATA_DIR=$(mktemp -d) pnpm bridge`. Vite passes the bridge port to the page, and the bridge allows requests only from the vite port. `dev/bridge-smoke.sh` and `dev/mssql-smoke.py` start their bridge on `L1_DATA_BRIDGE_PORT` too. Leave `L1_DATA_VITE_PORT` unset for `pnpm tauri dev`, because Tauri's `devUrl` is fixed at `:5173`.
2. Open `http://localhost:5173`, or the vite port you chose, in a browser tool such as agent-browser or Playwright. `src/dev/bridgeShim.ts` forwards every IPC call to the bridge, so the generated bindings run unchanged.
3. Select elements by `data-testid`. Add a `data-testid` to any new control you need to reach.
4. Connect to `postgres://l1data:l1data@localhost:5433/shop` and exercise the change. Tree rows have `data-testid="tree-node-<connection>/<database>/<schema>/..."`. The count pills that open the database and schema pickers are `db-pill-<connection>` and `schema-pill-<connection>/<database>`.

To compare a command's latency on two bridges, such as one built from `main` and one from your branch, run `node dev/perf/invoke.mjs <bridge-a> <bridge-b> <command> <json or @file> <runs>`. It calls each bridge once to warm up, then alternates the calls between the bridges and prints the p50 and p95 of each in milliseconds as JSON. It fails on any reply that is not a successful command. These options adapt it to commands that need more:

- `--body-a` and `--body-b` give each bridge its own body, because each bridge has its own connection and tab IDs. With both, leave out the shared body, which the probe would ignore with a warning.
- `--wait untimed` waits after each call for the run's `Finished` event on `/events`, so the next `console_run` on the tab does not return `Busy`. The sample is still the call alone, which is the time to accept the run. `--wait timed` measures from the call to the later of its reply and the `Finished` event. Both fail when the run's outcome is not `Ok`, and when no `Finished` event arrives within `--timeout-ms` of the reply, 30000 by default.
- `--teardown <command>` calls a command after each sample, untimed, with `--teardown-body`, or `--teardown-body-a` and `--teardown-body-b`. A `connection_connect` probe needs `--teardown connection_disconnect` with `{"id":"<id>","force":true}`, or every sample after the first times a connection that is already open.
- `--raw <path>` also writes every sample, in milliseconds, to a JSON file.

Chromium has no native dialogs, so `src/dev/dialogShim.ts` answers the dialog plugin's `open` and `save` in the page. Queue the answers before the action that opens the dialog, in agent-browser's `eval`:

- `window.__l1Dialog.answer(["/tmp/a.sql", "/tmp/b.sql"])` makes the next **Open** pick both files.
- `window.__l1Dialog.answer("/tmp/new.sql")` makes the next **Save As** or **Export** pick that path.
- `window.__l1Dialog.answer(null)` cancels the next dialog.
- `window.__l1Dialog.calls` lists each dialog's command and options, such as the default name Save As suggested.

Answers are used oldest first. With none queued, a dialog answers Cancel and logs a warning. The file controls are `start-open-file`, `tab-unsaved-<tab>` (the ring beside a file tab's name), `tab-file-missing-<tab>`, and the questions' `ask-dialog`, `ask-cancel`, and `ask-<choice>`, such as `ask-save`, `ask-discard`, and `ask-overwrite`. A `confirm` question keeps `confirm-dialog` and `confirm-btn`.

To drive the assistant, use the real CLI or the fake one. The real `claude` must be installed and signed in, and each turn spends that account's usage. For scripted turns with no account, run `cargo build -p engine --bin fake-claude`. Then, in **Settings** > **Providers**, set **Binary path** to `target/debug/fake-claude` and **Launch arguments** to `--fake-scenario <name>`, where `<name>` is one of `card`, `query`, `query-tampered`, `ask-differs`, `describe`, `slow`, `stop-then-query`, `wedged`, `crash`, `resume-lost`, `resume-lost-late`, `resume-lost-twice`, `resume-crash`, `signed-out-start`, or `echo`. `--fake-sql '<sql>'` sets the query the `query` scenario asks to run, and `--fake-linger-ms <n>` delays its exit on EOF. The relative path works because `pnpm bridge` runs from the repo root. In every scenario, fake-claude answers a `get_usage` request with a reply shaped like the real one, so **Settings** > **Usage** shows Claude's cards: 10% of the session left, which warns Running low, and 40% and 38% of the week. With `signed-out-start`, or a config directory whose name ends in `signed-out`, it answers with no limits. The comment at the top of `crates/engine/src/bin/fake_claude.rs` describes each scenario. The main controls are `assistant-input`, `assistant-send`, `approval-run`, `sql-card-uses`, `sql-run-new-tab`, and `provider-row-Claude`.

To drive the updater, start the bridge with `L1_DATA_UPDATE_SCENARIO=<name> pnpm bridge`. The bridge gives the engine a `ScriptedTransport` from `crates/engine/src/update/scripted.rs`, so the engine's real update machine and policy run against scripted feeds. A check takes 0.6 s and a download takes 5 s. The first check runs 15 s after the bridge starts, or when you click the button. The scenarios are:

- `current`, the default: build 0.1.0. Stable offers 0.1.0, so the build is up to date. Nightly offers 0.1.1-nightly.20261023.7.
- `available`: build 0.1.0. Stable offers 0.2.0, and Nightly offers 0.2.1-nightly.20261023.7.
- `nightly-build`: build 0.1.1-nightly.20261023.5 with no saved track, so it follows Nightly, which offers 0.1.1-nightly.20261024.6. Switching to Stable offers 0.1.0 as an older build.
- `offline`: build 0.1.0. Every check fails as offline.
- `bad-signature`: like `available`, but the download fails verification after its last chunk.
- `install-fails`: like `available`, but the install fails because the app runs from a read-only location.
- `off`: build 0.1.0 reports a development build, so updates are off and the sidebar shows no update button.

Each scripted feed also serves a change history, so once a check finds a build, hovering `update-button` opens `update-changes`. In `available`, Stable lists 0.2.0 and 0.1.1 and drops the running 0.1.0. A successful install stays at **Restarting…** in the bridge, because nothing relaunches. The controls are `update-button`, whose `data-status` names the status, `update-changes`, `update-track`, `update-action`, `update-status`, and the existing `confirm-btn`. The bridge cannot show the shell's `TauriTransport`. Only an installed, signed release build that a feed offers a newer signed build exercises it.

The bridge has three limits. It runs in Chromium, not WKWebView. Its file dialogs are the queued answers above, not the native ones. It serves commands inside a tokio runtime, so it cannot show a sync command that crashes the desktop app. For window chrome, the native file dialogs, the flush on window close and `⌘Q`, a new or changed sync command, or anything WebKit-specific, run `pnpm tauri dev` and check the real window. On macOS, `pnpm screenshot` rewrites `docs/images/l1-data.png` from the running app.

The environment variables that change the data directory, the credential store, and the test databases are listed in the README.

## Desktop

A virtual XFCE desktop runs on `DISPLAY=:1`. When the user asks to see the desktop, or when you launch a GUI app, open `http://localhost:6080/vnc.html?autoconnect=true&resize=scale` in the T3 preview pane.

## Keep the docs true

- When you add or change a user-visible feature or shortcut, update the "What it does" list in `README.md`.
- When the code departs from `docs/architecture.md`, change the section that is now wrong, or add a bullet to "Implementation reconciliation". Change the doc in the same PR as the code.
- When a change alters the main window, regenerate the screenshot with `pnpm screenshot`.

## Write commits and PRs

- Title commits and PRs in Conventional Commits form, `type(scope): subject`, with an imperative subject. Types in use are `feat`, `fix`, `test`, `docs`, and `chore`. Scopes in use are `app`, `engine`, `ui`, `window`, `icons`, `scaffold`, `readme`, and `architecture`.
- Open one PR per feature against `main`. Do not split a feature into stacked PRs. Each squash merge forces the next PR in the stack to be retargeted and rebased. Split the work into commits inside the PR instead. PRs merge as one squash commit.
- Write the PR body as a short briefing in these sections: `## Why`, `## Scope`, `## Tradeoffs`, `## Blast Radius`, and `## Verification`. Drop a section that has nothing to say. Under `## Verification`, name each check you ran and its result, say what you did not test, and attach a screenshot for UI changes.
- After you open a PR, watch it if your harness has a PR watcher, such as `watch_pull_request` in T3 Code. Fix failing checks and answer review comments without waiting to be asked. If your harness has no watcher, skip this step.
