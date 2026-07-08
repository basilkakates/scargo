# Dropbox OAuth Incremental Ingest

## Goal and success criteria
- Restore the original Dropbox OAuth design with Full Dropbox visibility per Scargo account.
- Download and ingest only new Dropbox CSV revisions into the owning account.
- Avoid retaining CSV or ZIP artifacts outside PostgreSQL after ingest finishes.
- Let a signed-in non-guest user connect, disconnect, pause/resume, change the monitored folder, and queue manual sync from the dashboard.
- Keep exact 17-character VIN folder handling conservative and offline-first.

Success means:
- Scargo can run as a deployed web server and poll saved Dropbox OAuth connections without a local sync agent.
- Refresh tokens are stored encrypted and never returned to the browser.
- A Dropbox folder shaped as `<root>/<VIN-or-key>/*.csv` ingests through the existing account-scoped CSV helper.
- Duplicate Dropbox revisions and duplicate file content do not create duplicate telemetry rows.
- Manual sync queues background work instead of blocking the browser request.
- CSV bytes are streamed from Dropbox into ingest without leaving retained files on disk.

## Implementation instructions
1. Start from `feature/shared-link-ingest`.
2. Add Dropbox OAuth config in `src/config/settings.rs`:
   - enable flag, app key, app secret, base URL, token encryption key, poll interval, default root path.
3. Add Dropbox tables in `src/db/migrate.rs`:
   - `dropbox_connection` for encrypted refresh token, root path, cursor, status, sync state, timestamps, and latest error.
   - `dropbox_oauth_state` for hashed short-lived browser state.
   - `dropbox_ingest_file` for path, revision, content hash, upload id, status, and per-file errors.
4. Add OAuth and connection APIs under `/api/dropbox/*`:
   - start OAuth, callback, inspect connection, update folder, pause/resume, queue manual sync, delete connection.
   - require a signed-in non-guest dashboard session for all management routes.
5. Add a server-side Dropbox worker using list-folder cursors plus per-file download:
   - only direct `<root>/<vehicle-key>/*.csv` files ingest.
   - root-level CSV files and deeper nested CSV files are skipped with visible status.
   - non-CSV files are ignored.
6. Feed accepted CSV bytes into `ingest_csv_for_account(db, account_id, vehicle_key, bytes, "dropbox-oauth")`.
7. Skip files whose `(connection, path, revision)` already reached `ingested` or `duplicate`.
8. Keep manual offline scripts for VIN repair and cohort metadata; runtime sync should not depend on ZIP staging or local Dropbox mirrors.
9. Update `/dropbox.html` and `/dropbox.js` for OAuth connect/disconnect, folder changes, pause/resume, sync-now, status, counts, and latest error.
10. Update `README.md`, `AGENTS.md`, `.env.example`, and privacy docs for the OAuth design.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Use lean-ctx reads/searches for current API, schema, dashboard, and VIN script behavior.
- Format and validate:
  - `cargo fmt --check`
  - `cargo test`
  - `git diff --check`

## Relevant files, data, and context
- `src/api/ingest.rs::ingest_csv_for_account` is the reusable account-scoped CSV ingestion entrypoint.
- `src/db/migrate.rs` is clean schema bootstrap only; do not add legacy migration rewrites.
- OBD Fusion exports commonly live under a Dropbox app folder, which requires Full Dropbox OAuth visibility from Scargo.
- The v1 source limit is one Dropbox connection per account.

## Acceptance checks and tests
- Unit tests cover OAuth state validation, root-path normalization, Dropbox path mapping, root CSV skip, nested CSV skip, duplicate ledger skip, and response shape.
- First sync ingests direct CSV children for the owning account.
- Second sync with unchanged revisions ingests zero rows.
- Manual sync returns immediately and marks the connection queued or running.
- Guest and bearer-token requests cannot manage Dropbox connections.
- Docs describe Full Dropbox access, incremental sync, and no retained CSV or ZIP artifacts.

## Suggested branch name
- `feature/shared-link-ingest`
