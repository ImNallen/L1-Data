<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/images/l1-logo-white.svg">
  <img src="docs/images/l1-logo-black.svg" alt="L1 Data logo" width="64">
</picture>

# L1 Data

L1 Data is a desktop IDE for PostgreSQL and Microsoft SQL Server, built with Tauri 2, Rust, and React. The design is in [docs/architecture.md](docs/architecture.md). Agents working in this repo start with [AGENTS.md](AGENTS.md).

![L1 Data running the revenue-by-region query against the seeded shop database, with the assistant answering a question about it](docs/images/l1-data.png)

## What it does

### Connections and the catalog

- Choose PostgreSQL or SQL Server when creating a connection. Each tab loads that connection's SQL dialect for highlighting, completion, formatting, and execution. SQL Server connects over TCP with a SQL login or with **Microsoft Entra ID (Azure CLI)**, chosen under **Authentication**. Its defaults are port 1433 and database `master`. To change database kind, create a new connection. Existing saved connections keep PostgreSQL.
- **Microsoft Entra ID (Azure CLI)** signs in to Azure SQL as the account of `az login`. The engine runs `az account get-access-token` and keeps the token in memory until five minutes before it expires, so the tabs of a connection share one `az` call. No password or Azure credential is saved. Install the [Azure CLI](https://learn.microsoft.com/cli/azure/install-azure-cli) and run `az login` first. If the server refuses the token, the next try asks `az` again, so running `az login` with another account takes effect at once. A pasted `sqlserver://` URL with `Authentication=ActiveDirectoryAzCli`, as a query option or a `;key=value` property, selects it.
- The connection tree shows each connection's databases, their schemas with their table counts, the tables and views of each schema, and a Functions group. Expand a table to see Columns, with a type glyph, primary and foreign key marks, the type, and `· null` on nullable columns. Defaults, Keys (primary, foreign with its target, unique, and check), Indexes (key list, method, and partial predicate), and Triggers (timing, events, and implementation) follow when the table has any. Use the arrow keys to move through the tree. `→` opens a row or moves into it, `←` closes it or moves to its parent, `Home` and `End` jump to the first and last rows, and `Enter` opens a table or view, inserts a routine, or opens and closes any other row.
- A connection row's `shown/total` count opens a picker of the server's databases, and a database row's count opens a picker of its schemas. Check a box to show that database or schema in the tree, or use **All**, **Default only**, or **None**. Adding a database also shows its default schema. System databases and schemas stay out of the pickers until you turn on **Show system databases** or **Show system schemas**. The count turns amber when nothing is shown. Each connection keeps its choices. The picker opens with its filter focused, `Tab` cycles inside it, and `Esc` or **Done** closes it and returns focus to the count.
- A disconnected connection row shows a **Connect** button in place of its count; a connected row shows only the count, dimmed while it is off.
- Double-click a table or a view to browse its rows in a tab. Saved PostgreSQL browse tabs keep paging, filters, sorting and editing with a trailing semicolon or outer whitespace in their generated query. Double-click a routine to insert its template at the active tab's cursor. The tab must target a saved connection of the same database kind. An unresolved or mismatched target shows an error without editing or retargeting the tab. SQL Server functions insert schema-qualified expressions for `SELECT` or `FROM`, and procedures insert `EXEC` calls. Templates with parameters require argument values. Replace the editable `NULL` placeholders as needed, declare table-valued arguments as `@table_argument_N` variables, and use variables with `OUTPUT` to capture output parameters.
- The sidebar's search box filters the tree in place. The tree keeps the tables, views, columns, and functions whose names contain every word you type, in the schemas it shows, and opens each match's parents and highlights the match. A matching column sits under its table. `Enter` opens the first table or view whose name starts with the words you typed, else the first one listed. `Esc` clears the search. `↓` moves into the tree. `⌘P` focuses the box.
- The connection dialog lists the saved connections on the left, with a live dot on each connected one. Click one to edit it, or **New** to add another. Paste a `postgres://` or `sqlserver://` URL to fill in the database kind, server, database, user, password, and TLS mode. The name is optional and defaults to the host and database. Pick a color to mark the connection in the tree and the top bar. To shade each colored connection in the tree with its color instead of a dot, turn on **Tint connections by color** in **Settings** > **Appearance**. **Test** shows the server version before you save. Leaving a form with unsaved edits, by choosing another connection or closing the dialog, asks before it drops them. Right-clicking a connection connects, disconnects, opens a SQL tab, refreshes the catalog, edits, and deletes.
- Set **Environment** in the connection dialog to **Local**, **Staging**, or **Production**, or leave it **Not set**. You can change it later. A Staging tab shows an amber flask in front of the connection name in the top bar. A Production tab shows a red shield there and a red line under the top bar. The connection tree marks the same connections with the same icons after their names.
- On a Production connection, a run that writes asks first, for example **Run 2 writes on Production?**, names the connection and database, and shows the first three write statements. **Cancel** runs nothing. The **Run** button carries a red ring on a Production tab. The question counts only the statements, or SQL Server batches, that the run executes: the selection, the marked statement, or all of them. A write is any statement that **Query history** puts under **Writes**, such as `INSERT`, `UPDATE`, `DELETE`, `MERGE`, `TRUNCATE`, DDL, `GRANT`, `CALL`, `DO`, and `EXECUTE`. On SQL Server, a batch that writes or runs `EXEC` counts. The check reads the SQL text, so a `SELECT` that calls a function with side effects runs without asking. Loading a table page does not ask. **Submit** for staged edits asks too, for example **Apply 2 changes on Production?**, and counts the inserts, updates, and deletes it would send. The engine makes this check where the run starts, so a write that skips the question is refused too. An answer covers only the statements, connection, database, schema, mode, and transaction state it named. If any of them changes, the app asks again. A tab that connected while its connection was Production keeps asking after you change the environment, until its session closes, for example when you disconnect. **Commit** asks too, for example **Commit 1 write on Production?**, when the open transaction holds writes that ran before the connection was marked Production.
- Turn on **SSH tunnel** in the connection dialog to reach the database through an SSH server. Sign in with a password, a private key file with an optional passphrase, or an SSH agent at `SSH_AUTH_SOCK` or a socket you name. The SSH server resolves and dials the database host, so a name only it knows, such as `db.internal`, works. The connection list shows the server after `via`. The app checks the server's host key against `~/.ssh/known_hosts`, which it never writes, and against its own `known_hosts` file in its data folder. When the key is unknown, **Test** shows its fingerprint with **Trust and test again**, and **Connect** asks before it trusts it. Trusting adds that key to the app's own file. A changed or revoked key fails, and the error says how to fix it. `~/.ssh/config` is not read, so enter the host, port, user, and key file in the dialog.
- The OS credential store holds one master key, and saved passwords and SSH secrets are sealed under it. macOS asks for keychain access once per launch.

### Query tabs

![SQL Server and PostgreSQL connections with a T-SQL batch and multiple result sets](docs/images/sql-server.png)

- SQL Server tabs run T-SQL batches separated by a standalone `GO` line. Semicolons inside a batch keep their variable and temporary-table scope. A cursor runs its batch; a selection runs exactly the selected text. Several `SELECT` statements in one batch produce separate results. `GO` repetition counts and sqlcmd directives are unsupported. Choose a database in the app instead of issuing `USE`.
- Cancelling SQL Server resets the tab's session, including its temporary tables and open transaction. Earlier auto-committed writes remain committed. The toolbar shows **session reset**; an interrupted native transaction can have an unknown outcome.
- **Output** shows SQL Server `PRINT` and `RAISERROR` messages at severity 10 or lower as they arrive, in order with each result's row count and every error, as SSMS shows them. A result that an error cuts short shows as failed. A severity from 1 to 9 shows as a WARNING and turns the **Output** button amber. Each `INSERT`, `UPDATE`, `DELETE`, `MERGE`, or `SELECT INTO` logs its count, as "5 rows affected", and the results header shows the last one. A batch that counts no rows, such as one under `SET NOCOUNT ON`, logs "Commands completed successfully".
- Every tab is a query editor. A tab opened from a table also browses that table. Tabs complete table and column names from the live catalog, including the columns after a table alias such as `o.`. The editor draws a rounded box behind the statement `⌘↵` will run. On PostgreSQL, a statement ends at `;`, or at a blank line when the next line starts a new statement. `⌘↵` or **Run** runs the selection as written, or the marked statement when nothing is selected. On a blank line nothing is marked, and `⌘↵` opens a list of the tab's statements instead. `⌘⇧↵` opens the list whenever the tab holds several statements. The list has an **All** row when there are several, and highlights in the editor what the row under the pointer or the arrow keys would run. `Enter`, a click, or a statement's number runs it and moves the cursor to its end. `Esc` closes the list. `Esc` or **Cancel** stops a running query.
- On PostgreSQL, `⌘⌥L` or **Format** formats the marked statement, or every statement the selection touches. Each clause starts a line, each list item and each `AND` or `OR` of a condition gets its own line, subqueries indent, and keywords are uppercase. Strings, comments, and function bodies stay byte for byte, and a column named like a keyword, such as `day`, keeps its case. Statements other than queries, `INSERT`, `UPDATE`, `DELETE`, and `CREATE TABLE` change only their keyword case. One `⌘Z` undoes the format. When the result might not mean the same, the editor keeps the text and says so.
- SQL Server formatting supports simple `SELECT` batches. Other constructs stay unchanged and show an unsupported message. Advanced table filters and sort expressions accept a conservative expression subset. Use a query tab for subqueries, casts, `CASE`, `COLLATE`, or functions outside that subset.
- Each tab has its own session, so `SET` and temp tables stay with the tab. The top bar shows where the tab runs: the environment icon and the connection, then the database. Click the connection name to pick the tab's connection, or the database name to pick one of the databases the tree shows. When the bar is narrow, the database name shortens before the connection name. Below a 900-pixel bar **Format** shows only its icon, and below 520 pixels it hides. `⌘⌥L` still formats. On PostgreSQL, the **Auto-commit** menu also offers app-managed Manual mode. The menu shows a confirmed PostgreSQL rollback or a SQL Server session reset after connection loss. Double-click a tab to rename it, unless it is bound to a file.
- **Commit** and **Rollback** appear while a transaction is open. The tab shows an amber dot, and the mode menu shows **in transaction** in amber. When the transaction is aborted, the dot turns red and the menu shows **aborted** in red. Closing that tab or disconnecting asks before it rolls back.
- `⌘O` opens `.sql` files, several at once, each in a new tab on the database `⌘T` would use. A file that is already open switches to its tab. Files must be UTF-8 and at most 4 MB. The editor shows neither a byte-order mark nor CRLF line breaks, and a save writes back the ones the file had.
- `⌘S` saves the tab to its file. In a tab with no file, `⌘S` asks where to save it, as **Save As** (`⌘⇧S`) does, and binds the tab to the new file. A tab bound to a file is named after the file, and a ring beside the name shows while its text differs from the file. With pending table edits in the tab, `⌘S` submits them instead.
- A save writes a new file beside the old one and renames it over the old one, so a crash never leaves half a file. A crash during a save can leave a hidden `.<name>.*.l1data-tmp` file in the folder. The new file keeps the old one's permission bits, but not its owner or group, its ACLs, or its extended attributes such as Finder tags, and other hard links to the old file keep the old text. A save writes through a symlink to its target. A read-only file is refused. If the file changed on disk since the tab opened or last saved it, the save asks before it overwrites. If you cancel, the tab shows the ring, since its file no longer holds what the tab last read. **Save As** refuses a file that another tab has open.
- Closing a tab whose text differs from its file asks **Save**, **Don't Save**, or **Cancel**. Every tab's text, saved or not, comes back after you close the window or quit with `⌘Q`. At launch, a tab with no unsaved changes reloads its file if the file changed. A tab whose file was moved or deleted keeps its text, and its name is struck through until a save writes the file again.
- SQL Server runs native scripts in **Auto-commit**, including explicit `BEGIN TRANSACTION`, `COMMIT` and `ROLLBACK`. The toolbar reports when a transaction is open; its nesting depth and committability are unavailable through the SQL Server protocol. App-managed Manual mode is unavailable. Server-side code such as triggers can commit or replace a transaction, so the app cannot guarantee that wrapping an arbitrary batch keeps every write rollbackable.
- An error shows the available server diagnostics, including PostgreSQL SQLSTATE, and the editor underlines a reported error location.
- The **Output** button at the right end of the results header opens a timestamped log of everything the tab ran: each statement, its server messages such as `RAISE NOTICE` and warnings, and its result line. **Commit**, **Rollback**, and table edits are logged too. When a statement returns no rows, the log fills the results pane. The button turns amber when the latest run raised a warning.
- **Query history** groups identical SQL per connection and database. Search SQL, connection names, databases, or errors, then filter by **All**, **Failed**, or **Writes**. Select a query to preview its SQL and paged run timeline. Copy it, open or run it in a new tab on its saved database, or insert it into the active tab.
- **Saved queries** keeps the queries you use often. History pruning and **Clear history** never remove them. `⌘⇧E` saves the selection, or the marked statement, and asks for a name. A blank name uses the query's first comment, else its first 72 characters. **Save query** in query history saves the query you are previewing. Saving the same SQL again opens the entry you already have, marked **Already saved**. The bookmark button at the bottom of the sidebar, or `⌘E`, opens the list with the most recently used query first. Type to filter by name or SQL. `Enter` inserts the query at the active tab's cursor, or opens it in a new tab when no query tab is open. `⌘↵` opens it in a new tab, on the active tab's database when the dialects match, else on a connection of the query's dialect, else with no connection. `⌘C` copies it. **Edit** renames the query or changes its SQL, and **Delete** asks before it removes the query. A saved query holds up to 64 KB of SQL.

### Results and table data

- SQL Server table browsing preserves exact decimal and money values, Unicode, binary values, and temporal precision. An arbitrary query returning raw `money` must cast it to `decimal(19,4)` because the driver otherwise rounds it through floating point. Unsupported value conversions fail explicitly.
- SQL Server filters compare typed values and search the text shown in the grid. Equality filters for text, ntext, and XML compare serialized text. Text searches on floating-point columns or with excessively long patterns are refused with guidance.
- The results grid streams large results in pages. Select cells and press `⌘C` to copy them as TSV. Press `Enter`, or click the **…** on a cut-off value, to see the full value.
- Right-click a foreign key cell, or press `⇧F10` on it, and choose **Go to referenced row in** the referenced table to open that table filtered to the row the key points at, with one filter per key column. When two keys from one cell reach the same table, each item names its key's columns. On PostgreSQL this works on any query whose key columns come straight from a table, and each copy of a key column follows its own row. On SQL Server it works on a table tab's own rows. A key with a `NULL` value cannot be followed. A key of several columns is not offered when the result shows a column of its table twice, as a self-join does, because the result cannot say which copy each column came from. A self-join that shows each key column only once can still mix two rows. If the referenced table already has a tab with no staged edits and its browse SQL unchanged, the follow replaces that tab's `WHERE`, `ORDER BY`, filters, and page. If that tab holds staged edits or edited SQL, the row opens in a second tab. The filter compares the text the grid shows, so a source session with a non-default `DateStyle`, `IntervalStyle`, or `extra_float_digits` can make a date, interval, or floating-point key find no row. The menu reads keys from the loaded catalog, as the primary key icons and staged edits do, so after schema changes made outside L1 Data, choose **Refresh catalog** in the connection's right-click menu before you follow a key.
- `⌘I`, or **View as record** in the grid's right-click menu, shows the selected row as a list of fields, each with its type glyph and its full value. It shows the saved values, not staged edits, and it is disabled on a staged new row. `↑` and `↓` move to the previous and next row.
- A run with several result sets has a selector to switch between them.
- **Export** writes a result as CSV, JSON, a Markdown table, or SQL `INSERT` statements. An `INSERT` export of one table keeps its identity values and leaves out computed columns, so the file replays into an empty copy of the table. Choose the loaded rows, or all rows when the grid holds only part of a read or one page of a table. All rows runs the statement again on the tab's own session, or reads the whole filtered table on a table tab, and streams every row to the file. On PostgreSQL the re-run happens in a read-only transaction that is always rolled back, so it cannot write or change your open transaction. A statement that may write offers only the loaded rows, and one that calls functions L1 Data cannot vouch for asks before it runs again. The results header shows the rows written so far, and **Cancel** or `Esc` stops the export. The file appears only when it is complete, so a cancelled or failed export leaves an existing file unchanged.
- A table tab pages through rows, from 10 to 1,000 per page. It filters with a `WHERE` condition or per-column filters, and sorts with `ORDER BY`. Click a header to sort by it, and Shift-click to add a column to the sort. SQL Server requires exact table column names for Shift-click sorting. If a raw name uses another spelling, edit `ORDER BY` or click without Shift to replace the simple column order.
- **Last page** fetches an exact filtered count. SQL Server counts use the table tab's session settings and include its uncommitted native writes. While SQL Server counts, the toolbar shows **Counting** and **Cancel**. **Cancel** or `Esc` resets that session. Page requests wait for the count, and the latest request wins. Changing the tab's target, query, text, or page intent while the count is pending discards the reply.
- A PostgreSQL table tab with a primary key can add, edit, and delete rows, and set a cell to `NULL` or `DEFAULT`. Changes stay pending until **Submit** (`⌘S`), which applies them in one transaction. In manual mode, submitted changes wait in the tab's open transaction until **Commit**. **Revert** asks, then drops them. If commit or rollback cannot be confirmed, the app asks you to verify saved data before retrying.
- SQL Server table results are read-only. Use SQL scripts to insert, update or delete rows. Staged grid writes are unavailable because server-side transaction control can defeat their all-or-nothing guarantee.

### The assistant

- SQL Server chats use the database's catalog and T-SQL context for suggestions. Model-written SQL cannot execute against SQL Server because it lacks the enforced read-only transaction used for PostgreSQL. Run a suggested SQL card in a normal query tab when you want to execute it.
- The right panel is a chat with Claude. It runs on your own Claude subscription through the `claude` CLI you have installed and signed in to, so L1 Data needs no API key. Each message sends Claude the active tab's connection, database, and SQL, the statement at the cursor or the selection, the last error, and an outline of the schemas the sidebar shows.
- On PostgreSQL, Claude reads the catalog through two tools that never ask. One describes tables, and the other searches table, column, and function names. It can also run a read-only query. The permission picker under the message box chooses when a query asks first, and a change applies from the next query. In **Ask first**, every query waits for **Run** or **Deny**. In **Auto**, a query runs without asking only when it is a read whose every function call and operator is a vetted built-in: aggregates, window functions, math, string, date and time, JSON, array, and catalog helpers such as `pg_size_pretty`, and the comparison, arithmetic, text, pattern, JSON, and array operators. A query that calls any other function or operator, uses a `U&` escape, or selects `INTO` asks first and names what it found. Views, functions, and casts to or from types defined in the database are not inspected, so your database role is the last limit on what a query can reach. Only `SELECT`, `WITH`, `VALUES`, `TABLE`, `SHOW`, and `EXPLAIN` run. Each query runs on its own session in a `READ ONLY` transaction with a 30-second timeout, and Claude sees at most 50 rows.
- SQL in a reply shows as a card that names the tables it uses. **Run in new tab** opens the SQL in a new tab and runs it. **Insert at cursor** inserts it into the active tab. **Replace tab** replaces the active tab's text as one edit that `⌘Z` undoes. A card whose SQL writes, or calls a function the app cannot vouch for, offers **Open in new tab** in place of **Run in new tab**, so it waits for you to run it.
- The paperclip attaches the result the active tab shows, up to 100 rows. Suggestion chips under a reply send their text as the next message. `Enter` sends, `Shift+Enter` starts a new line, and **Stop** ends the turn: it denies a waiting approval, asks the CLI to stop, and kills the process if it has not exited after 5 seconds. Quitting the app stops every turn the same way with a 1-second grace and kills any `claude` still running within about 4 seconds.
- The model and effort pickers list the models and effort levels your CLI offers. Switching models keeps the effort when the new model offers it. A new chat starts in **Ask first**, on the model and effort of your newest chat while the CLI still offers them. Chats are saved. A restart reopens the chat you had open, the history button lists the others, and the next message continues where the conversation left off. The history button shows a dot while a chat waits for your approval. If the CLI no longer has a chat's session, the next message restarts it with a summary of the conversation and says so. Deleting a chat asks first and removes it from L1 Data. The CLI keeps its own transcript of the conversation under its config directory, at `projects/<the app's assistant directory, with slashes as dashes>/<session id>.jsonl`.
- **Settings** > **Providers** shows the CLI's version, sign-in state, and plan, with a switch that turns the provider on or off. It also sets the display name, the binary path, the config directory, and the launch arguments. Launch arguments pass to the CLI, except the flags the app sets itself, which the field lists as ignored. `--model` and `--effort` there set the model and effort a new chat starts with. **Health check interval** under **Advanced** sets how often the app checks the CLI in the background. The default is 300 seconds, and 0 turns the check off.
- **Settings** > **Usage**, also opened by the chart button at the bottom of the sidebar, shows how much of each Claude and Codex subscription limit is left. Each window, such as the five-hour session or the week, gets a card with the percent left, a **Running low** warning, and the time until it resets. The app asks your own `claude` and `codex` CLIs for these numbers and reads no credentials itself. The refresh button at the top right reads them again, and the page also reads them again when a window resets while it is open. A CLI that is not installed shows as **not installed**, and a signed-out one shows the command that signs it in.

### Updates

- L1 Data follows one of two update tracks, **Stable** or **Nightly**, which you choose in **Settings** > **About**. A build whose version contains `-nightly.` follows Nightly until you choose. The first launch saves that track, so a nightly install that the Nightly feed later moves onto a stable release stays on Nightly.
- The round button at the right end of the sidebar footer shows the update status. Click it to check for an update, click again to download what the check found, and click a third time to restart into the new build. A ring shows the download's progress, and a toast says when the download is ready. Nothing downloads without a click. Once a check finds a build, hovering the button lists what changed in it and in each release since the running build.
- Before the restart, a dialog names what the restart ends: running queries, open transactions, table edits that haven't been submitted, and assistant turns. If more starts before you confirm, the dialog opens again with the longer list. While the update installs, a notice covers the window and takes no input, so nothing new starts before the restart. If the install fails, the notice goes away and the button shows the error.
- The app checks 15 seconds after launch, then every 4 hours, and an hour after a failed check. Switching tracks checks at once. A nightly build on the Stable track is offered the latest stable release even when that release is older, so you can leave Nightly. Every other check offers only a newer build. The track cannot change while a download or an install runs.
- **Settings** > **About** shows the running version, the update status, and a button for the same next step as the sidebar button. Development builds, and Linux installs other than the AppImage, do not update themselves and show no sidebar button.

### The window

- **Settings** (`⌘,`) opens in place of the sidebar and editor. **Appearance** picks a System, Light, or Dark scheme and the themes for each. It also sets the density (Compact, Standard, or Spacious), the speed of panel, dialog, and menu animations, and the interface and monospace fonts and sizes. The monospace font is used by the SQL editor and the result tables. **Queries** sets the row limit, the table page size, and whether a script stops at its first error. The search box above the sections filters the settings in place and shows the matches with their controls. `Esc` clears it and returns to the section you were on.
- On macOS the window keeps the native traffic lights. On Windows and Linux the app draws its own window controls and resize edges.

## Keyboard shortcuts

`Ctrl` works in place of `⌘` on Windows and Linux.

| Keys | Action |
|---|---|
| `⌘T` | New SQL tab |
| `⌘O` | Open `.sql` files |
| `⌘S` | Save the tab to its file, or submit pending table edits |
| `⌘⇧S` | Save the tab to a new file |
| `⌘N` | New connection |
| `⌘P` | Focus the sidebar search: the tree, or the settings while Settings is open |
| `⌘,` | Open or close Settings |
| `⌘W` | Close the tab, or close Settings |
| `⌘↵` | Run the selection, or the marked statement under the cursor |
| `⌘⇧↵` | Choose which statement to run, or all of them |
| `⌘⌥L` | Format the marked statement, or the statements the selection touches |
| `⌘E` | Open or close Saved queries |
| `⌘⇧E` | Save the selection, or the marked statement, to Saved queries |
| `Esc` | Cancel the running query |
| `Enter` | Edit the selected cell in an editable table, or show its full value elsewhere |
| `F2` | Edit the selected cell |
| `⌘⌫` | Delete or restore the selected rows |
| `⌘C` | Copy the selected cells as TSV |
| `⌘A` | Select all cells |
| `⌘I` | Show the selected row as a record |
| `⇧F10` | Open the selected cell's menu |

In query history, `↑` and `↓` select a query, `Enter` opens it in a new tab, `⌘↵` runs it in a new tab, and `Alt+Enter` inserts it at the active tab's cursor. `⌘C` copies the query unless text is selected in the search field. `Esc` closes history. **Clear history** asks for confirmation.

In Saved queries, `↑` and `↓` select a query, `Enter` inserts it at the active tab's cursor, `⌘↵` opens it in a new tab, and `⌘C` copies it unless text is selected in the search field. While you edit a query, `Enter` in the name field, or `⌘S` or `⌘↵` in the SQL, saves the edit. `Esc` leaves an edit or a confirmation first, and asks before it drops changed text. `Esc` from the list closes Saved queries.

On macOS, **Close Window** moves to `⇧⌘W` so that `⌘W` closes a tab. On macOS the global shortcuts take `⌘` only. `Ctrl` chords there stay with the editor.

## Quickstart

The repo is tested with Rust 1.93, Node 26, and pnpm 11. You also need Docker.

```sh
pnpm install
pnpm db:up          # Postgres 16 on localhost:5433 with the seeded `shop` database
pnpm tauri dev      # the desktop app
pnpm bridge         # dev-bridge on :7077 plus vite on :5173: the real engine from a browser
```

Connect to `postgres://l1data:l1data@localhost:5433/shop`. The seed also creates `lab.type_zoo`, a table with one column for each hard-to-render type.

For SQL Server development, run `pnpm db:mssql:up`. It starts SQL Server 2022 Developer on `localhost:1434` and seeds `shop` and `reporting`. Choose **SQL Server**, database `shop`, user `sa`, password `L1Data_dev_Str0ng`, and **Require** encryption for this self-signed test server. The container uses an amd64 image. The setup script can run again without duplicating seed data.

## Checks

```sh
cargo test --workspace   # engine units, bindings drift test, bridge coverage test
cargo test -p engine --features pg-tests   # engine against PostgreSQL
cargo test -p engine --features mssql-tests   # engine against SQL Server; needs pnpm db:mssql:up
cargo test -p engine --features ssh-tests   # engine through an SSH bastion; needs pnpm db:up and pnpm db:ssh:up
python3 dev/mssql-tls-smoke.py   # public Engine VerifyFull CA/name checks against a disposable SQL Server
pnpm lint                # oxlint over the frontend and scripts/
pnpm fmt:check           # oxfmt check of the frontend, scripts/, and root configs; pnpm fmt rewrites them
pnpm typecheck
pnpm test                # vitest, then the release scripts' node tests
pnpm build               # production frontend bundle
pnpm self-check          # boots the engine inside the Tauri binary, connects to the Docker database, prints JSON, exits
cargo build -p dev-bridge && dev/bridge-smoke.sh   # drives the bridge over HTTP against the Docker database
cargo build -p dev-bridge -p engine && python3 dev/mssql-smoke.py   # mixed PostgreSQL/SQL Server bridge checks; needs both databases and a free bridge port
cargo test -p engine --features claude-smoke --test claude_smoke -- --nocapture   # the assistant through your installed claude CLI
pnpm bindings            # rewrites src/ipc/bindings.ts after an IPC type or command changes
pnpm icons               # rewrites src-tauri/icons from src-tauri/app-icon.svg
pnpm screenshot          # macOS: rewrites docs/images/l1-data.png from the running app
```

The SQL Server TLS smoke needs Docker, OpenSSL, Rust, and the cached `mcr.microsoft.com/mssql/server:2022-latest` image. It builds the engine and a temporary public-API probe, starts its own container on a free loopback port, and uses generated fixture credentials and certificates. It checks trusted `localhost`, hostname rejection on `127.0.0.1`, unrelated-CA rejection, and **Require** controls. Each probe child gets isolated `SSL_CERT_FILE` and `SSL_CERT_DIR` settings. The script removes its container and temporary files on success or failure. It does not use the seeded databases or bridge.

`pnpm db:ssh:up` starts an OpenSSH bastion on `localhost:2222` that forwards only to `postgres:5432`, and to `mssql:1433` when `pnpm db:mssql:up` is up too. The `ssh-tests` SQL Server case runs only then. The bastion's host key and the client keys under `dev/sshd` are test-only fixtures.

A debug binary rewrites `src/ipc/bindings.ts` each time it starts, so `pnpm tauri dev` and `pnpm self-check` keep it current too.

The `claude-smoke` test needs the `claude` CLI installed and signed in, and the Docker database. It spends three Haiku turns on your Claude subscription, and one of them waits 75 seconds for an approval.

`pnpm screenshot` starts its own Postgres container on port 5434. It needs `swift`, `osascript`, `cliclick`, and `python3`, free ports 7077, 5173, and 5434, and Accessibility and Screen Recording permission for the terminal.

## Environment variables

| Variable | Effect |
|---|---|
| `L1_DATA_DATA_DIR` | The data directory. The app reads it only in debug builds. The dev-bridge uses a temporary directory without it. |
| `L1_DATA_SECRET_STORE=memory` | Keeps the master key in memory instead of the OS credential store. Debug builds only. |
| `L1_DATA_SELF_CHECK=1` | Runs the self-check and exits without opening a window. |
| `L1_DATA_SELF_CHECK_URL` | The database for the self-check. Default `postgres://l1data:l1data@localhost:5433/shop`. |
| `L1_DATA_TEST_PG` | The database for `pg-tests` and `claude-smoke`. Same default. |
| `L1_DATA_TEST_MSSQL` | SQL Server URL for the mixed bridge smoke. Default `mssql://sa:L1Data_dev_Str0ng@localhost:1434/shop`. |
| `L1_DATA_TEST_MSSQL_HOST` | Host for the SQL Server Rust integration tests. Default `localhost`. |
| `L1_DATA_TEST_MSSQL_PORT` | Port for the SQL Server Rust integration tests. Default `1434`. |
| `L1_DATA_TEST_MSSQL_PASSWORD` | The `sa` password for the SQL Server Rust integration tests. Default is the development fixture password above. |
| `L1_DATA_BRIDGE_PORT` | The port the dev-bridge listens on, and the port vite gives the page for it. `dev/bridge-smoke.sh` and `dev/mssql-smoke.py` use it too. Default `7077`. |
| `L1_DATA_VITE_PORT` | The port vite serves the UI on, and the only port the dev-bridge accepts requests from. Default `5173`. `pnpm tauri dev` needs the default. |
| `L1_DATA_AZ_BINARY` | The program the dev-bridge's engine runs instead of `az` on the PATH for Entra ID sign-in, such as `target/debug/fake-az`. A relative path is relative to the working directory. Only the dev-bridge reads it. The app ignores it. |
| `L1_FAKE_AZ_SCENARIO`, `L1_FAKE_AZ_LOG` | The scenario and call log of `fake-az`. AGENTS.md describes them. |
| `L1_DATA_UPDATE_SCENARIO` | The scripted update feed the dev-bridge serves: `current` (the default), `available`, `nightly-build`, `offline`, `bad-signature`, `install-fails`, or `off`. AGENTS.md describes each. |
| `UPDATE_BINDINGS=1` | Makes the bindings drift test rewrite `src/ipc/bindings.ts` instead of failing. |

## Layout

- `crates/engine` holds every behavior and every IPC type (`src/model.rs`). It has no Tauri dependency.
- `src-tauri` is the app shell. Each command is a one-line call into the engine.
- `crates/dev-bridge` serves the same engine over HTTP for browser-driven UI work.
- `src` is the React frontend. `src/ipc/bindings.ts` is generated, so do not edit it.
- `dev` holds the Docker database, its seed, and the bridge smoke, screenshot, and icon scripts.
- `docs` holds the architecture doc, [docs/releasing.md](docs/releasing.md) on publishing Stable and Nightly releases, and the README images.
