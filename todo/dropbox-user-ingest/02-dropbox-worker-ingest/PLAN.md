# Dropbox Worker Ingest

## Goal and success criteria
- Poll each active account-owned Dropbox connection for `/OBD Fusion/CsvLogs`.
- Treat each direct child folder under `CsvLogs` as the VIN or vehicle key.
- Ingest only CSV files below a VIN folder, for example `/OBD Fusion/CsvLogs/<VIN>/*.csv`.
- Skip CSV files directly under `/OBD Fusion/CsvLogs` and surface a visible status error.
- Reuse the same account-scoped ingest logic as `POST /api/ingest/csv`.
- Persist cursor and file state so repeated worker passes do not duplicate rows.

Success means:
- Scargo launches a background Dropbox worker only when `SCARGO_DROPBOX_ENABLED=true`.
- First sync ingests new CSV files below VIN folders for the owning account.
- Later syncs resume from Dropbox cursor and ingest only new or changed CSV files.
- Root-level CSV files are skipped and recorded as an actionable connection/file error.
- Duplicate Dropbox files and duplicate CSV content produce zero new metric rows.

## Implementation instructions
1. Extract the core of `src/api/ingest.rs::upload_csv` into a reusable helper that accepts:
   - `Database`
   - `account_id`
   - `vin`
   - CSV bytes
   - content type or source label
2. Keep `/api/ingest/csv` behavior unchanged by calling the shared helper after resolving the request account.
3. Return a structured ingest result from the helper:
   - upload id when available
   - `rows_ingested`
   - duplicate flag
   - vehicle id
4. Ensure the helper still:
   - decodes VIN metadata locally
   - creates or updates `vehicle`
   - inserts or reuses `ingest_upload` by `(vehicle_id, content_hash)`
   - links upload to the owning account through `account_vehicle_upload`
   - deletes the new `ingest_upload` if CSV parsing fails
5. Add Dropbox client code behind a small trait or interface so tests can use a fake Dropbox implementation.
6. Poll `dropbox_connection` rows with `status='active'`.
7. Use Dropbox list-folder and continue APIs for the fixed root path `/OBD Fusion/CsvLogs`.
8. Store and reuse the Dropbox cursor on successful list completion.
9. Map Dropbox entries:
   - `CsvLogs/<VIN>/<file>.csv` -> ingest with `vin=<VIN>`
   - `CsvLogs/<VIN>/nested/<file>.csv` -> either skip for v1 or document and test intentional handling
   - `CsvLogs/<file>.csv` -> skip with latest error because no VIN folder exists
   - deleted entries -> mark file state deleted without deleting Scargo telemetry
10. Add or complete `dropbox_ingest_file` schema:
    - `id UUID PRIMARY KEY`
    - `connection_id UUID NOT NULL REFERENCES dropbox_connection(id)`
    - `account_id UUID NOT NULL REFERENCES account(id)`
    - `dropbox_file_id TEXT`
    - `path_lower TEXT NOT NULL`
    - `rev TEXT`
    - `content_hash TEXT`
    - `vin TEXT`
    - `upload_id UUID REFERENCES ingest_upload(id)`
    - `status TEXT NOT NULL`
    - `rows_ingested BIGINT NOT NULL DEFAULT 0`
    - `duplicate BOOLEAN NOT NULL DEFAULT FALSE`
    - `latest_error TEXT`
    - `seen_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`
    - `ingested_at TIMESTAMPTZ`
    - unique index on `(connection_id, path_lower, rev)`
    - index on `(connection_id, dropbox_file_id)`
11. Before download, skip a file when the same connection/path/rev is already ingested or duplicate.
12. After download, also rely on `ingest_upload` content hash to prevent duplicate rows across renamed or repeated files.
13. Update connection status fields after each worker pass:
    - `last_sync_at`
    - `last_success_at`
    - `latest_error`
    - `status`
14. Treat transient Dropbox failures as retryable and keep existing cursor.
15. Avoid logging tokens, file contents, exact access tokens, or account secrets.
16. Add unit tests for path mapping, root CSV skip, cursor persistence, duplicate state, changed rev, deleted file state, and transient API failure.
17. Add an integration-style test with a fake Dropbox client and Scargo account proving the second worker pass ingests zero duplicates.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Review current ingest and bulk patterns:
  - `ctx_read src/api/ingest.rs`
  - `ctx_read src/ingest/csv.rs`
  - `ctx_read src/api/privacy.rs`
  - `ctx_read src/bulk.rs`
  - `ctx_read src/db/migrate.rs`
  - `ctx_read src/main.rs`
- Format and validate:
  - `cargo fmt`
  - `cargo test`
  - `cargo test --test smoke_stack -- --ignored --nocapture`

## Relevant files, data, and context
- `src/api/ingest.rs` currently owns HTTP CSV upload, duplicate content hashing, account upload linking, and error cleanup.
- `src/ingest/csv.rs` owns parsing, raw metric insert, and daily rollup maintenance.
- `src/bulk.rs` already contains vehicle-folder CSV traversal and duplicate handling patterns.
- `src/main.rs` starts the Actix server and is the likely place to spawn the worker task after config and DB bootstrap.
- `src/db/migrate.rs` must remain current-schema bootstrap; do not add legacy data rewrites to startup.
- Dropbox remote files must not be moved or archived in v1.

## Acceptance checks and tests
- `cargo test`
- Fake Dropbox first sync ingests files under `/OBD Fusion/CsvLogs/<VIN>/*.csv`.
- Fake Dropbox second sync with same cursor/file state ingests zero rows.
- New CSV arrival after cursor resume ingests once.
- Changed rev of the same Dropbox file is evaluated and either ingested or marked duplicate by content hash.
- Root-level CSV under `/OBD Fusion/CsvLogs` is skipped and recorded with a visible error.
- Deleted Dropbox file marks file state deleted and does not delete local telemetry.
- Transient Dropbox API failure leaves cursor unchanged and retries later.
- Existing `/api/ingest/csv?vin=VIN` responses remain compatible.

## Suggested branch name
- `feature/dropbox-worker-ingest`
