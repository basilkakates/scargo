# Deploy Runbook And CI

## Goal and success criteria

- Document the production deployment workflow after the database and container
  path are chosen.
- Add only the CI checks needed to keep that workflow from silently breaking.

Success means an agent or maintainer can build, configure, deploy, verify,
rebuild, rollback, and smoke-check Scargo without searching chat history.

## Implementation instructions

1. Add `docs/deployment.md`.
2. Cover required production env: `SCARGO_ENV=production`, `SCARGO_DATABASE_URL`,
   `SCARGO_HTTP_HOST`, `SCARGO_HTTP_PORT`, Dropbox OAuth vars, and token
   encryption key.
3. Include deployment steps for the chosen v1 path from task 01 and the container
   output from task 02.
4. Include health checks, log checks, database backup/restore expectations,
   Dropbox redirect URI setup, image rebuild/restart steps, and rollback.
5. Update README and AGENTS to point at the runbook.
6. Add a CI image-build check only after a Dockerfile exists.

## Tools and commands to use

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test`
- `docker build -t scargo:ci .`
- `git diff --check`

## Relevant files, data, and context

- `.github/workflows/ci.yml`
- `README.md`
- `AGENTS.md`
- `.env.example`
- `docs/deployment-options.md`

## Acceptance checks and tests

- The runbook names every required production secret without providing real values.
- The runbook explains how to verify `/api/health` after deploy.
- The runbook keeps raw vehicle CSVs out of the repo and outside persistent app
  storage.
- CI fails if the production image cannot be built once container support exists.

## Suggested branch name

- `docs/deployment-runbook-ci`
