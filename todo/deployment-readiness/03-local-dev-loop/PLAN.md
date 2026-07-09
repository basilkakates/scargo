# Local Dev Loop

## Goal and success criteria

- Make it easy to run a dev Scargo instance while editing the app.
- Keep the fast path as host-run Rust plus Compose-managed TimescaleDB.
- Avoid adding watcher dependencies unless the plain workflow is not enough.

Success means a fresh checkout can start the DB, run the app locally, edit code,
stop/re-run `cargo run`, and reset the local DB without touching production data.

## Implementation instructions

1. Add a minimal helper script only if it removes repeated manual setup.
2. The helper should load ignored `.env` when present, provide the local
   `POSTGRES_PASSWORD=scargo` default, start `scargo_db`, wait for readiness, and
   run `cargo run`.
3. Keep rebuilding explicit: after code changes, stop and rerun the dev command.
4. Do not add `cargo-watch`, npm, live reload, or a dev container unless a later
   task proves the plain loop is painful.
5. Update README and AGENTS with the dev loop and reset workflow.

## Tools and commands to use

- `docker compose up -d scargo_db`
- `cargo run`
- `scripts/reset-dev-db.sh`
- `cargo test`
- `git diff --check`

## Relevant files, data, and context

- `scripts/reset-dev-db.sh`
- `scripts/smoke-docker.sh`
- `compose.yaml`
- `.env.example`

## Acceptance checks and tests

- Dev app starts on `http://localhost:8080` with the Compose DB.
- The helper does not require committed secrets.
- The helper does not set `SCARGO_ENV=production`.
- Reset workflow remains separate and clearly destructive only to the local
  Compose volume.

## Suggested branch name

- `feature/deployment-local-dev-loop`
