# Daily/Weekly/Monthly Rollups + Engine-Specific Public Cohorts

## Goal and success criteria

- Shift long-term owner/public reads off raw telemetry rows.
- Keep raw telemetry for recent detail and exact-sample analysis only.
- Add durable daily per-vehicle numeric rollups for owner summaries and public cohorts.
- Exclude vehicles from public cohorts until both `model` and `engine_family` are resolved.

Success means:

- `/api/analysis/dashboard` summary reads use `vehicle_metric_day` for `1d`, `1w`, `1mon`
- `/api/analysis/summary/{channel}` uses `vehicle_metric_day`
- `/api/analysis/cohort/{channel}` requires `engine_family` and stays aggregate-only
- `/api/vehicles` returns `model` and `engine_family`
- offline scripts exist for rollup backfill, vehicle metadata backfill, and retention reporting

## Implementation instructions

1. Add `vehicle.engine_family` and `vehicle_metric_day` to schema bootstrap.
2. Preserve existing vehicle metadata on ingest when VIN decode returns blanks.
3. Maintain `vehicle_metric_day` during numeric ingest writes.
4. Parse summary buckets as `1d`, `1w`, `1mon`.
5. Route owner summary/dashboard and public cohort reads through `vehicle_metric_day`.
6. Page initial dashboard chart fetches to 12 metrics, with on-demand expansion.
7. Add offline scripts:
   - `scripts/backfill-daily-rollups.py`
   - `scripts/backfill-vehicle-metadata.py`
   - `scripts/rollup-retention-report.py`
8. Update docs in `README.md`, `AGENTS.md`, and `docs/privacy-model.md`.

## Tools and commands to use

- `cargo fmt`
- `cargo test`
- `python3 scripts/backfill-vehicle-metadata.py --self-test`
- `python3 scripts/analyze-telemetry.py --self-test`
- `python3 scripts/backfill-daily-rollups.py --truncate-first`
- `python3 scripts/rollup-retention-report.py`

## Relevant files, data, and context

- `src/db/migrate.rs`
- `src/ingest/csv.rs`
- `src/api/dashboard.rs`
- `src/api/summary.rs`
- `src/api/cohort.rs`
- `src/api/vehicles.rs`
- `dashboard/static/index.html`
- `dashboard/static/app.js`
- `scripts/analyze-telemetry.py`
- `docs/privacy-model.md`

## Acceptance checks and tests

- `cargo test`
- ignored smoke test against a running database still passes
- summary endpoints return day/week/month buckets after raw retention backfill
- public cohorts split `year`/`make`/`model` by `engine_family`
- vehicles missing `model` or `engine_family` do not appear in public cohorts

## Suggested branch name

- `feature/daily-rollups-public-engine-cohorts`
