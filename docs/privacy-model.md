# Scargo Privacy And Scaling Model

Scargo stores high-volume OBD2 telemetry so users can inspect their own vehicles
and compare sensor behavior against similar vehicles. The comparison value comes
from shared fleet data, but the product boundary is that raw vehicle identity,
ownership, and user-specific data must not leak through comparison features.

## Data Classes

| Class | Examples | API exposure |
|-------|----------|--------------|
| Account identity | username, password hash, session hash | Internal only |
| Vehicle identity | VIN, stable vehicle UUID | VIN internal only; UUID exposed only to owners |
| Ownership | account-to-upload access rows, vehicle profile sharing preferences | Internal only |
| Dropbox ingest state | encrypted refresh token, selected root path, cursor, per-file ledger | Internal only |
| Raw telemetry | timestamped channel values | Owner-scoped only |
| Derived telemetry | normalized channels, calculated metrics | Owner-scoped until policy-allowed aggregation |
| Sensitive telemetry | GPS/location and phone sensor channels | Owner-scoped raw only |
| Comparison cohorts | year/make/model/engine-family/channel aggregates | Public only after aggregation thresholds |

## Current Access Model

The browser lands on a dedicated `/auth.html` page, then uses username/password
login with an HttpOnly session cookie before entering the dashboard. Dashboard
CSV upload and Dropbox OAuth management use that same session. Dev/test mode
still exposes the deterministic `guest`/`local-dev` account, but dashboard access now
requires an explicit browser-side `Continue as guest` choice stored only in
session storage; production disables that fallback unless explicitly
configured. Deprecated `X-Scargo-User-Key` remains only as a dev/test
compatibility path.

On CSV ingest, Scargo:

1. Computes the stable vehicle id from VIN.
2. Upserts the vehicle without exposing VIN in list responses.
3. Resolves the account from a session or dev/test guest fallback.
4. Creates or reuses one `ingest_upload` row for that vehicle and content hash.
5. Links the upload to the account through `account_vehicle_upload` with private access enabled.
6. Stores raw readings and daily rollups keyed by `upload_id`, vehicle id, channel, and time.

Dropbox ingest uses the same account-scoped CSV helper. Managing Dropbox on
`/dropbox.html` requires an HttpOnly dashboard session for a signed-in
non-guest user; guests cannot start OAuth, inspect, pause, sync, or delete
connections. The API returns status, selected root path, and sync counts, but
not the stored refresh token. Deleting a connection
removes the encrypted token, cursor, and file ledger while leaving already
ingested telemetry and public approval state intact.

Read APIs for vehicles, latest readings, dashboard series, trends, and summaries
are scoped to the request account through uploads whose
`account_vehicle_upload.private_access` flag is still true. A request without
`vehicle_id` means "all uploads currently visible to this account", not all
vehicles in the database.

## Comparison Rule

Comparison endpoints must not return raw rows from vehicles the requester does
not own. They should return cohort aggregates only.

Minimum rules for future comparison endpoints:

- Group by non-identifying attributes such as normalized year, make, model,
  engine family, channel, and calendar-aligned time bucket.
- Enforce a minimum cohort size before returning data. Start with at least 5
  distinct vehicles and revisit before production.
- Return aggregates such as avg, min, max, percentiles, count, and standard
  deviation; do not return peer vehicle ids, VINs, owner ids, upload filenames,
  exact trip boundaries, or exact timestamps from other users.
- Bucket or blur time where calendar timing is not needed for the comparison.
- Treat location-derived and phone-sensor fields as sensitive by default. They
  can be ingested as owner-scoped raw telemetry, but must not be written to
  public rollups or comparison cohorts.
- Use the metric policy allowlist in `docs/metric-policy.md` before a channel
  can enter `vehicle_metric_day` or `/api/analysis/cohort/{channel}`.

## Vehicle Access Changes

Vehicles can change owners, but Scargo treats uploaded telemetry as belonging to
the uploader rather than permanently to the vehicle identity. Private access is
tracked per account and per upload. Dropping a vehicle from an account flips the
linked uploads to `private_access=false` and removes that account's vehicle
profile preference, so future raw reads disappear for that account while the
same stable vehicle id can later be linked to another account's uploads.

Shared public data is intentionally stickier than private access:

- Exact-VIN public stats require both approval on `ingest_upload.approved_exact_vin_at`
  and at least one linked account row with `exact_vin_share_enabled=true`.
- Year/make/model/engine-family cohort contribution is not optional for accepted
  uploads, but public cohort reads still require approval on
  `ingest_upload.approved_cohort_at` plus metric-policy eligibility.
- In dev/test mode only, signed-in non-guest dashboard sessions can manually stamp
  missing `approved_exact_vin_at` or `approved_cohort_at` values for uploads
  that remain linked to that same account with `private_access=true`. This does
  not bypass metric-policy restrictions, and production rejects that path.
- Dropping a vehicle from an account does not retract already approved public
  exact-VIN or cohort aggregates.

## Cost Controls

The app should stay cheap while usage is small and scale gradually as telemetry
volume grows.

- Keep the monolith and TimescaleDB while the workload is small.
- Use Timescale compression and retention policies once raw data volume grows.
- Add rollups for common dashboard windows before adding new infrastructure.
- Keep raw telemetry compressed for recent detail only; retain durable daily
  rollups for long-term owner views and public cohorts, limited to metric-policy
  allowlisted vehicle channels.
- Keep VIN inference conservative. Dropbox sync may reuse a unique exact
  VIN-pattern match from known metadata for exact 17-character VIN folders, then
  fetch from NHTSA vPIC into `vin_decode_cache` when no unique match exists. It
  does not guess metadata for non-VIN vehicle keys.
- Move heavy ingest, simulation, and cohort calculations to async jobs only when
  request latency or database load requires it.
- Use discovered correlations to reduce future client uploads to the smallest
  useful sensor set, while preserving enough raw data early to validate those
  derived metrics.

## Implementation Status

Implemented:

- `account` table with username, display name, password hash, and guest flag.
- `account_session` stores hashed dashboard session tokens with expiry.
- `ingest_upload` stores vehicle-level duplicate detection plus approval state
  for public exact-VIN and cohort sharing.
- `account_vehicle_profile` stores the per-account default exact-VIN sharing
  preference for a vehicle.
- `account_vehicle_upload` stores the per-account link to uploads, including
  private-access state and exact-VIN sharing state.
- `dropbox_connection` stores one encrypted Dropbox refresh token plus cursor
  state per account.
- `dropbox_oauth_state` stores short-lived hashed OAuth state during the
  browser redirect flow.
- `dropbox_ingest_file` stores per-connection path/revision sync status.
- `vin_decode_cache` stores cached exact-VIN NHTSA vPIC results and retry state.
- CSV ingest links uploads to the request account instead of asserting durable
  vehicle ownership.
- Vehicle list, latest readings, dashboard series, trends, and summaries are
  account-scoped.
- Upload packet hashes are tracked in `ingest_upload` with a uniqueness
  constraint per vehicle and content hash so duplicate CSV/data packet uploads
  are skipped in the API and database, not only by folder tooling.
- Core ingest takes a database advisory lock per vehicle while writing readings,
  so concurrent uploads for the same vehicle serialize regardless of whether
  they come from the dashboard, Dropbox worker, or a future mobile client.
- `/api/vehicles/{vehicle_id}/exact-vin-sharing` lets a signed-in owner toggle
  exact-VIN public sharing for all linked uploads on that vehicle.
- `GET /api/auth/me` reports whether the current session may use manual public
  approval controls.
- `GET /api/vehicles` reports pending exact-VIN and cohort approval counts for
  the requesting account's still-private uploads.
- `/api/vehicles/{vehicle_id}/approve-exact-vin-sharing` and
  `/api/vehicles/{vehicle_id}/approve-cohort-sharing` are dev/test-only manual
  approval routes scoped to the current account's linked private uploads.
- `DELETE /api/vehicles/{vehicle_id}` removes private access for that account
  without deleting already approved public aggregates.
- `/api/public/vehicle/{vin}` exposes approval-gated exact-VIN public
  aggregates sourced from shared daily rollups only.
- `/api/analysis/cohort/{channel}` returns aggregate-only comparison buckets for
  a year/make/model/engine-family cohort, sourced from `vehicle_metric_day`,
  with a minimum of 5 distinct vehicles per bucket.
- `src/ingest/canonical.rs` classifies metrics by category, sensitivity, daily
  rollup eligibility, and public cohort eligibility. GPS, phone sensor, trip
  behavior, adapter, system, and unknown numeric keys stay out of
  `vehicle_metric_day` and public cohorts.
- Vehicles missing `model` or `engine_family` remain owner-visible through
  account-scoped endpoints but are excluded from public cohorts.
- `/api/dropbox/*` lets signed-in non-guest users authorize, delete,
  pause/resume, and sync one Dropbox connection.
- Public cohort metadata is enriched offline from ignored local NHTSA vPIC
  cache rows, with conservative future-VIN inference only when VIN positions
  1-8 plus model year map to one unique metadata tuple.
- `/api/analysis/dashboard` returns account-scoped raw or summary series for
  dashboard charts. Summary reads use `vehicle_metric_day` for `1d`, `1w`, and
  `1mon` buckets.
- VIN is not returned by `/api/vehicles`.

Not implemented yet:
- Explicit vehicle transfer endpoint.
- Fully automated retention enforcement beyond the documented target of 180 days
  of compressed raw rows plus indefinite daily rollups.
- Mobile preprocessing upload format.
