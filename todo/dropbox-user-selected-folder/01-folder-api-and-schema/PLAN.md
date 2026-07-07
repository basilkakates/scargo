# Dropbox Folder API and Schema

## Goal and success criteria
- Let each signed-in Scargo account choose the Dropbox folder that contains vehicle log folders.
- Store the selected folder path on that account's existing Dropbox connection.
- Keep the current one-connection-per-account OAuth/token model.
- Preserve token secrecy and account ownership boundaries.

Success means:
- `POST /api/dropbox/oauth/start` accepts an optional `root_path` and carries it through OAuth state.
- `GET /api/dropbox/oauth/callback` stores the selected `root_path` on `dropbox_connection`.
- `GET /api/dropbox/connection` returns the stored `root_path`.
- `POST /api/dropbox/connection/folder` updates only the signed-in account's folder path.
- Updating the folder clears `cursor`, `latest_error`, `last_sync_at`, and `last_success_at` so the next sync scans the new root.
- Guests and bearer upload tokens cannot create, update, or inspect Dropbox connections.

## Implementation instructions
1. Start from `feature/dropbox-user-folder-api`.
2. Inspect the current Dropbox implementation before editing:
   - `git status --short --branch`
   - `ctx_read src/api/dropbox.rs`
   - `ctx_read src/dropbox_worker.rs`
   - `ctx_read src/db/migrate.rs`
   - `ctx_read dashboard/static/app.js`
3. Add `root_path: Option<String>` to the OAuth start request body.
4. Add `root_path TEXT NOT NULL` to `dropbox_oauth_state`, or an equivalent pending-state storage field.
5. Normalize Dropbox root paths in one server-side helper:
   - trim whitespace
   - require a leading `/`
   - reject empty values and `/`
   - remove trailing slashes except for `/`, which is rejected
   - preserve internal spaces and case for display
6. Use `/OBD Fusion/CsvLogs` only when no `root_path` is supplied, for compatibility with the existing UI and docs.
7. Store the normalized path in OAuth state and then in `dropbox_connection.root_path` during callback upsert.
8. Add `POST /api/dropbox/connection/folder`:
   - request body: `{ "root_path": "/Some Dropbox Folder" }`
   - requires Dropbox support enabled
   - requires a signed-in non-guest account session
   - updates only `WHERE account_id = $1`
   - clears cursor and sync status fields
   - returns the same connection response shape as `GET /api/dropbox/connection`
9. Keep `DropboxConfig.root_path` only as a default suggestion, or replace it with a constant if simpler.
10. Do not add a Dropbox picker, multi-folder support, or CSV-content VIN detection in this task.

## Relevant files, data, and context
- `src/api/dropbox.rs` owns OAuth start/callback, connection status, pause/resume, sync-now, and disconnect.
- `src/dropbox_worker.rs` already reads `dropbox_connection.root_path`; it should not need a schema redesign.
- `src/db/migrate.rs` is clean schema bootstrap for the current shape, not a legacy migration layer.
- The vehicle identity contract remains: selected root contains direct child folders named by VIN or vehicle key.

## Acceptance checks and tests
- `cargo fmt --check`
- `cargo test`
- Unit tests cover path normalization for blank, `/`, missing slash, trailing slash, and `/OBD Fusion/CsvLogs/`.
- OAuth state tests prove selected root path survives callback storage.
- API tests or focused helper tests prove folder update is account-scoped and rejects guests.
- Existing `/api/dropbox/connection/pause`, `/sync-now`, and `DELETE /connection` behavior remains compatible.

## Suggested branch name
- `feature/dropbox-user-folder-api`
