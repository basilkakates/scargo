# Sharing Approval API

## Goal and success criteria

- Expose whether the current session can use manual sharing approval controls.
- Expose pending exact-VIN and cohort approval counts per owned vehicle.
- Add dev/test-only endpoints to approve pending public stats for the current
  account's linked private uploads.
- Reuse existing `ingest_upload` approval timestamps.
- Preserve existing privacy boundaries.

## Implementation instructions

1. Extend auth bootstrap data:
   - Update `src/api/auth.rs` so `GET /api/auth/me` returns:
     - `capabilities.approve_pending_public_stats: bool`
   - Set the flag `true` only for signed-in non-guest accounts in dev/test mode.
   - Set the flag `false` for guests.
   - Set the flag `false` in production mode.
2. Extend vehicle list data:
   - Update `src/api/vehicles.rs` `GET /api/vehicles` payload with:
     - `exact_vin_pending_approval_count`
     - `cohort_pending_approval_count`
   - Count only uploads linked through
     `account_vehicle_upload.private_access=true` for the requesting account.
   - Count exact-VIN pending uploads where
     `exact_vin_share_enabled=true` and `approved_exact_vin_at IS NULL`.
   - Count cohort pending uploads where `approved_cohort_at IS NULL`.
   - Keep existing `public` versus `pending` pill behavior. Do not add a
     `partial` status.
3. Add backend approval endpoints:
   - Add `POST /api/vehicles/{vehicle_id}/approve-exact-vin-sharing`.
   - Add `POST /api/vehicles/{vehicle_id}/approve-cohort-sharing`.
   - Wire both endpoints in `src/api/routes.rs`.
   - Reject guest accounts.
   - Reject approval attempts in production mode.
   - Require that the requesting account can currently access the vehicle.
   - Update only uploads linked to the current account with
     `private_access=true` and missing approval timestamps.
   - Make both endpoints idempotent.
4. Return this response shape from both endpoints:
   - `vehicle_id`
   - `approval`
   - `approved_upload_count`
   - `already_approved_upload_count`
5. Update docs:
   - Update `README.md` API docs with the auth capability field, vehicle pending
     counts, and approval endpoints.
   - Update `docs/privacy-model.md` with dev/test-only manual approval behavior
     and account-scoped approval stamping.
   - Update `AGENTS.md` endpoint and behavior notes.

## Tools and commands to use

- Inspect status before edits:
  - `git status --short --branch`
- Review backend code and docs with lean-ctx:
  - `ctx_read src/api/auth.rs`
  - `ctx_read src/api/vehicles.rs`
  - `ctx_read src/api/routes.rs`
  - `ctx_read docs/privacy-model.md`
  - `ctx_read README.md`
  - `ctx_read AGENTS.md`
- Search approval references:
  - `ctx_search "approved_exact_vin_at|approved_cohort_at|private_access" src docs README.md AGENTS.md`
- Validate after edits:
  - `cargo fmt`
  - `cargo test`
  - `cargo test --test smoke_stack -- --ignored --nocapture` if a local
    TimescaleDB is available.

## Relevant files, data, and context

- `src/api/auth.rs` owns `GET /api/auth/me`.
- `src/api/vehicles.rs` owns vehicle listing, exact-VIN sharing toggles, and
  vehicle access checks.
- `src/api/routes.rs` wires API routes.
- `src/db/migrate.rs` already defines `ingest_upload.approved_exact_vin_at` and
  `ingest_upload.approved_cohort_at`.
- Public cohorts already exclude vehicles missing `model` or `engine_family`.
- Approval must not imply that metric-policy restrictions are bypassed.
- Dev/test guest fallback exists, but guests must not approve public stats.

## Acceptance checks and tests

- `GET /api/auth/me` returns
  `capabilities.approve_pending_public_stats=true` only for signed-in non-guest
  dev/test sessions.
- `GET /api/auth/me` returns that capability `false` for guests and production.
- `GET /api/vehicles` includes correct pending approval counts.
- Exact-VIN pending counts ignore uploads with disabled exact-VIN sharing.
- Approval endpoints update only the current account's linked private uploads.
- Approval endpoints are idempotent and return stable repeat-call counts.
- Guest users cannot approve sharing stats.
- Production mode rejects manual approval actions.
- Existing exact-VIN sharing toggle behavior still works.
- `cargo test` passes.

## Suggested branch name

- `feature/sharing-approval-api`
