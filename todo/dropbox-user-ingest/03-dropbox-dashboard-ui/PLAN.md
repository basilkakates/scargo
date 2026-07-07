# Dropbox Dashboard UI

## Goal and success criteria
- Add dashboard controls for account-owned Dropbox ingest.
- Let signed-in users connect Dropbox, disconnect, pause/resume polling, and request sync now.
- Show fixed folder path, last sync, ingested count, duplicate count, and latest error.
- Keep tokens and OAuth secrets out of browser-visible data.
- Keep guest users from connecting Dropbox.

Success means:
- Signed-in dashboard users see a compact Dropbox status/control area.
- Guest users see no connect flow and no token-sensitive controls.
- Connect Dropbox starts the OAuth flow through `POST /api/dropbox/oauth/start`.
- Sync now calls `POST /api/dropbox/connection/sync-now` and refreshes status.
- Pause/resume and disconnect update the status without a page reload.

## Implementation instructions
1. Add Dropbox UI to `dashboard/static/index.html` near account/upload controls or another compact account-management area.
2. Keep the UI work-focused and consistent with current dashboard styling.
3. Add client logic in `dashboard/static/app.js`.
4. Reuse existing helpers where possible:
   - `apiGet`
   - `apiPostJson`
   - `setAccount`
   - account signed-in detection
5. On dashboard load for signed-in accounts, call `GET /api/dropbox/connection`.
6. Expected connection response shape should include:
   - `connected`
   - `status`
   - `root_path`
   - `last_sync_at`
   - `last_success_at`
   - `ingested_count`
   - `duplicate_count`
   - `latest_error`
   - `enabled`
7. Add buttons and states:
   - Connect Dropbox
   - Disconnect
   - Pause or Resume
   - Sync now
8. Disable controls while requests are in flight.
9. Show a clear disabled state when Dropbox support is not enabled server-side.
10. Do not display OAuth codes, access tokens, refresh tokens, encryption keys, or raw Dropbox errors that include secrets.
11. After OAuth callback redirect returns to the dashboard, refresh the connection status.
12. If the OAuth/storage task chooses a separate `dropbox.html` landing page, keep the dashboard entry point and status still present.
13. Add accessible labels and predictable focus behavior for buttons.
14. Add or update tests where the repo already has JS/unit coverage; otherwise document manual UI smoke checks in the final implementation notes.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Review current dashboard and auth flow:
  - `ctx_read dashboard/static/index.html`
  - `ctx_read dashboard/static/app.js`
  - `ctx_read dashboard/static/auth.html`
  - `ctx_read dashboard/static/auth.js`
  - `ctx_read src/api/dropbox.rs`
  - `ctx_read src/api/routes.rs`
- Format and validate:
  - `cargo test`
  - browser smoke against local Scargo when practical

## Relevant files, data, and context
- `dashboard/static/app.js` already centralizes API helpers and account-state rendering.
- `dashboard/static/index.html` contains dashboard controls and account/token UI.
- Dedicated auth lives in `dashboard/static/auth.html` and `dashboard/static/auth.js`; do not move login/register back into the dashboard.
- Dropbox tokens are server-only and must never be returned to client JS.
- The fixed Dropbox folder path is `/OBD Fusion/CsvLogs`.
- Vehicle data should appear through existing dashboard/vehicle APIs after worker ingest, not through Dropbox-specific telemetry reads.

## Acceptance checks and tests
- `cargo test`
- Signed-in user can start Dropbox OAuth from the dashboard.
- Guest user cannot start Dropbox OAuth from the dashboard.
- Connected state shows `/OBD Fusion/CsvLogs`, status, last sync, ingested count, duplicate count, and latest error.
- Pause/resume toggles server state and refreshes the view.
- Sync now triggers a worker pass and updates visible status.
- Disconnect removes the connection and returns the UI to connect state.
- No browser response or DOM text contains Dropbox token material.
- Dashboard still redirects unauthenticated users to `/auth.html`.

## Suggested branch name
- `feature/dropbox-dashboard-ui`
