# Dropbox Docs and Deploy Ops

## Goal and success criteria
- Document Dropbox app setup, required environment, redirect URL, folder layout, and duplicate guarantees.
- Keep user-facing setup docs, agent context, safe env examples, and deployment notes aligned with implemented Dropbox behavior.
- Make it clear that Dropbox ingest requires persistent database storage for encrypted tokens, cursors, and file state.

Success means:
- `README.md` explains how to enable Dropbox ingest locally and in production.
- `AGENTS.md` includes Dropbox architecture, env vars, API endpoints, DB tables, and worker behavior.
- `.env.example` lists safe placeholders for all Dropbox-related config.
- Deployment docs or a new `docs/dropbox-ingest.md` describe Dropbox app configuration and operational checks.

## Implementation instructions
1. Update `README.md` with:
   - Dropbox ingest overview
   - Dropbox app setup steps
   - redirect URL format: `${SCARGO_BASE_URL}/api/dropbox/oauth/callback`
   - required app permissions/scopes chosen by implementation
   - local enablement with `.env`
   - fixed OBD Fusion folder layout
   - manual smoke flow
2. Update `AGENTS.md` with:
   - project tree entries for new Dropbox modules
   - env var table rows
   - API endpoint table rows
   - DB schema table rows
   - worker startup behavior
   - duplicate guarantees and path mapping
3. Update `.env.example` with safe placeholders only:
   - `SCARGO_DROPBOX_ENABLED=false`
   - `DROPBOX_APP_KEY=`
   - `DROPBOX_APP_SECRET=`
   - `SCARGO_BASE_URL=http://localhost:8080`
   - `SCARGO_TOKEN_ENCRYPTION_KEY=`
   - `SCARGO_DROPBOX_POLL_SEC=300`
4. Add `docs/dropbox-ingest.md` if the README would become too long.
5. Document the fixed OBD Fusion source path:
   - `/OBD Fusion/CsvLogs/<VIN>/*.csv`
   - child folder name is the VIN or vehicle key
   - root-level CSV files are skipped
   - nested folder handling must match the worker implementation
6. Document account and privacy boundaries:
   - one Dropbox connection per Scargo account in v1
   - OAuth tokens encrypted at rest
   - tokens never returned to browser
   - Dropbox files ingest into the connecting Scargo account only
7. Document duplicate guarantees:
   - Dropbox file state prevents repeated path/rev processing
   - `ingest_upload` content hash prevents duplicate telemetry rows
   - Dropbox files are not moved or archived remotely in v1
8. Document operational checks:
   - verify `SCARGO_DROPBOX_ENABLED=true`
   - verify callback URL in Dropbox app config
   - verify persistent database volume/backups
   - inspect connection status through `/api/dropbox/connection`
   - watch logs for sync errors without exposing secrets
9. Keep examples free of real secrets, VINs that look like private production data, and access tokens.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Review docs and implemented API before writing:
  - `ctx_read README.md`
  - `ctx_read AGENTS.md`
  - `ctx_read .env.example`
  - `ctx_read docs/privacy-model.md`
  - `ctx_read src/config/settings.rs`
  - `ctx_read src/api/dropbox.rs`
  - `ctx_read src/db/migrate.rs`
- Validate docs and code after edits:
  - `cargo test`
  - `git diff --check`

## Relevant files, data, and context
- `README.md` is the primary user-facing setup and local workflow document.
- `AGENTS.md` is the durable agent/developer context and must stay current with API, schema, config, script, dashboard, or ingest changes.
- `.env.example` is tracked and must use placeholders only.
- `docs/privacy-model.md` should be updated if Dropbox ingest changes privacy/account scoping details.
- Add `docs/dropbox-ingest.md` only when the implementation details do not fit cleanly in existing docs.

## Acceptance checks and tests
- `cargo test`
- `git diff --check`
- README has Dropbox setup, redirect URL, env vars, folder layout, and smoke test.
- AGENTS has Dropbox project structure, schema, endpoints, env vars, and worker notes.
- `.env.example` contains all Dropbox config keys with safe placeholder values.
- Docs state that persistent DB storage is required for OAuth tokens, cursors, and file dedupe.
- Docs state root-level CSV files under `/OBD Fusion/CsvLogs` are skipped.
- Docs state tokens are encrypted at rest and never returned to the browser.

## Suggested branch name
- `docs/dropbox-deploy-ops`
