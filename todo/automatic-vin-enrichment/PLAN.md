# Automatic VIN Enrichment

## Goal and success criteria

- Enrich valid 17-character VIN uploads during normal manual and Dropbox ingest.
- Use existing metadata and exact-pattern inference before calling NHTSA vPIC.
- Cache vPIC results and avoid hammering vPIC with per-VIN retry windows plus an app-wide throttle.

Success means uploads continue when vPIC is unavailable, complete vehicles keep existing metadata, inferred VINs do not call vPIC, and unresolved VINs retry only after `next_retry_after`.

## Implementation instructions

1. Add `vin_decode_cache` and `external_lookup_throttle` to schema bootstrap.
2. Keep `ingest_csv_for_account` as the shared manual/Dropbox ingest entry point.
3. Enrich in this order: existing complete metadata, local VIN year/make decode, unique `VIN[0..8] + year` inference, cached vPIC, throttled vPIC.
4. Cache `ok`, `incomplete`, and `error` vPIC results with attempt count and retry timestamps.
5. Do not fail uploads because vPIC is unavailable, throttled, or incomplete.
6. Update README, AGENTS, and privacy docs.

## Tools and commands to use

- `cargo fmt --all -- --check`
- `cargo test`
- `git diff --check`

## Relevant files, data, and context

- `src/api/ingest.rs`
- `src/ingest/vin.rs`
- `src/db/migrate.rs`
- `README.md`
- `AGENTS.md`
- `docs/privacy-model.md`

## Acceptance checks and tests

- VIN validation rejects malformed VINs and `I/O/Q`.
- Engine family uses one-decimal displacement and does not infer cylinder layout when vPIC omits it.
- Failed lookups wait at least 24 hours before retry.
- Incomplete lookups wait at least 7 days before retry.
- vPIC is globally throttled to at most one request per minute.

## Suggested branch name

- `feature/automatic-vin-enrichment`
