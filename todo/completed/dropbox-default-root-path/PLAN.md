# Dropbox Default Root Path

## Goal and success criteria
- Correct Scargo's default Dropbox root path from `/OBD Fusion/CsvLogs` to `/Apps/OBD Fusion/CsvLogs`.
- Keep the fix narrow so new Dropbox setups point at the working OBD Fusion app-folder path without adding broader sync recovery logic.

Success means:
- New Dropbox OAuth flows default to `/Apps/OBD Fusion/CsvLogs`.
- Disconnected Dropbox UI placeholders and saved-root fallbacks show `/Apps/OBD Fusion/CsvLogs`.
- Clean schema bootstrap uses `/Apps/OBD Fusion/CsvLogs` as the default `dropbox_connection.root_path`.
- Docs and tests no longer refer to `/OBD Fusion/CsvLogs` as the default root.
- Existing connections already saved with `/OBD Fusion/CsvLogs` are called out as a manual one-time fix, not auto-rewritten.

## Implementation instructions
1. Start from `fix/dropbox-default-root-path`.
2. Replace the baked-in default root path string with `/Apps/OBD Fusion/CsvLogs` in:
   - `src/config/settings.rs`
   - `src/api/dropbox.rs`
   - `src/db/migrate.rs`
   - `dashboard/static/dropbox.js`
   - `dashboard/static/dropbox.html`
3. Update any tests in `src/api/dropbox.rs` and `src/dropbox_worker.rs` that hard-code the old default path.
4. Update `README.md` and `AGENTS.md` so setup text, API/UI descriptions, and examples match the new default path.
5. Do not add alias handling, worker recovery, cursor reset logic, or database rewrite logic in this task.
6. Mention in docs or task notes that accounts already saved with `/OBD Fusion/CsvLogs` must save the new folder once or reconnect Dropbox.

## Tools and commands to use
- Inspect status before edits: `git status --short --branch`
- Use lean-ctx reads/searches for current config, API, dashboard, schema, and docs references.
- Validate with:
  - `cargo test`
  - `git diff --check`
  - `git status --short --branch`

## Relevant files, data, and context
- `src/api/dropbox.rs` owns Dropbox OAuth defaults, connection response fallbacks, and root-path normalization tests.
- `src/config/settings.rs` owns environment-backed Dropbox config defaults.
- `src/db/migrate.rs` is clean schema bootstrap only; update the default literal there without adding compatibility migrations.
- `dashboard/static/dropbox.js` and `dashboard/static/dropbox.html` own the default root shown to users on the Dropbox management page.
- `todo/shared-link-ingest/PLAN.md` stays active until that broader task is reviewed, approved, and merged.

## Acceptance checks and tests
- Unit tests accept and normalize `/Apps/OBD Fusion/CsvLogs/` to `/Apps/OBD Fusion/CsvLogs`.
- Disconnected/default connection responses return `/Apps/OBD Fusion/CsvLogs`.
- Dropbox path-mapping tests that rely on the default root use `/Apps/OBD Fusion/CsvLogs`.
- Docs describe `/Apps/OBD Fusion/CsvLogs` as the default monitored folder.
- No code path attempts to auto-rewrite existing saved rows that still use `/OBD Fusion/CsvLogs`.

## Suggested branch name
- `fix/dropbox-default-root-path`
