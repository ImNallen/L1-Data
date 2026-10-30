# AGENTS.md

Instructions for AI agents working on L1 Data, a desktop SQL IDE for PostgreSQL and SQL Server. It has a Rust engine, a Tauri 2 shell, and a React frontend.

## Read first

- [README.md](README.md): features, commands, and checks.
- [docs/architecture.md](docs/architecture.md): the design. Read its "Load-bearing decisions" before you change how data moves between Rust and the webview.

## Keep docs in their place

- `README.md` is for humans: what the app does, and how to build, run, and test it.
- `AGENTS.md` is for agents: only what the README and the code don't already say.
- Don't copy text between them. Link to it.

## Prove your change works

Passing checks don't prove a change works.

1. Run the README's "Checks" that cover the change.
2. Exercise the change itself.
3. For a UI change, run `pnpm bridge` to drive the real UI against the real engine. Capture screenshots or a video before and after.
4. Report what you ran, what you saw, and what you did not test.

## Monitor your PRs

- Watch every PR you open until its checks pass and its reviews are answered. Fix failures without being asked.
- With more than one PR open, watch them together and wait until all are done.

## Answer concisely

Give all the information in the fewest, shortest sentences that carry it.
