# Scargo — OBD2 Telematics

OBD2 telematics ingestion, analysis, and dashboard built in Rust.

Scargo starts as a cheap developer project for CSV-based vehicle uploads and is
intended to grow into a multi-user analytics platform where users can inspect
their own vehicles, evaluate used-car sensor behavior, and compare vehicles
against similar year/make/model cohorts. Early collection keeps broad telemetry
so useful correlations can be found; later clients should upload a smaller
preprocessed sensor set once derived metrics are proven.

## Quick Start

```bash
# 1. Start the database
docker compose up -d scargo_db

# Optional without Docker: create a TimescaleDB-backed local DB
# createdb scargo
# psql scargo -c "CREATE EXTENSION IF NOT EXISTS timescaledb;"

# 2. Run
cargo run --release
# Dashboard at http://localhost:8080
```

For a local smoke check against the Compose database, run:

```bash
scripts/smoke-docker.sh
```

The helper loads ignored `.env` and `.env.smoke` files, starts `scargo_db` with
Docker Compose, supplies a local default `POSTGRES_PASSWORD` when none is set,
waits for Postgres readiness, then runs the ignored smoke test. The smoke test
uses `POSTGRES_HOST`, `POSTGRES_PORT`, `POSTGRES_USER`, `POSTGRES_PASSWORD`, and
`SCARGO_SMOKE_ADMIN_DB=postgres` to create a disposable `scargo_smoke_*`
database, starts Scargo on `SCARGO_SMOKE_HTTP_PORT=18080`, checks HTTP health,
uploads a tiny CSV, verifies vehicle/channel/dashboard data, then drops the
temporary database. It ignores `SCARGO_DATABASE_URL` so smoke
runs cannot target a live application database by accident. It does not require
GitLab CI, GitHub Actions, `curl`, or repository secrets.

`scripts/reset-dev-db.sh` is destructive to the local Compose volume only.
CSV ingest is exposed through the dashboard upload form and the Dropbox worker;
there are no local drop-folder ingest helpers.

Scargo defaults to `SCARGO_ENV=dev`. In dev mode, `SCARGO_DATABASE_URL` wins
when it is set; otherwise the app builds a local URL from `POSTGRES_HOST`,
`POSTGRES_PORT`, `POSTGRES_USER`, optional `POSTGRES_PASSWORD`, and
`POSTGRES_DB`. Those parts match the Docker Compose defaults, so local dev does
not need a committed database URL.
Local database connections use plain PostgreSQL on localhost or the Compose
network. Add TLS only when a production database requires it.

Keep real secrets in ignored `.env` or `.env.*` files. The tracked
`.env.example` documents placeholder names only. This checkout also has an
ignored `.env.smoke` for local smoke-test database credentials.

Production must run with `SCARGO_ENV=production` and an explicit
`SCARGO_DATABASE_URL` supplied by the environment or ignored `.env`. If Dropbox
ingest is enabled, production also needs `DROPBOX_APP_KEY`,
`DROPBOX_APP_SECRET`, `SCARGO_BASE_URL`, and a 32-byte hex
`SCARGO_TOKEN_ENCRYPTION_KEY` so refresh tokens stay encrypted at rest. Runtime
config is environment-only: use `SCARGO_*` for app settings and `POSTGRES_*`
for the dev database fallback. See `.env.example` for placeholder settings.
Set `SCARGO_DROPBOX_REDIRECT_URI` only when Dropbox must redirect to an exact
host or path that differs from `SCARGO_BASE_URL + /api/dropbox/oauth/callback`.
Dropbox ingest uses Full Dropbox OAuth because OBD Fusion writes exports into
its own Dropbox app folder. Scargo stores only an encrypted refresh token and a
cursor, lists the selected Dropbox folder incrementally, downloads only unseen
CSV revisions, streams those bytes into the existing ingest path, and does not
retain CSV or ZIP artifacts outside the database. The Dropbox poll interval
defaults to `SCARGO_DROPBOX_POLL_SEC=300`.

## Features

- **CSV ingestion** — upload OBD2 CSV exports via HTTP API
- **Time-series storage** — TimescaleDB hypertables on PostgreSQL
- **Trend analysis** — query historical readings by channel and vehicle
- **Relationship analysis** — graph numeric metrics against each other and run one-time correlation reports
- **Owner-scoped reads** — vehicle and raw telemetry APIs are scoped by account
- **Dropbox OAuth ingest** — poll one account-owned Dropbox connection for new CSV revisions only
- **Dashboard history** — browse older readings with relative or custom time windows
- **Web dashboard** — real-time charts with Chart.js, zero build step

The dashboard visual system follows the Scargo creative direction documented in
[docs/dashboard-creative-direction.md](docs/dashboard-creative-direction.md).

The hosted-beta roadmap and active task plan index live in
[docs/roadmap.md](docs/roadmap.md).

## Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust 2021 |
| Web framework | actix-web 4 |
| Database | TimescaleDB on PostgreSQL |
| Connection pool | deadpool-postgres 0.14 |
| Dashboard | Vanilla JS + Chart.js 4 (CDN) |
| Serialization | serde + serde_json |
| CSV parsing | csv crate |
| Config | dotenvy |

## API

Dashboard users start on `/auth.html`, then register or log in with
username/password credentials before entering `/`. The browser uses an HttpOnly
session cookie for dashboard reads, dashboard CSV upload, and Dropbox OAuth
management. Signed-in users manage Dropbox OAuth ingest on `/dropbox.html`.
Dev/test mode still exposes the
deterministic guest account, but the browser must explicitly choose `Continue as
guest` on `/auth.html`; set `SCARGO_ENV=production` or
`SCARGO_ENABLE_GUEST=false` to disable that fallback. Raw vehicle reads are
scoped to the authenticated account.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/health` | Health check |
| POST | `/api/auth/register` | Create an account and set a session cookie |
| POST | `/api/auth/login` | Verify username/password and set a session cookie |
| POST | `/api/auth/logout` | Clear the current session cookie |
| GET | `/api/auth/me` | Current account, or guest in dev/test fallback mode, plus `capabilities.approve_pending_public_stats` |
| POST | `/api/dropbox/oauth/start` | Start Dropbox OAuth for the current signed-in non-guest account |
| GET | `/api/dropbox/oauth/callback` | Finish Dropbox OAuth and persist the encrypted refresh token |
| GET | `/api/dropbox/connection` | Current Dropbox connection status, selected folder, sync state, counts, and latest error |
| POST | `/api/dropbox/connection/folder` | Update the Dropbox root folder to monitor for new CSV files |
| POST | `/api/dropbox/connection/pause` | Pause or resume Dropbox polling for the current account |
| POST | `/api/dropbox/connection/sync-now` | Queue an immediate Dropbox sync for the current account |
| DELETE | `/api/dropbox/connection` | Remove the current Dropbox connection while keeping ingested telemetry |
| GET | `/api/channels` | Channel registry used by the dashboard, including display-unit and metric-policy metadata |
| GET | `/api/vehicles` | Account-linked vehicles with owner-visible metadata, reading counts, upload counts, sharing state, and pending approval counts |
| POST | `/api/vehicles/{vehicle_id}/exact-vin-sharing` | Enable or disable this account's exact-VIN public sharing preference |
| POST | `/api/vehicles/{vehicle_id}/approve-exact-vin-sharing` | Dev/test-only manual approval for this account's pending exact-VIN public uploads |
| POST | `/api/vehicles/{vehicle_id}/approve-cohort-sharing` | Dev/test-only manual approval for this account's pending cohort public uploads |
| DELETE | `/api/vehicles/{vehicle_id}` | Drop the vehicle from this account's private view while leaving approved public stats intact |
| POST | `/api/ingest/csv?vin=VIN` | Upload one OBD CSV export |
| GET | `/api/analysis/dashboard` | Batched dashboard series. Query: `?view=summary`, `?limit=N`, `?channel_limit=N`, `?vehicle_id=UUID`, `?start=...`, `?end=...`, `?bucket=1d|1w|1mon`, `?channels=key1,key2` |
| GET | `/api/analysis/pairs` | Owner-scoped numeric metric pairs for scatter charts. Query: `?x=key1&y=key2`, plus optional `vehicle_id`, `start`, `end`, and `limit` |
| GET | `/api/analysis/trends/{channel}` | Time-series for a channel, with optional `start`/`end` bounds |
| GET | `/api/analysis/summary/{channel}` | Daily/weekly/monthly aggregates from `vehicle_metric_day`, with optional `start`/`end` bounds |
| GET | `/api/analysis/cohort/{channel}` | Aggregate-only comparison by `year`, `make`, `model`, and `engine_family` |
| GET | `/api/analysis/latest/{vehicle_id}` | Recent readings for a vehicle |
| GET | `/api/public/vehicle/{vin}` | Approval-gated public exact-VIN aggregate stats |

## CSV Input

The ingest path accepts VIN-scoped OBD CSV exports. The common shape is:

```csv
# StartTime = 03/27/2026 06:54:01.3973 PM
Time (sec),Engine RPM (RPM),Vehicle speed (MPH),Intake manifold absolute pressure (kPa),...
4.406,1074,2.4854848,48,...
```

Metadata rows before the header are skipped. Every non-time header is normalized
into a metric key. Known numeric metrics are converted into one canonical
storage unit per metric family during ingest, while unknown headers keep the
current raw-key fallback and preserve their parsed unit. Bare acceleration
headers using `m/s` or `ft/s` are normalized into the existing acceleration
family and stored in canonical `m/s²`.

The upload VIN is decoded locally for basic vehicle metadata. Scargo stores the
model year from the 10th VIN character and a small common WMI-to-make map when
available. `model` and `engine_family` are preserved across later ingests. For
valid 17-character VINs that still lack public-cohort metadata, ingest tries a
unique exact VIN-pattern match from existing data, then cached NHTSA vPIC data,
then a throttled vPIC lookup only when needed. Failed or incomplete lookups are
cached with a retry timestamp so uploads do not hammer the official service.

Scargo also stores every non-time CSV column as an owner-scoped raw metric, even
when it is not a known OBD channel. GPS, acceleration, status fields, duplicate
headers, and future logger-specific fields are retained and queryable. Sensitive
fields such as GPS and phone sensors are not written to daily rollups and are
not exposed through cross-vehicle comparison endpoints.
Raw metric labels, canonical storage units, and the metric value kind are stored
once in the global `obd2_metric` registry; time-series rows store the metric id
rather than repeating header text for every sample.

Each metric key now has one strict value type across the whole registry. A key
that has already been established as numeric cannot later ingest text, and a
text key cannot later ingest numeric values. Blank cells are still skipped, but
type conflicts return `400 Bad Request` from `POST /api/ingest/csv`.

Known equivalent headers and unit variants are grouped under the same channel
key. Examples:
- `rpm` and `Engine RPM (RPM)` -> `engine_rpm`
- `speed`, `Vehicle speed (MPH)`, and `Vehicle speed (km/h)` -> `vehicle_speed`
- `MAP (psi)` and `Intake manifold absolute pressure (kPa)` ->
  `intake_manifold_absolute_pressure`

The dashboard uses `/api/channels` metadata to let users pick display units per
metric on the front end without changing the stored series or analysis routes.
It also returns each metric's `category`, `sensitivity`, `rollup`,
`public_cohort`, and `derived_preferred` policy fields. New ingest writes raw
metrics and incrementally maintains a durable `vehicle_metric_day` rollup only
for allowlisted numeric vehicle metrics. `obd2_metric_reading` remains a
TimescaleDB hypertable without a per-sample uniqueness constraint so recent
detail can stay available while long-term day/week/month reads move to the daily
rollup. Duplicate upload packets are still blocked by `ingest_upload`, and both
raw rows and daily rollups now carry `upload_id` so account-private access can
be revoked without deleting already approved public aggregates.

The vehicle key is supplied by the dashboard upload form or Dropbox folder name,
not by the CSV body.

Duplicate headers are retained with stable suffixes such as
`intake_manifold_absolute_pressure` and `intake_manifold_absolute_pressure_2`.

Signed-in non-guest dashboard sessions can manage one Dropbox OAuth connection
on `/dropbox.html` through `/api/dropbox/*`. The monitored root defaults to
`/Apps/OBD Fusion/CsvLogs` and can be changed per account. `POST /dropbox/oauth/start`
begins the browser redirect flow, `GET /dropbox/connection` returns current
status and counts, `POST /dropbox/connection/folder` changes the monitored
root, `POST /dropbox/connection/pause` toggles active/paused,
`POST /dropbox/connection/sync-now` queues a background sync, and `DELETE
/dropbox/connection` removes the connection without deleting telemetry. Guests
cannot manage Dropbox connections. The Dropbox path
contract is `<vehicle-key>/<file>.csv`; root-level CSV files are recorded as
skipped, nested CSVs are skipped for v1, and non-CSV files are ignored. Exact
17-character VIN folders first try a unique existing VIN-pattern match from
known metadata, then fall back to cached or throttled NHTSA vPIC metadata during
sync.
When Dropbox rejects a token refresh, folder listing, or file download, the
saved connection `latest_error` now includes the Dropbox HTTP status and a
short upstream error summary when one is present.
Dropbox redirect URI matching is exact. The default callback is
`SCARGO_BASE_URL + /api/dropbox/oauth/callback`; when a proxy or deployment
needs a different callback host/path, configure the exact registered value in
`SCARGO_DROPBOX_REDIRECT_URI`.

Dropbox access uses Full Dropbox visibility because OBD Fusion writes to its
own app folder. Scargo does not mirror that folder locally: it stores Dropbox
cursor state and per-file ingest metadata in PostgreSQL, downloads CSV bytes
only when a revision is new, and discards those bytes after ingest completes.
Accounts already saved with `/OBD Fusion/CsvLogs` must save
`/Apps/OBD Fusion/CsvLogs` once on `/dropbox.html` or reconnect Dropbox.
Before Dropbox file APIs run, Scargo resolves the account root namespace with
`users/get_current_account` and sends `Dropbox-API-Path-Root` on list and
download calls so full-account paths continue to work on accounts whose home
namespace differs from the root namespace.

## One-time relationship analysis

After loading data, run:

```bash
python3 scripts/analyze-telemetry.py
```

The script uses `psql` and the same `SCARGO_DATABASE_URL` or `POSTGRES_*`
environment defaults as the app. It writes pairwise numeric relationship
reports to `analysis/telemetry-relationships.json` and
`analysis/telemetry-relationships.csv`. By default it reads `vehicle_metric_day`
so long-term trend and retention reports stay off raw rows. Use
`--raw-relationships` for debugging the slower old Python path, and use
`--vin VIN` when exact-sample raw reconstruction work is required.

Known maintenance or fuel-quality events can be supplied as CSV:

```csv
label,date,vehicle_id,before_days,after_days,uncertainty_days
oil change,2026-04-15,,90,14,0
bad gas suspected,2026-05-03,,14,7,2
```

Run:

```bash
python3 scripts/analyze-telemetry.py --events events.csv
```

`before_days` measures long degradation before the event, `after_days` measures
rapid recovery after it, and `uncertainty_days` skips an approximate event window
around the date. Event outputs are written to `analysis/telemetry-events.json`
and `analysis/telemetry-events.csv`.

To estimate the smallest set of metrics needed to reconstruct a vehicle's full
numeric dataset, pass the VIN:

```bash
python3 scripts/analyze-telemetry.py --vin DEMO-HONDA-ACCORD
```

The script greedily selects keys that cover other same-sample keys whose absolute
correlation is at least `--reconstruct-threshold` (default `0.98`). Outputs are
`analysis/telemetry-reconstruction.json` and
`analysis/telemetry-reconstruction.csv`. This VIN reconstruction uses the same
SQL aggregate relationship output and avoids materializing every raw reading in
Python.
Generated `analysis/` outputs are ignored and should not be committed.

TimescaleDB stores raw metric readings in a hypertable. Compression policy and
an hourly continuous aggregate are configured during runtime bootstrap.

## Rollups and retention

Daily owner/public reads now flow through `vehicle_metric_day`:

- `1d` summary buckets read daily rows directly
- `1w` and `1mon` roll up daily rows with weighted averages from `value_sum / reading_count`
- daily rollups include only metric-policy allowlisted numeric vehicle channels
- public cohorts require `year`, `make`, `model`, `engine_family`, and a public metric-policy channel
- vehicles missing `model` or `engine_family` stay owner-visible but are excluded from public cohorts

Use `python3 scripts/rollup-retention-report.py` to inspect raw-vs-rollup
coverage and footprint.

Automatic VIN enrichment normalizes `engine_family` to `EV`, `Hybrid`, `PHEV`,
or labels such as `2.4L 4cyl NA`, `1.5L I4 Turbo`, and `3.5L V6 NA` when vPIC
provides cylinder configuration. When cylinder configuration is missing, Scargo
keeps the conservative `6cyl` style instead of guessing `I` or `V`. The intended
retention target is 180 days of compressed raw rows plus indefinite daily
rollups.

## Privacy And Scaling

Scargo separates upload-linked private access from stable vehicle identity. VIN
is stored internally for ingest and de-duplication, but `/api/vehicles` returns
only the owner-visible vehicle UUID handle and non-sensitive metadata. Raw
telemetry APIs return data only for uploads still linked to the request account.
Users can manage vehicle access on `/vehicles.html`, including dropping a
vehicle from their account. In dev/test signed-in sessions, the same page can
also approve the account's still-pending exact-VIN or cohort uploads when manual
approval is enabled. Exact-VIN public stats are optional per account and remain
approval-gated. Year/make/model/engine-family cohort sharing is always on for
accepted uploads, but public cohort reads require approval and metric-policy
eligibility.

Cross-vehicle comparison features must use cohort aggregates rather than peer
raw rows. See [docs/privacy-model.md](docs/privacy-model.md) for the privacy
model, ownership assumptions, comparison rules, and cost-scaling path. See
[docs/metric-policy.md](docs/metric-policy.md) for the metric category,
rollup/public allowlist, and minimal-measurement policy.

## Project Layout

```
docs/               # Roadmap, privacy, metric policy, deployment, product notes
src/
├── main.rs          # Entry point
├── config/          # Settings, error types
├── db/              # Connection pool, schema bootstrap
├── ingest/          # CSV parsing and insertion
└── api/             # HTTP handlers
dashboard/
└── static/          # Web dashboard (HTML + JS)
todo/               # Active and completed claimable implementation plans
```

See [AGENTS.md](AGENTS.md) for the full agent/developer context.

## License

MIT
