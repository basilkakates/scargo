# Dropbox Ingest Operability

## Goal and success criteria

Make Dropbox sync understandable and supportable for hosted beta.

Success means a user or operator can tell whether Dropbox is connected, paused,
syncing, current, skipped, or failing, with enough error detail to fix common
configuration and folder issues.

## Implementation instructions

1. Review existing connection status, file ledger, latest error, and sync-now
   behavior before adding new state.
2. Surface concise sync state on `/dropbox.html`: selected root, active/paused,
   last sync attempt, last success, file counts, skip counts, and latest error.
3. Preserve the existing path contract: direct `<vehicle-key>/<file>.csv`
   children under `/Apps/OBD Fusion/CsvLogs`.
4. Improve operator docs for callback URI, namespace-root behavior, and common
   Dropbox HTTP errors.
5. Do not persist CSV or ZIP artifacts outside PostgreSQL.

## Tools and commands to use

- `cargo test`
- targeted unit tests around Dropbox status/error formatting where practical
- `git diff --check`

## Relevant files, data, and context

- `src/api/dropbox.rs`
- `src/dropbox_worker.rs`
- `dashboard/static/dropbox.html`
- `dashboard/static/dropbox.js`
- `README.md`
- `AGENTS.md`
- `docs/roadmap.md`

## Acceptance checks and tests

- Signed-in non-guest users can understand current Dropbox state from the page.
- Dropbox errors keep HTTP status plus short upstream summary when available.
- Root-level CSVs and nested CSVs remain skipped according to the existing v1
  contract.
- No raw Dropbox file bytes are retained after ingest.

## Suggested branch name

- `feature/dropbox-ingest-operability`
