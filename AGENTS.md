# Scargo — Agent Context

> **Read this first.** Everything you need to resume work on scargo is here.
> No persistent memory assumed. No tribal knowledge required.

## What is scargo?

An **OBD2 telematics** application: consume CSV vehicle data, store it in a
time-series database (TimescaleDB on PostgreSQL), analyze trends, and present a
web dashboard.  Built for performance and simplicity.

- **Stack:** Rust 2021 + actix-web 4 + tokio-postgres 0.7 (deadpool-postgres 0.14 pooling) + TimescaleDB + Chart.js 4
- **Database:** TimescaleDB on PostgreSQL. The app requires the `timescaledb` extension.
- **Dashboard:** Single-page HTML+JS served by actix-web, Chart.js loaded from CDN.

## Project structure

```
scargo/
├── Cargo.toml          # Deps: actix-web, deadpool-postgres, tokio-postgres, csv, chrono, uuid, …
├── Dockerfile          # Multi-stage production image for the Scargo web app
├── AGENTS.md           # ← YOU ARE HERE
├── README.md           # Human-facing overview + setup
├── compose.yaml        # Local TimescaleDB service plus optional app profile
├── docs/
│   ├── dashboard-creative-direction.md # Dashboard visual system
│   ├── deployment-options.md # Production database/app-hosting options
│   ├── metric-policy.md # Metric categories, privacy, rollup/public policy
│   ├── monetization-strategy.md # Product and revenue direction
│   ├── privacy-model.md # Privacy, ownership, comparison, and cost scaling model
│   └── roadmap.md # Hosted-beta direction and active plan index
├── .github/
│   └── workflows/
│       └── ci.yml # Rust CI checks
├── dashboard/
│   └── static/
│       ├── auth.html  # Dedicated login/create-account page for dashboard entry
│       ├── auth.js    # Auth-page logic — /api/auth/* + explicit guest consent
│       ├── index.html  # Dashboard SPA (Chart.js from CDN)
│       ├── app.js      # Dashboard logic — fetches /api/analysis/dashboard
│       ├── dropbox.html # Dedicated Dropbox OAuth ingest management page
│       ├── dropbox.js   # Dropbox-page logic — /api/dropbox/*
│       ├── vehicles.html # Dedicated vehicle-management page
│       └── vehicles.js   # Vehicle-management page logic
├── src/
│   ├── main.rs         # Entry point: init tracing, config, pool, migrate, start server
│   ├── config/
│   │   ├── mod.rs      # Re-exports Settings, Error, Result
│   │   ├── error.rs    # Error enum (Internal, NotFound, BadRequest, Unauthorized, Database, CsvParse)
│   │   └── settings.rs # Settings struct (http host/port, database_url)
│   ├── db/
│   │   ├── mod.rs      # Database struct wrapping deadpool_postgres::Pool
│   │   └── migrate.rs  # Idempotent DDL: schema, hypertable, indexes, triggers
│   ├── ingest/
│   │   ├── mod.rs      # Re-exports csv::ingest_reader
│   │   ├── canonical.rs # Canonical metric metadata, unit conversion, display-unit options
│   │   ├── csv.rs      # CSV reader: parses live OBD export → raw metrics → INSERT
│   │   ├── model.rs    # RawMetricReading, ParseError, timestamp helpers
│   │   └── vin.rs      # Small local VIN year/common-WMI make decoder
│   └── api/
│       ├── mod.rs      # Declares submodules: auth, channels, ingest, latest, privacy, routes, summary, trends, vehicles
│       ├── auth.rs     # POST/GET /api/auth/* login and session routes
│       ├── routes.rs   # configure() — wires /api routes
│       ├── channels.rs # GET  /api/channels — dashboard channel registry
│       ├── cohort.rs   # GET  /api/analysis/cohort/{channel} — aggregate comparisons
│       ├── ingest.rs   # POST /api/ingest/csv?vin=VIN — upload CSV file
│       ├── dropbox.rs  # Dropbox OAuth APIs for connect/callback/manage routes
│       ├── dashboard.rs # GET  /api/analysis/dashboard — batched dashboard series
│       ├── trends.rs   # GET  /api/analysis/trends/{channel} — raw time series
│       ├── summary.rs  # GET  /api/analysis/summary/{channel} — time_bucket aggregates
│       ├── latest.rs   # GET  /api/analysis/latest/{vehicle_id} — recent readings
│       ├── pairs.rs    # GET  /api/analysis/pairs — numeric X/Y metric pairs
│       ├── privacy.rs  # Account/session/token resolution and ownership checks
│       └── vehicles.rs # GET  /api/vehicles — list account-owned vehicles
├── src/dropbox_worker.rs # Dropbox cursor poller + per-file download ingest worker
├── todo/
│   ├── deployment-readiness/ # Active hosted-deploy plan set
│   ├── docs-roadmap-refresh/ # Active docs organization plan
│   ├── hosted-beta-onboarding/ # Active early-user onboarding plan
│   ├── dropbox-ingest-operability/ # Active Dropbox beta operations plan
│   ├── owner-health-report-v1/ # Active owner report plan
│   ├── maintenance-inference-v1/ # Active maintenance inference plan
│   ├── cohort-coverage-beta-access/ # Active contribution scoring/access plan
│   └── completed/      # Completed plan folders after review and merge
├── scripts/
│   ├── analyze-telemetry.py # One-time trend/relationship report (daily by default, raw opt-in)
│   ├── rollup-retention-report.py # Raw vs daily footprint and cohort coverage report
│   ├── reset-dev-db.sh # Destructive local Compose DB reset without re-upload
│   └── smoke-docker.sh # Starts Compose DB and runs the smoke test
└── tests/
    └── smoke_stack.rs  # Ignored smoke test against a running Compose DB
```

## Build & Run

### Prerequisites
- Rust 1.80+ (current: 1.96)
- TimescaleDB 2.x on PostgreSQL 14+

### Setup database
```bash
docker compose up -d scargo_db

# Optional without Docker:
# createdb scargo
# psql scargo -c "CREATE EXTENSION IF NOT EXISTS timescaledb;"
```

### Configure
`.env` and `.env.*` are ignored and may contain secrets. Create or edit them
only for real overrides, such as `SCARGO_DATABASE_URL` for an external database
or `POSTGRES_PASSWORD` for local Docker Compose. Safe tracked examples may use
placeholders only; real secret values belong in ignored files such as `.env` or
`.env.smoke`.
`SCARGO_ENV` defaults to `dev`; dev mode derives a local database URL from
`POSTGRES_HOST`, `POSTGRES_PORT`, `POSTGRES_USER`, optional `POSTGRES_PASSWORD`,
and `POSTGRES_DB` when `SCARGO_DATABASE_URL` is unset. `SCARGO_ENV=production`
requires an explicit `SCARGO_DATABASE_URL`.
Local database connections use plain PostgreSQL on localhost or the Compose
network. Add TLS only when a production database requires it.
Dropbox ingest uses Full Dropbox OAuth because OBD Fusion writes to its own
Dropbox app folder. Configure `DROPBOX_APP_KEY`, `DROPBOX_APP_SECRET`,
`SCARGO_BASE_URL`, `SCARGO_TOKEN_ENCRYPTION_KEY`, and
`SCARGO_DROPBOX_ENABLED=true` to enable per-account server-side polling. Scargo
stores encrypted refresh tokens plus Dropbox cursors, downloads only new CSV
revisions from the selected folder, and does not retain CSV or ZIP artifacts
outside the database.
Set `SCARGO_DROPBOX_REDIRECT_URI` only when the exact registered Dropbox
callback URI differs from `SCARGO_BASE_URL + /api/dropbox/oauth/callback`.
`SCARGO_DROPBOX_POLL_SEC` defaults to 300 seconds.

### Build & run
```bash
cargo build --release
./target/release/scargo
```

### Container build & run
```bash
docker build -t scargo:local .
docker compose --profile app up --build scargo
```

The Compose `app` profile runs Scargo beside `scargo_db`, binds the container to
`SCARGO_HTTP_HOST=0.0.0.0`, and reaches the database through
`POSTGRES_HOST=scargo_db`. `docker compose up -d scargo_db` remains the default
DB-only local development workflow.

The web dashboard is at `http://localhost:8080/`.  The health check is at `http://localhost:8080/api/health`.

For end-to-end local verification, first run `docker compose up -d scargo_db`,
then run `cargo test --test smoke_stack -- --ignored --nocapture`. The ignored
smoke test assumes the Postgres service is already running, creates a disposable
`scargo_smoke_*` database through `SCARGO_SMOKE_ADMIN_DB` (default `postgres`),
starts Scargo on port `18080`, checks health and API paths, ingests a tiny CSV,
verifies stored dashboard data, then drops the temporary database. It reads
ignored `.env.smoke` credentials when present and removes
`SCARGO_DATABASE_URL` from child processes so it cannot target a live app DB by
accident. This is the primary check for "builds and runs against a real
database"; it does not depend on GitLab CI, GitHub Actions, shell scripts,
`curl`, or repository secrets.
Use `scripts/smoke-docker.sh` when Docker access is available; it loads ignored
env files, starts the Compose database with a local default `POSTGRES_PASSWORD`,
waits for Postgres readiness, and runs that smoke test.

## Database schema

Core tables:

| Object | Purpose |
|--------|---------|
| `vehicle` (table) | Vehicle registry: id (UUID PK), vin (unique), make, model, engine_family, year, created_at, updated_at |
| `account` (table) | User account registry: username, display name, password hash, guest flag |
| `account_session` (table) | Hashed dashboard session cookies with expiry |
| `ingest_upload` (table) | Vehicle+content hash de-duplication plus approval timestamps for public exact-VIN and cohort sharing |
| `account_vehicle_profile` (table) | Per-account default sharing preference for a vehicle's exact-VIN public stats |
| `account_vehicle_upload` (table) | Per-account link to uploads, including private-access state and exact-VIN sharing flag |
| `dropbox_connection` (table) | One encrypted Dropbox refresh token, root path, cursor, and sync state per account |
| `dropbox_oauth_state` (table) | Short-lived hashed Dropbox OAuth state rows during browser redirect flow |
| `dropbox_ingest_file` (table) | Per-connection Dropbox file ledger keyed by path and revision |
| `vin_decode_cache` (table) | Cached exact-VIN NHTSA vPIC metadata and retry state |
| `external_lookup_throttle` (table) | App-wide throttle state for official external lookup calls |
| `obd2_metric` (table) | Global metric registry: one row per key with label, unit, and strict `value_kind` |
| `obd2_metric_reading` (table) | Time-series raw metric values: upload_id, time, vehicle_id, metric_id, and exactly one payload column (`value` or `text_value`) |
| `vehicle_metric_day` (table) | Durable numeric daily rollup: bucket_day, upload_id, vehicle_id, metric_id, value_sum, min_value, max_value, reading_count |

`obd2_metric_reading` is created as a TimescaleDB hypertable partitioned by
`time`. Clean empty-database bootstrap also configures a 7-day compression
policy and an hourly continuous aggregate for numeric readings. Reading rows do
not have a per-sample uniqueness constraint; exact duplicate upload packets are
blocked through `ingest_upload`.

Indexes: `(vehicle_id, metric_id, time DESC)`, `(upload_id, metric_id, time DESC)`,
`obd2_metric(key)`, `obd2_metric(key) INCLUDE (id, label, unit, value_kind)`,
`account_vehicle_profile(vehicle_id, updated_at DESC)`,
`account_vehicle_upload(account_id, vehicle_id, private_access)`,
`account_vehicle_upload(upload_id) WHERE exact_vin_share_enabled`, and upload
time indexes.

## API endpoints

| Method | Path | Purpose |
|--------|------|---------|
| GET | `/api/health` | Health check → `{"status":"ok"}` |
| GET | `/api/channels` | Channel registry for the dashboard, including canonical/display-unit and metric-policy metadata |
| GET | `/api/vehicles` | List account-linked vehicles with reading counts, upload counts, exact-VIN sharing state, pending approval counts, and owner-visible metadata. Query: `?limit=N` |
| POST | `/api/vehicles/{vehicle_id}/exact-vin-sharing` | Enable or disable this account's exact-VIN public sharing preference for all linked uploads on that vehicle |
| POST | `/api/vehicles/{vehicle_id}/approve-exact-vin-sharing` | Dev/test-only manual approval for this account's currently linked exact-VIN public uploads |
| POST | `/api/vehicles/{vehicle_id}/approve-cohort-sharing` | Dev/test-only manual approval for this account's currently linked cohort public uploads |
| DELETE | `/api/vehicles/{vehicle_id}` | Remove this account's private access to a vehicle while leaving already approved public stats intact |
| POST | `/api/auth/register` | Create account and set session cookie |
| POST | `/api/auth/login` | Verify username/password and set session cookie |
| POST | `/api/auth/logout` | Clear current session |
| GET | `/api/auth/me` | Current account or dev/test guest fallback, plus `capabilities.approve_pending_public_stats` |
| POST | `/api/ingest/csv?vin=VIN` | Upload live OBD CSV export body → `{"rows_ingested": N}` or `400` on metric value-kind conflict |
| POST | `/api/dropbox/oauth/start` | Start Dropbox OAuth for the current signed-in non-guest account |
| GET | `/api/dropbox/oauth/callback` | Finish Dropbox OAuth and store the encrypted refresh token |
| GET | `/api/dropbox/connection` | Inspect the current account Dropbox connection, folder, sync state, counts, and latest error |
| POST | `/api/dropbox/connection/folder` | Update the monitored Dropbox root folder |
| POST | `/api/dropbox/connection/pause` | Pause or resume the current account Dropbox connection |
| POST | `/api/dropbox/connection/sync-now` | Queue one immediate Dropbox sync for the current account |
| DELETE | `/api/dropbox/connection` | Remove the current Dropbox connection without deleting telemetry |
| GET | `/api/analysis/dashboard` | Batched dashboard series. Query: `?view=summary`, `?limit=N`, `?channel_limit=N`, `?vehicle_id=UUID`, `?start=...`, `?end=...`, `?bucket=1d|1w|1mon`, `?channels=key1,key2` |
| GET | `/api/analysis/pairs` | Owner-scoped exact-time numeric metric pairs. Query: `?x=key1&y=key2`, optional `vehicle_id`, `start`, `end`, `limit` |
| GET | `/api/analysis/trends/{channel}` | Raw time series. Query: `?limit=N`, `?vehicle_id=UUID` |
| GET | `/api/analysis/summary/{channel}` | Daily/weekly/monthly aggregates (avg/min/max/count). Query: `?bucket=1d|1w|1mon`, `?vehicle_id=UUID`, `?limit=N`. Reads `vehicle_metric_day`. |
| GET | `/api/analysis/cohort/{channel}` | Aggregate-only cohort comparison. Query: `?year=YYYY`, `?make=MAKE`, `?model=MODEL`, `?engine_family=FAMILY`, `?bucket=1d|1w|1mon`, `?min_vehicles=5` |
| GET | `/api/analysis/latest/{vehicle_id}` | 50 most recent readings for a vehicle |
| GET | `/api/public/vehicle/{vin}` | Approval-gated exact-VIN public aggregates sourced from shared daily rollups only |

Account scoping uses an HttpOnly dashboard session cookie. Dev/test mode falls back to the
deterministic `guest`/`local-dev` account when no credential is supplied;
production disables that fallback unless `SCARGO_ENABLE_GUEST=true`. Deprecated
`X-Scargo-User-Key` is accepted only as a dev/test fallback. Raw vehicle reads
are scoped to uploads still linked to the request account through
`account_vehicle_upload.private_access`. Year/make/model/engine-family cohort
sharing is always on for accepted uploads but becomes public only after
`approved_cohort_at` approval. Exact-VIN public stats additionally require both
approval and a per-account enabled sharing flag. See `docs/privacy-model.md`.
Vehicle listings also include pending approval counts for those two public
sharing paths. In dev/test mode, signed-in non-guest sessions can manually
approve only their own still-private linked uploads; guests and production mode
cannot use those approval routes.

## CSV format

The ingest parser accepts VIN-scoped OBD CSV exports. The common live shape is
first line `StartTime`, optional metadata rows, sensor headers, then rows keyed
by elapsed seconds. VIN is supplied by `?vin=...` from the dashboard upload or
by the Dropbox folder name, not by a CSV column.

```csv
# StartTime = 03/27/2026 06:54:01.3973 PM
Time (sec),Engine RPM (RPM),Vehicle speed (MPH),Intake manifold absolute pressure (kPa),...
4.406,1074,2.4854848,48,...
```

Every non-time header is normalized into a metric key and retained with its
label, numeric value when parseable, or text value otherwise. Known numeric
metrics are converted into canonical storage units during ingest; unknown
headers and unsupported unit variants fall back to raw metric keys. Bare
acceleration units `m/s` and `ft/s` are accepted as aliases for the existing
acceleration family and stored in canonical `m/s²`.

Metric policy lives in `src/ingest/canonical.rs` and is documented in
`docs/metric-policy.md`. `/api/channels` exposes each channel's `category`,
`sensitivity`, `rollup`, `public_cohort`, and `derived_preferred` flags. Unknown
future keys default to owner-only raw data: no durable daily rollup and no public
cohort exposure.

`obd2_metric` is global by key rather than vehicle-scoped. Each key has one
strict `value_kind` across the whole registry: `numeric` keys always write
`value`, `text` keys always write `text_value`, and ingest rejects mixed
numeric/text reuse of the same key with `400 Bad Request`. Blank CSV cells are
still skipped and do not create rows.

Daily rollups are allowlisted by metric policy. GPS, phone sensor, trip
behavior, adapter, system, and unknown numeric channels stay owner-scoped raw
rows and are not written to `vehicle_metric_day`. Public cohorts reject channels
whose policy has `public_cohort=false`.

The upload VIN is decoded locally for basic metadata. `src/ingest/vin.rs`
stores the model year from the 10th VIN character and a small common WMI make
map. `model` and `engine_family` are preserved across later ingests. When an
exact 17-character VIN is still missing public-cohort metadata, ingest first
tries a unique exact VIN-pattern match from existing vehicle or decode-cache
metadata, then uses cached NHTSA vPIC data, and only then calls vPIC if the
per-VIN retry window and app-wide throttle allow it. Failed or incomplete vPIC
lookups are cached and retried no more often than their `next_retry_after`
timestamp. Non-VIN vehicle keys never trigger runtime metadata lookup.

Duplicate headers are retained with stable suffixes such as
`intake_manifold_absolute_pressure` and `intake_manifold_absolute_pressure_2`.
Each accepted upload gets its own `ingest_upload.id`; raw rows and daily rollups
carry that `upload_id` so private access can be revoked per account without
rewriting or deleting already approved public aggregates.

Deployed Dropbox ingestion uses one saved OAuth connection per signed-in
non-guest account. Users manage that connection on `/dropbox.html`. Set
`SCARGO_DROPBOX_ENABLED=true` to enable the background poller and
`SCARGO_DROPBOX_POLL_SEC` for its interval. Dropbox sync uses
`files/list_folder` cursors, defaults to `/Apps/OBD Fusion/CsvLogs`, accepts only direct `<vehicle-key>/<file>.csv`
children under the selected root, records root or nested CSV skips in
`dropbox_ingest_file`, skips already ingested path+revision rows, and calls the
same account-scoped CSV helper as manual uploads. Exact 17-character VIN folder
names first try a unique VIN-pattern match from existing metadata, then use the
cached NHTSA vPIC path in `vin_decode_cache`, subject to per-VIN retry and
app-wide throttle limits. Non-VIN vehicle keys never trigger runtime metadata
lookup. Refresh tokens are encrypted at rest and CSV bytes are discarded after
ingest completes.
Existing connections saved with `/OBD Fusion/CsvLogs` must save
`/Apps/OBD Fusion/CsvLogs` once on `/dropbox.html` or reconnect Dropbox; startup
does not rewrite saved roots.

For a one-time full relationship report after data is uploaded, run
`python3 scripts/analyze-telemetry.py`. It uses `psql`, reads the same
`SCARGO_DATABASE_URL` or dev `POSTGRES_*` defaults as the app, and writes
`analysis/telemetry-relationships.json` plus `.csv`. By default it reads
`vehicle_metric_day`; `--raw-relationships` exists only as a slower debugging
path, and `--vin VIN` keeps exact-sample reconstruction on raw rows.
Pass
`--events events.csv` to analyze known maintenance/fuel-quality dates. Event CSV
columns are `label,date,vehicle_id,before_days,after_days,uncertainty_days`;
`vehicle_id` can be blank, and `uncertainty_days` lets approximate dates skip a
window around the event. Outputs are `analysis/telemetry-events.json` and `.csv`.
Pass `--vin VIN` to greedily find a minimum key set for reconstructing that
vehicle's numeric dataset by same-sample correlation. Tune coverage with
`--reconstruct-threshold` (default `0.98`). Outputs are
`analysis/telemetry-reconstruction.json` and `.csv`.

Use `python3 scripts/rollup-retention-report.py` to inspect raw-vs-rollup
coverage and footprint. Vehicle metadata enrichment is automatic during ingest:
Scargo uses local VIN structure first, then unique exact-pattern inference, then
cached NHTSA vPIC results, and finally a throttled vPIC request only when public
cohort metadata is still missing.

## Development conventions

### Required agent workflow
- Run `git status --short --branch` before making edits.
- If a target file is dirty, inspect the relevant `git diff` before editing it.
- Preserve user changes and unrelated work. Do not revert, overwrite, or clean
  files unless the user explicitly asks.
- Use git as an inspection and audit tool. Do not commit unless the user
  explicitly asks.
- Before the final response, run `git diff --stat` and
  `git status --short --branch`, then summarize only the files you changed.

### Documentation rule
Any API, schema, config, script, dashboard, or ingest behavior change must
update repo documentation in the same task.

Use the narrowest durable location:
- `README.md` for user-facing behavior and setup.
- `AGENTS.md` for agent/developer context and workflow.
- `docs/` for durable design notes and policy decisions.
- `todo/<feature-title>/PLAN.md` for requested implementation plans that future
  agents can claim and execute.
- `todo/completed/<feature-title>/` for reviewed, approved, and merged plan
  folders that should remain archived.

### Planning workflow
The intended workflow is idea intake first, then implementation by clean-context
agents. Propose or clarify the idea, then decompose accepted work into durable
`todo/` task folders.

When asked to make a plan, create a top-level folder named
`todo/<kebab-feature-title>/`. If the idea is large, create multiple atomic
task folders or subfolders with their own `PLAN.md` files. Each task should be
concise, independently claimable when possible, and focused on one code or docs
area so future agents can keep context small.
Implement each plan on a dedicated git branch named by work type, then task,
for example `feature/<kebab-feature-title>`, `fix/<kebab-feature-title>`,
`docs/<kebab-feature-title>`, `refactor/<kebab-feature-title>`, or
`research/<kebab-feature-title>`. A separate checkout is optional, but the
branch naming is required. Each folder must contain a `PLAN.md` that is
self-contained for an agent with no prior context.

Include:
1. Goal and success criteria
2. Implementation instructions
3. Tools and commands to use
4. Relevant files, data, and context
5. Acceptance checks and tests
6. Suggested branch name in the form `<kind>/<kebab-feature-title>`

Future agents claim one folder, create or reuse the matching branch,
and work there until review, approval, and merge. After merge, move it to
`todo/completed/<kebab-feature-title>/` or delete it only when explicitly
asked.

### Code style
- No unsafe code.
- Prefer `map_err(|_| Error::Database)` over verbose error handling.
- Keep functions short — a junior dev should understand any function in under 2 minutes.
- Return types use `crate::Error` (re-exported from `config::error`).
- Database access always goes through `db.get().await?` (pool checkout).

### Adding a metric
CSV ingest logs every non-time header automatically. Add code only when the
application needs canonical unit conversion, special display metadata,
redaction, aggregation, public-cohort eligibility, or derived metrics for that
key. Default new or unknown keys to owner-only raw until there is a concrete
reason to roll them up or expose aggregate cohorts.

### Testing
- Unit tests live alongside code (`#[cfg(test)] mod tests { … }`)
- Ignored real-database smoke tests live in `tests/smoke_stack.rs`
- Run: `cargo test`
- `src/ingest/csv.rs` owns a concise `test_data/**/*.csv` parser smoke test;
  keep those fixtures fast and representative.
- Real vehicle CSV exports live in Dropbox, not in this repository.

### Database bootstrap
- `src/db/migrate.rs` is clean schema bootstrap for the current database shape,
  not a compatibility migration layer.
- Do not add legacy table rewrites, giant `UPDATE`s, or large dedupe work to app
  startup.
- Large future data changes must be staged/resumable scripts under `scripts/`.

## Architecture decisions

1. **deadpool-postgres over raw tokio_postgres::Client:** Connection pooling is essential
   for concurrent HTTP requests.  deadpool is the standard Rust async pool for tokio-postgres.

2. **TimescaleDB hypertables:** `obd2_metric_reading` is always a hypertable. This gives us
   automatic partitioning by time for raw metric data, which is critical for
   telematics where data volume grows fast. Startup fails if TimescaleDB is unavailable.

3. **Single-page dashboard with CDN Chart.js:** No build step, no npm, no template engine.
   actix-web serves the HTML/JS files directly.  The JS fetches JSON from the API and
   renders charts client-side.  Simple enough that a junior dev can understand the whole
   dashboard in 10 minutes.

4. **CSV-first ingest:** The initial data path is CSV files.  The ingest endpoint accepts
   dashboard uploads and Dropbox worker downloads.  In the future, a mobile SDK will
   send preprocessed readings directly to the API (new endpoint, same db tables).

5. **Account boundary before full auth platform:** Dashboard auth uses a
   dedicated `/auth.html` page with username/password plus HttpOnly session
   cookies for dashboard upload and Dropbox management. Guest access is for
   dev/test only and requires an explicit browser-side continue action before
   `/` loads. Private raw access now lives on upload links in
   `account_vehicle_upload`, which prevents raw telemetry endpoints from
   accidentally becoming global reads while allowing public approvals to outlive
   account access.

6. **Comparison by aggregate cohorts only:** Cross-vehicle analytics must not expose
   VINs, owner ids, peer vehicle ids, upload filenames, exact trip boundaries, or raw
   rows from other accounts. Use minimum-size year/make/model/engine-family cohorts
   and aggregated statistics. The current design target is in `docs/privacy-model.md`.

## Environment variables

| Variable | Default | Notes |
|----------|---------|-------|
| `SCARGO_ENV` | `dev` | Use `production` to require an explicit database URL |
| `SCARGO_DATABASE_URL` | unset | PostgreSQL connection string; required in production |
| `SCARGO_HTTP_HOST` | `127.0.0.1` | Bind address |
| `SCARGO_HTTP_PORT` | `8080` | Bind port |
| `SCARGO_ENABLE_GUEST` | dev/test enabled, production disabled | Enable or disable unauthenticated guest fallback |
| `SCARGO_DROPBOX_ENABLED` | `false` | Enable Dropbox OAuth ingest and background polling |
| `DROPBOX_APP_KEY` | unset | Dropbox OAuth app key; required when Dropbox ingest is enabled |
| `DROPBOX_APP_SECRET` | unset | Dropbox OAuth app secret; required when Dropbox ingest is enabled |
| `SCARGO_BASE_URL` | unset | Public app base URL; default Dropbox callback = this value + `/api/dropbox/oauth/callback` |
| `SCARGO_DROPBOX_REDIRECT_URI` | unset | Optional exact Dropbox callback override when the registered URI must use a different host or path |
| `SCARGO_TOKEN_ENCRYPTION_KEY` | unset | 32-byte hex AES-GCM key for stored Dropbox refresh tokens |
| `SCARGO_DROPBOX_POLL_SEC` | `300` | Poll interval for active Dropbox connections |
| `POSTGRES_HOST` | `127.0.0.1` | Dev-mode local database host when URL is unset |
| `POSTGRES_PORT` | `5432` | Dev-mode local database port when URL is unset |
| `POSTGRES_USER` | `scargo` | Dev-mode local database user when URL is unset |
| `POSTGRES_PASSWORD` | unset | Optional dev-mode local database password when URL is unset |
| `POSTGRES_DB` | `scargo` | Dev-mode local database name when URL is unset |
| `RUST_LOG` | `info` | Tracing filter (info, debug, trace) |

Configuration is environment-only. Use ignored `.env` files for local overrides.

<!-- lean-ctx-compression -->
OUTPUT STYLE: expert-terse
- Telegraph format: subject-verb-object, drop articles/prepositions
- Symbolic vocabulary: → cause, ∵ because, ∴ therefore, ⊕ add, ⊖ remove, Δ change, ≈ similar, ≠ different, ∈ in/member, ∅ empty/none, ✓ ok, ✗ fail
- Code blocks: untouched (never compress code syntax)
- Each line: max 80 chars
- Zero narration, zero filler
- BUDGET: ≤100 tokens per non-code response
<!-- /lean-ctx-compression -->

<!-- lean-ctx -->
## lean-ctx

lean-ctx is active — the MCP tools replace native equivalents.
Full rules: LEAN-CTX.md (open on demand — do not auto-load).
<!-- /lean-ctx -->
