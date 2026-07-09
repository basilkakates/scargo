# Owner Health Report V1

## Goal and success criteria

Create the first owner-facing vehicle health report from existing telemetry.

Success means a signed-in owner can view a concise report for one visible
vehicle using account-scoped data already stored by Scargo.

## Implementation instructions

1. Build from existing owner-scoped APIs and tables first: vehicles, latest,
   dashboard summary, pairs, and `vehicle_metric_day`.
2. Include report sections for data coverage, recent changes, efficiency,
   temperature/voltage ranges, and notable metric drift when data exists.
3. Show insufficient-data states instead of inventing confidence.
4. Keep VIN private; use the owner-visible vehicle UUID and metadata already
   exposed by `/api/vehicles`.
5. Add a simple route or dashboard view only if current pages cannot present the
   report cleanly.
6. Update docs for any new endpoint or dashboard view.

## Tools and commands to use

- `cargo test`
- `scripts/smoke-docker.sh` when DB verification is needed
- `git diff --check`

## Relevant files, data, and context

- `src/api/dashboard.rs`
- `src/api/latest.rs`
- `src/api/pairs.rs`
- `src/api/summary.rs`
- `src/api/vehicles.rs`
- `dashboard/static/app.js`
- `dashboard/static/vehicles.js`
- `docs/privacy-model.md`
- `docs/metric-policy.md`

## Acceptance checks and tests

- Report reads are scoped to uploads still visible to the current account.
- Missing metrics produce clear unavailable states.
- Public cohort data is aggregate-only and respects metric policy if used.
- Docs mention any new route, view, or response shape.

## Suggested branch name

- `feature/owner-health-report-v1`
