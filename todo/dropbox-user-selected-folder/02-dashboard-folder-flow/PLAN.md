# Dropbox Dashboard Folder Flow

## Goal and success criteria
- Let a logged-in user type the Dropbox folder path that Scargo should ingest from.
- Make the current stored folder visible and editable from the dashboard.
- Keep the UI compact and consistent with the existing Dropbox panel.

Success means:
- Signed-in users can enter a Dropbox root folder before starting OAuth.
- Connected users can update the root folder without reconnecting Dropbox.
- The panel displays the stored root path from `/api/dropbox/connection`.
- Guest users still cannot see or use Dropbox management controls.
- No Dropbox tokens, OAuth codes, or encryption details are exposed in the DOM.

## Implementation instructions
1. Start from `feature/dropbox-user-folder-ui`.
2. Inspect status and current dashboard code before editing:
   - `git status --short --branch`
   - `ctx_read dashboard/static/index.html`
   - `ctx_read dashboard/static/app.js`
   - `ctx_read src/api/dropbox.rs`
3. Add a folder path input to the existing Dropbox panel.
4. Default the input to `/OBD Fusion/CsvLogs` only when the server has no connection payload yet.
5. On `Connect Dropbox`, send `{ redirect_path: "/", root_path: input.value }` to `POST /api/dropbox/oauth/start`.
6. Add a compact `Save folder` action that calls `POST /api/dropbox/connection/folder` with the input value.
7. Disable connect/save/sync/pause/disconnect while a Dropbox request is in flight.
8. After connect callback or folder save, refresh Dropbox status and account data.
9. Display validation or server errors in the existing Dropbox status area.
10. Keep login/register on `auth.html`; do not move auth controls back into the dashboard.

## Relevant files, data, and context
- `dashboard/static/index.html` contains the Dropbox panel markup and CSS.
- `dashboard/static/app.js` owns `renderDropbox`, `startDropboxOAuth`, and Dropbox status actions.
- Reuse existing helpers such as `apiGet`, `apiPostJson`, `apiDelete`, and account signed-in detection.
- The folder layout remains `<selected-root>/<VIN-or-vehicle-key>/*.csv`.

## Acceptance checks and tests
- `cargo fmt --check`
- `cargo test`
- Manual browser smoke when practical:
  - signed-in user sees folder input and connect button
  - guest user sees no Dropbox controls
  - connect sends the typed root path
  - connected state shows stored root path
  - save folder updates status without page reload
  - sync now still triggers a worker pass
- Inspect browser responses/DOM text to confirm no token material is visible.

## Suggested branch name
- `feature/dropbox-user-folder-ui`
