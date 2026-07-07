# Telemetry Analysis V1

## Goal

Implement the first Scargo pass for richer telemetry collection and analysis:
Summary-first dashboard, VIN metadata capture, metric-vs-metric graphing, clean
TimescaleDB schema bootstrap for an empty database, and a one-time relationship
analysis report over stored numeric data.

## Success Criteria

- Dashboard no longer exposes the Raw tab.
- Dashboard can graph one numeric metric against another as a scatter plot.
- VIN ingest stores decoded year and common WMI make when available.
- Reading storage remains TimescaleDB hypertable-first and no longer depends on
  a unique reading constraint.
- Duplicate payload upload prevention remains in `ingest_upload`.
- A one-time script can generate JSON and CSV relationship reports.

## Implementation Notes

- Use branch `feature/telemetry-analysis-v1`.
- Keep raw telemetry APIs account-scoped.
- Keep comparison/cohort APIs aggregate-only.
- Treat the current DB as empty; do not add legacy compatibility migrations.
- Do not add a persistent analysis scheduler or findings table in this pass.

## Checks

- `cargo test`
- Optional with Docker DB running: `scripts/smoke-docker.sh`
- Optional after reuploading data: `python3 scripts/analyze-telemetry.py`
