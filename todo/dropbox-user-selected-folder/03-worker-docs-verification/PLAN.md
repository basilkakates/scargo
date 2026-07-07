# Dropbox Worker Docs and Verification

## Goal and success criteria
- Align worker tests and docs with user-selected Dropbox roots.
- Remove fixed-root v1 language from durable docs.
- Keep the simple folder contract: selected root contains direct VIN or vehicle-key child folders.

Success means:
- Worker tests prove arbitrary selected roots map files correctly.
- README, AGENTS, and `docs/dropbox-ingest.md` describe user-selected folders.
- Docs still explain duplicate guarantees, privacy boundaries, and operational checks.
- Fixed `/OBD Fusion/CsvLogs` appears only as a default example, not as the only supported root.

## Implementation instructions
1. Start from `docs/dropbox-user-folder-docs`.
2. Inspect status, tests, and docs before editing:
   - `git status --short --branch`
   - `ctx_read src/dropbox_worker.rs`
   - `ctx_read README.md`
   - `ctx_read AGENTS.md`
   - `ctx_read docs/dropbox-ingest.md`
3. Update worker tests around path mapping:
   - `/Logs/DEMO-HONDA-ACCORD/a.csv` ingests as vehicle key `DEMO-HONDA-ACCORD`
   - `/Logs/a.csv` skips because no vehicle folder exists
   - `/Logs/DEMO-HONDA-ACCORD/nested/a.csv` skips unless implementation intentionally supports nesting
   - paths outside the selected root are ignored
4. Keep worker behavior based on `dropbox_connection.root_path`; do not add per-deployment global root config.
5. Update README Dropbox setup and API tables for folder selection and `POST /api/dropbox/connection/folder`.
6. Update AGENTS with current API, schema, dashboard, and worker behavior.
7. Update `docs/dropbox-ingest.md` with:
   - signed-in user chooses a Dropbox root folder
   - folder contract: `<root>/<VIN-or-vehicle-key>/*.csv`
   - root-level CSV files are skipped
   - Dropbox files are not moved, renamed, or archived
   - dedupe still uses Dropbox path/rev plus Scargo content hash
   - persistent DB storage is required for tokens, cursors, chosen paths, and file state
8. Keep docs free of real secrets, private VINs, and access tokens.

## Relevant files, data, and context
- `src/dropbox_worker.rs` already maps `connection.root_path` to VIN child folders.
- `dropbox_connection.root_path` stores the selected path.
- `dropbox_ingest_file` records path/rev/status for duplicate prevention.
- Account scoping remains through the signed-in Scargo account that owns the Dropbox connection.

## Acceptance checks and tests
- `cargo fmt --check`
- `cargo test`
- `git diff --check`
- Docs state that many users can each connect their own Dropbox account and chosen folder.
- Docs state that files ingest only into the connecting Scargo account.
- Docs do not describe `/OBD Fusion/CsvLogs` as the required global root.

## Suggested branch name
- `docs/dropbox-user-folder-docs`
