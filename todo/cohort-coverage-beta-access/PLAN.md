# Cohort Coverage Beta Access

## Goal and success criteria

Define how Scargo measures useful contribution during hosted beta.

Success means the app can identify which uploads improve cohort coverage and
which early users should receive premium or contributor access.

## Implementation instructions

1. Define a simple contribution score from existing data: cohort gap, metric
   coverage, recency, repeated uploads, timestamp quality, and rarity.
2. Keep the score internal; do not expose leaderboards or driver scoring.
3. Use year/make/model/engine_family availability and metric-policy eligibility
   to decide cohort usefulness.
4. Prefer a report or admin/operator query before adding user-facing UI.
5. Keep exact VIN and owner identity private.
6. Update monetization and roadmap docs if the scoring model changes launch
   order or contributor benefits.

## Tools and commands to use

- `cargo test`
- `python3 scripts/rollup-retention-report.py` for live-data coverage checks
- `git diff --check`

## Relevant files, data, and context

- `docs/monetization-strategy.md`
- `docs/privacy-model.md`
- `docs/metric-policy.md`
- `scripts/rollup-retention-report.py`
- `src/api/cohort.rs`
- `src/api/vehicles.rs`

## Acceptance checks and tests

- Score never requires exposing raw peer telemetry.
- Missing vehicle metadata reduces cohort usefulness instead of blocking owner
  visibility.
- Contributor access rules are documented before any billing or entitlement code.
- Any generated analysis output remains ignored.

## Suggested branch name

- `feature/cohort-coverage-beta-access`
