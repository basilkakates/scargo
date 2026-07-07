# Minimal Telemetry Policy

## Goal and success criteria

- Classify metric keys by source category, sensitivity, rollup eligibility, and
  public-cohort eligibility.
- Keep broad owner-scoped raw ingest, including GPS and phone sensor columns.
- Prevent GPS, phone sensor, trip behavior, adapter, system, and unknown numeric
  metrics from entering durable daily rollups or public anonymous cohorts.
- Document what Scargo should measure directly versus derive later.

Success means:

- `/api/channels` exposes additive policy metadata for each channel.
- Live ingest and bulk-load finalization use the same allowlist for
  `vehicle_metric_day`.
- `/api/analysis/cohort/{channel}` rejects private or unknown channels.
- Docs explain that SAE/raw vehicle signals are the measurement base and derived
  fuel, CO2, trip, power, torque, acceleration, and cost values should not become
  permanent required uploads without evidence.

## Implementation instructions

1. Add metric policy helpers to `src/ingest/canonical.rs`.
2. Use policy rollup eligibility in `src/ingest/csv.rs`.
3. Use the policy allowlist in `src/db/migrate.rs` bulk rollup rebuild.
4. Add policy fields to `/api/channels` in `src/api/channels.rs`.
5. Reject non-public cohort channels in `src/api/cohort.rs`.
6. Update `README.md`, `AGENTS.md`, `docs/privacy-model.md`, and
   `docs/metric-policy.md`.

## Tools and commands to use

- `cargo fmt`
- `cargo test`
- Optional real database check: `cargo test --test smoke_stack -- --ignored --nocapture`

## Relevant files, data, and context

- `src/ingest/canonical.rs`
- `src/ingest/csv.rs`
- `src/db/migrate.rs`
- `src/api/channels.rs`
- `src/api/cohort.rs`
- `docs/privacy-model.md`

## Acceptance checks and tests

- Policy unit tests cover public vehicle, GPS, phone sensor, trip cost, fuel
  remaining, adapter, system, duplicate suffix, and unknown future keys.
- Ingest unit tests verify private metrics are excluded from daily rollups.
- Cohort tests verify `vehicle_speed` is accepted and `latitude`/`accel_x` are
  rejected.
- `cargo test` passes.

## Suggested branch name

- `feature/minimal-telemetry-policy`
