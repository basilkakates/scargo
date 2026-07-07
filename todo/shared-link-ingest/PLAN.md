# Shared Link Ingest

## Goal and success criteria
- Replace Dropbox OAuth with headless ingest from one user-provided Dropbox shared folder link per Scargo account.
- Avoid Dropbox app keys, app secrets, OAuth callbacks, refresh tokens, and Full Dropbox access.
- Let a signed-in non-guest user save, replace, delete, pause/resume, and sync one shared folder source from the dashboard.
- Ingest direct CSV children from folders inside that shared folder into the owning Scargo account.
- Fetch and cache NHTSA vPIC metadata automatically for exact 17-character VIN folder names, without running external scripts.

Success means:
- Scargo can run as a deployed web server and poll saved shared links without a local sync agent.
- The shared link is stored encrypted and is never returned to the browser after save.
- A shared folder shaped as `<shared-root>/<VIN-or-key>/*.csv` ingests through the existing account-scoped CSV helper.
- Duplicate file content does not create duplicate telemetry rows.
- Exact VIN folder names populate cached public metadata when vPIC is reachable.
- Non-VIN vehicle-key folders still ingest, but do not trigger external VIN fetches.

## Implementation instructions
1. Start from `feature/shared-link-ingest`.
2. Keep the removed Dropbox OAuth feature deleted. Do not reintroduce Dropbox app credentials, OAuth state, refresh tokens, cursor sync, or Full Dropbox permissions.
3. Add a generic shared-ingest source table in `src/db/migrate.rs`, one row per account for v1:
   - account id, encrypted shared link URL, status, poll interval or next poll time, last sync timestamps, latest error, created/updated timestamps.
4. Add a per-source file ledger table keyed by source id, archive path, and content hash:
   - source id, account id, path, vehicle key, content hash, upload id, status, rows ingested, latest error, seen/ingested timestamps.
5. Add a VIN decode cache table:
   - VIN, status, year, make, model, engine family, raw response JSON, source, fetched_at, next_retry_at, latest_error.
   - Cache both successes and failures; retry failures only after `next_retry_at`.
6. Add shared-link API routes under `/api/ingest-sources/shared-link`:
   - `GET` returns enabled/connected status, redacted link label, sync counts, timestamps, and latest error.
   - `PUT` saves or replaces the current user's link after validating it looks like a Dropbox shared URL.
   - `DELETE` removes the current user's source and encrypted URL, leaving ingested telemetry intact.
   - `POST /pause` toggles active/paused.
   - `POST /sync-now` runs one sync pass for the current user.
   - All routes require a signed-in non-guest session; bearer upload tokens and guests cannot manage shared links.
7. Add a server-side worker that polls active shared-link sources when enabled by config.
8. Fetch the shared folder as a downloadable archive or equivalent server-readable listing from the shared link.
9. Scan archive entries with the contract:
   - `<vehicle-key>/<file>.csv` ingests.
   - root-level CSV files are skipped with visible status because no vehicle key exists.
   - nested CSV files below the vehicle folder are skipped for v1.
   - non-CSV files are ignored.
10. Feed accepted CSV bytes into `ingest_csv_for_account(db, account_id, vehicle_key, bytes, "shared-link")`.
11. Before ingesting, skip files whose source/path/content hash already reached `ingested` or `duplicate`.
12. After ingest, record rows, duplicate state, upload id, and latest per-file error in the ledger.
13. For every direct child folder whose name is a valid 17-character VIN:
   - check the VIN decode cache first.
   - if missing or retryable, call the official NHTSA vPIC API from the worker.
   - normalize make, model, model year, displacement, cylinder layout, electrification, and engine family using the existing script logic as the source behavior.
   - update `vehicle` metadata only when the fetched field is non-empty and the current vehicle field is empty or less specific.
14. Keep manual offline scripts for bulk repair, but shared-link VIN folders must not require `scripts/fetch-vin-decodes.py` or `scripts/backfill-vehicle-metadata.py`.
15. Add dashboard controls near account/upload controls:
   - shared link input, save, delete, pause/resume, sync now, status, last sync/success, ingested count, duplicate count, latest error.
   - Never render the full stored shared link after save; show only a redacted label.
16. Update `README.md`, `AGENTS.md`, `.env.example`, and privacy docs for the new shared-link design.
17. Document that anyone with the shared link can read the Dropbox folder, so users should share a narrow OBD Fusion `CsvLogs` folder and revoke the link in Dropbox to cut access.

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
- `scripts/fetch-vin-decodes.py` and `scripts/backfill-vehicle-metadata.py` contain the NHTSA/vPIC normalization behavior to port into Rust.
- OBD Fusion exports commonly live under a Dropbox app folder, which Dropbox App-folder OAuth for Scargo cannot read.
- The v1 source limit is one shared folder per account.

## Acceptance checks and tests
- Unit tests cover shared-link URL validation, archive path mapping, root CSV skip, nested CSV skip, duplicate ledger skip, and redacted response shape.
- Worker tests use a fake shared-link fetcher and fake VIN metadata fetcher.
- First fake sync ingests direct CSV children for the owning account.
- Second fake sync with unchanged content ingests zero rows.
- Exact 17-character VIN folder triggers one VIN metadata fetch and cache write.
- Repeated sync uses cached VIN metadata and does not call NHTSA again.
- Non-VIN vehicle-key folder ingests without calling NHTSA.
- Guest and bearer-token requests cannot save, inspect, or sync shared links.
- Docs contain no Dropbox app key, Dropbox app secret, OAuth callback, or Full Dropbox setup.

## Suggested branch name
- `feature/shared-link-ingest`
