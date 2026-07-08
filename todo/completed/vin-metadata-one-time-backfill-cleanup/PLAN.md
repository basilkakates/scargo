# VIN Metadata One-Time Backfill Cleanup

## Goal and success criteria

- Use the old VIN fetch/backfill scripts once for the two existing unresolved DB VINs.
- Remove those scripts after automatic ingest-time enrichment exists.
- Leave docs pointing at automatic cached enrichment instead of offline CSV backfill.

Success means the unresolved-vehicle query returns no rows, the old scripts are gone, and docs no longer tell agents or users to run them.

## Implementation instructions

1. Query unresolved rows:
   `SELECT vin, year, make, model, engine_family FROM vehicle WHERE model = '' OR engine_family = '' ORDER BY vin;`
2. Continue only if exactly two rows are unresolved.
3. Run `scripts/fetch-vin-decodes.py --missing-only --cache /tmp/scargo-vin-decodes.csv`.
4. Run `scripts/backfill-vehicle-metadata.py --decode-csv /tmp/scargo-vin-decodes.csv --dry-run`.
5. Run `scripts/backfill-vehicle-metadata.py --decode-csv /tmp/scargo-vin-decodes.csv`.
6. Verify the unresolved query returns no rows.
7. Delete `scripts/fetch-vin-decodes.py` and `scripts/backfill-vehicle-metadata.py`.
8. Remove offline-script workflow docs.

## Tools and commands to use

- `psql -A -t -q`
- `python3 scripts/fetch-vin-decodes.py`
- `python3 scripts/backfill-vehicle-metadata.py`
- `cargo fmt --all -- --check`
- `cargo test`
- `git diff --check`

## Relevant files, data, and context

- `scripts/fetch-vin-decodes.py`
- `scripts/backfill-vehicle-metadata.py`
- `README.md`
- `AGENTS.md`
- `docs/privacy-model.md`

## Acceptance checks and tests

- Initial unresolved query shows exactly the two expected DB VIN rows.
- Dry run reports two prepared direct metadata updates.
- Applied backfill updates two rows.
- Final unresolved query returns no rows.
- Deleted scripts are not referenced by docs.

## Suggested branch name

- `chore/remove-vin-backfill-scripts`

## Completion note

- 2026-07-07: The unresolved-vehicle query returned no rows, so the planned
  one-time script run was already unnecessary.
- Removed `scripts/fetch-vin-decodes.py` and
  `scripts/backfill-vehicle-metadata.py`.
- Docs already pointed at automatic cached ingest-time enrichment.
