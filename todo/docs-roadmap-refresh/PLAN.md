# Docs Roadmap Refresh

## Goal and success criteria

Keep Scargo docs consistent, organized, and tied to the hosted-beta roadmap.

Success means README, AGENTS, docs, and active todo folders agree on current
behavior, defaults, active work, and where future agents should look first.

## Implementation instructions

1. Add or update `docs/roadmap.md` as the app-direction index.
2. Keep `README.md` focused on user-facing setup, current capabilities, and
   links to deeper docs.
3. Keep `AGENTS.md` focused on repo structure, workflow, APIs, env vars, and
   developer context.
4. Remove stale active-plan references, especially completed Dropbox/shared-link
   wording.
5. Align documented defaults with code and `.env.example`.
6. Do not change runtime behavior in this docs-only task.

## Tools and commands to use

- `git status --short --branch`
- `rg -n "SCARGO_DROPBOX_POLL_SEC|roadmap" README.md AGENTS.md .env.example docs todo --glob '!todo/completed/**'`
- `rg -n "shared-link-ingest" README.md AGENTS.md docs --glob '!todo/completed/**'`
- `git diff --check`

## Relevant files, data, and context

- `README.md`
- `AGENTS.md`
- `.env.example`
- `docs/`
- `todo/deployment-readiness/`
- `src/config/settings.rs`

## Acceptance checks and tests

- `docs/roadmap.md` links to the major supporting docs and active todo sets.
- README and AGENTS do not claim completed todo folders are active.
- `SCARGO_DROPBOX_POLL_SEC` docs match the implemented default.
- `git diff --check` passes.

## Suggested branch name

- `docs/docs-roadmap-refresh`
