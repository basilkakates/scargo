# Container Runtime

## Goal and success criteria

- Add the minimum container runtime needed to deploy Scargo on a generic online
  platform or small VM.
- Reuse the existing Compose TimescaleDB service instead of creating a separate
  deployment stack.
- Keep app config environment-only.

Success means Scargo can be built into a production container image, run with
Compose beside `scargo_db` for local verification, and pass `/api/health`.

## Implementation instructions

1. Add a multi-stage `Dockerfile` that builds the release binary and copies only
   the runtime files needed by the server.
2. Add `.dockerignore` to exclude build outputs, git metadata, local env files,
   analysis outputs, and local data.
3. Extend `compose.yaml` with an optional app service or profile without breaking
   the existing DB-only workflow.
4. Configure the container app to bind `SCARGO_HTTP_HOST=0.0.0.0`.
5. In Compose, connect the app to `scargo_db` through the Compose network and set
   `POSTGRES_HOST=scargo_db`.
6. Keep production secrets injected by environment or ignored `.env` files only.
7. Update README and AGENTS with the new container build/run commands.

## Tools and commands to use

- `docker build -t scargo:local .`
- `docker compose up -d scargo_db`
- `docker compose --profile app up --build scargo`
- `cargo test`
- `git diff --check`

## Relevant files, data, and context

- `src/main.rs` serves `dashboard/static` from the runtime working directory.
- `compose.yaml` already defines `scargo_db` and the shared Compose network.
- `.env.example` documents non-secret placeholders only.

## Acceptance checks and tests

- `docker build -t scargo:local .` succeeds.
- Compose app starts and can reach the Compose database.
- `GET /api/health` returns `{"status":"ok"}` from the container.
- Existing `docker compose up -d scargo_db` still works for host-run development.

## Suggested branch name

- `feature/deployment-container-runtime`
