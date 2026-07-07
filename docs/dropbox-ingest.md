# Dropbox Ingest

Dropbox ingest lets a signed-in Scargo account connect one Dropbox account and
sync OBD Fusion CSV exports without moving remote files. It is off by default.

## Dropbox App Setup

1. Create a Dropbox app for the deployment.
2. Set the OAuth redirect URI to:
   `${SCARGO_BASE_URL}/api/dropbox/oauth/callback`
3. Enable offline access. Scargo requests `token_access_type=offline` so it can
   refresh access tokens for background polling.
4. Grant file read access for the app's visible files. The worker lists folders
   and downloads CSV files through Dropbox `files/list_folder`,
   `files/list_folder/continue`, and `files/download`.
5. Store the app key and secret in ignored environment config, not tracked files.

## Environment

Local `.env` example:

```env
SCARGO_DROPBOX_ENABLED=true
DROPBOX_APP_KEY=your-dropbox-app-key
DROPBOX_APP_SECRET=your-dropbox-app-secret
SCARGO_BASE_URL=http://localhost:8080
SCARGO_TOKEN_ENCRYPTION_KEY=32-byte-hex-or-base64-value
SCARGO_DROPBOX_POLL_SEC=300
```

Production must also set `SCARGO_ENV=production` and `SCARGO_DATABASE_URL`.
`SCARGO_BASE_URL` must be the public origin users reach in the browser. The
encryption key must decode to exactly 32 bytes as hex or base64.

## Source Folder

v1 reads one fixed Dropbox root:

```text
/OBD Fusion/CsvLogs/
  DEMO-HONDA-ACCORD/
    CSVLog_20260327_185401.csv
```

The direct child folder is the VIN or non-identifying vehicle key. The worker
ingests only `/OBD Fusion/CsvLogs/<VIN>/*.csv`.

Root-level CSV files under `/OBD Fusion/CsvLogs` are skipped because no vehicle
key is available. Nested CSV paths below the VIN folder are also skipped in v1.
Scargo records skipped file status so the dashboard can show the latest error.

## Account And Privacy Boundary

- One Dropbox connection is stored per Scargo account.
- Guest sessions and bearer upload tokens cannot manage Dropbox connections.
- OAuth refresh and access tokens are encrypted at rest.
- Tokens are never returned to the browser.
- Dropbox files ingest only into the Scargo account that connected Dropbox.
- Raw telemetry stays subject to normal account-scoped read rules.

## Duplicate Guarantees

Dropbox state and Scargo upload state both participate in de-duplication:

- `dropbox_connection.cursor` stores the latest completed Dropbox cursor.
- `dropbox_ingest_file` records path, rev, status, rows, duplicate state, and errors.
- Already ingested or duplicate `(connection, path, rev)` entries are skipped.
- `ingest_upload` enforces one content hash per vehicle, preventing duplicate
  telemetry rows even if a file appears at a new Dropbox path.
- Dropbox files are not moved, renamed, or archived remotely in v1.

These tables must live on persistent database storage and be included in normal
database backups. Losing them can force OAuth reconnects and replay Dropbox file
scans, although `ingest_upload` still protects already-loaded telemetry rows.

## Manual Smoke Flow

1. Start Scargo with Dropbox enabled and a persistent database.
2. Register or log in as a non-guest user.
3. Open `/vehicles.html` and connect Dropbox.
4. Add one small CSV under `/OBD Fusion/CsvLogs/DEMO-HONDA-ACCORD/`.
5. Click `Sync now` or wait for `SCARGO_DROPBOX_POLL_SEC`.
6. Confirm `/api/dropbox/connection` reports `connected=true` and updated
   `last_success_at`, `ingested_count`, or `duplicate_count`.
7. Confirm `/api/vehicles` and the dashboard show the uploaded vehicle data.

## Operational Checks

- Verify `SCARGO_DROPBOX_ENABLED=true` only where Dropbox should run.
- Verify the Dropbox app redirect URI exactly matches deployment base URL plus
  `/api/dropbox/oauth/callback`.
- Verify the database volume and backups include OAuth tokens, cursors, and
  file dedupe rows.
- Inspect `/api/dropbox/connection` from a signed-in browser session for current
  status, root path, counts, and latest error.
- Watch app logs for sync errors, but do not log OAuth tokens or CSV contents.
