# Global Metric Registry + Strict Value Types

## Goal and success criteria

- Replace per-vehicle metric definitions with one global `obd2_metric` row per
  `key`.
- Enforce one strict metric value type per key across ingest and storage.
- Normalize bare acceleration units `m/s` and `ft/s` into canonical `m/s²`.
- Treat empty-database bootstrap as the supported path; do not keep legacy
  remap helpers in the repo.

Success means:

- `obd2_metric` stores `key`, `label`, `unit`, and `value_kind` globally.
- `POST /api/ingest/csv` rejects numeric/text conflicts for an existing key.
- `/api/channels`, dashboard queries, pair queries, and telemetry analysis stop
  depending on metric `vehicle_id`.
- Fresh databases get the final global metric schema directly from bootstrap.

## Implementation instructions

1. Update empty-db bootstrap in `src/db/migrate.rs`:
   - drop metric `vehicle_id`
   - add `value_kind`
   - add row-shape guard on `obd2_metric_reading`
2. Update ingest canonicalization:
   - treat `m/s` and `ft/s` as acceleration aliases
   - keep canonical acceleration storage unit `m/s²`
3. Update ingest write path:
   - validate one value kind per key within an upload
   - upsert metric ids by key only
   - reject key reuse across numeric/text kinds with `BadRequest`
4. Update read paths and analysis script to join metrics by `metric_id` only.
5. Update `README.md` and `AGENTS.md`.
6. After verification, archive:
   - `todo/daily-rollups-public-engine-cohorts/`
   - `todo/telemetry-analysis-v1/`

## Tools and commands to use

- `cargo fmt`
- `cargo test`
- `python3 scripts/analyze-telemetry.py --self-test`
- `docker compose up -d scargo_db`
- `cargo test --test smoke_stack -- --ignored --nocapture`

## Relevant files, data, and context

- `src/db/migrate.rs`
- `src/ingest/model.rs`
- `src/ingest/canonical.rs`
- `src/ingest/csv.rs`
- `src/api/channels.rs`
- `src/api/dashboard.rs`
- `src/api/pairs.rs`
- `scripts/analyze-telemetry.py`
- `README.md`
- `AGENTS.md`

## Acceptance checks and tests

- `cargo test`
- ingest tests cover acceleration aliases and numeric/text conflicts
- `python3 scripts/analyze-telemetry.py --self-test`
- real-DB smoke flow passes after running the backfill script against Compose DB

## Suggested branch name

- `feature/global-metric-registry`
