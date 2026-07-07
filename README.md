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
temporary database and test drop-root. It ignores `SCARGO_DATABASE_URL` so smoke
runs cannot target a live application database by accident. It does not require
GitLab CI, GitHub Actions, `curl`, or repository secrets.

For a fresh local upload check without one all-in-one script, use three separate
steps against a small local drop folder. Real vehicle exports live in Dropbox.

```bash
# 1. Run Scargo in another terminal
cargo run --release

# 2. Create a local account and capture an upload token
token="$(python3 scripts/local-auth.py your_username 'your-password')"

# If the account already exists, log in and mint a fresh token instead:
# token="$(python3 scripts/local-auth.py --login your_username 'your-password')"

# 3. Upload a small local drop folder when needed
python3 scripts/scan-and-ingest.py drop-root --once --reset-state --api-token "$token"
```

`scripts/reset-dev-db.sh` is destructive to the local Compose volume only.
`scripts/local-auth.py` talks to `/api/auth/register` or `/api/auth/login` plus
`/api/auth/tokens` and prints the token to stdout. Dropbox is the source of
record for real export data.

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
`SCARGO_DATABASE_URL` supplied by the environment or ignored `.env`. Runtime
config is environment-only: use `SCARGO_*` for app settings and `POSTGRES_*`
for the dev database fallback. See `.env.example` for placeholder settings.
Dropbox ingest is off by default. Enable it only when
`SCARGO_DROPBOX_ENABLED=true` and all of `DROPBOX_APP_KEY`,
`DROPBOX_APP_SECRET`, `SCARGO_BASE_URL`, and `SCARGO_TOKEN_ENCRYPTION_KEY` are
set. The encryption key must decode to 32 bytes as hex or base64. v1 stores one
Dropbox connection per signed-in account and fixes the ingest root to
`/OBD Fusion/CsvLogs`. When enabled, Scargo starts a background worker that
polls active connections, treats each direct child folder as the VIN or vehicle
key, and ingests only `CsvLogs/<VIN>/*.csv` files for that account. Root-level
CSV files are skipped and recorded in connection/file status because they do not
identify a vehicle.
See [docs/dropbox-ingest.md](docs/dropbox-ingest.md) for Dropbox app setup,
redirect URI, folder layout, smoke checks, and deployment notes.

## Features

- **CSV ingestion** — upload OBD2 CSV exports via HTTP API
- **Time-series storage** — TimescaleDB hypertables on PostgreSQL
- **Trend analysis** — query historical readings by channel and vehicle
- **Relationship analysis** — graph numeric metrics against each other and run one-time correlation reports
- **Owner-scoped reads** — vehicle and raw telemetry APIs are scoped by account
- **Dashboard history** — browse older readings with relative or custom time windows
- **Web dashboard** — real-time charts with Chart.js, zero build step

The dashboard visual system follows the Scargo creative direction documented in
[docs/dashboard-creative-direction.md](docs/dashboard-creative-direction.md).

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
session cookie, while scripts and external upload tools use generated
`Authorization: Bearer` upload tokens. Successful registration still returns the
first upload token; the auth page stores it in browser session storage and the
dashboard shows it once after redirect. Dev/test mode still exposes the
deterministic guest account, but the browser must explicitly choose `Continue as
guest` on `/auth.html`; set `SCARGO_ENV=production` or
`SCARGO_ENABLE_GUEST=false` to disable that fallback. Raw vehicle reads are
scoped to the authenticated account.

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/health` | Health check |
| POST | `/api/auth/register` | Create an account, set a session cookie, and return a first upload token |
| POST | `/api/auth/login` | Verify username/password and set a session cookie |
| POST | `/api/auth/logout` | Clear the current session cookie |
| GET | `/api/auth/me` | Current account, or guest in dev/test fallback mode, plus `capabilities.approve_pending_public_stats` |
| POST | `/api/auth/tokens` | Create a new upload token for the logged-in account |
| POST | `/api/dropbox/oauth/start` | Create a Dropbox OAuth authorize URL for the signed-in non-guest account |
| GET | `/api/dropbox/oauth/callback` | Validate OAuth state, store encrypted Dropbox tokens, and redirect back into the app |
| GET | `/api/dropbox/connection` | Read Dropbox connection status for the signed-in non-guest account, or `enabled=false` when support is off |
| POST | `/api/dropbox/connection/pause` | Pause or resume Dropbox polling for the signed-in non-guest account |
| POST | `/api/dropbox/connection/sync-now` | Run one Dropbox sync pass for the signed-in non-guest account |
| DELETE | `/api/dropbox/connection` | Remove the signed-in non-guest account's Dropbox connection and stored tokens |
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
available. `model` and `engine_family` are preserved across later ingests, then
backfilled offline for public cohorts from an ignored local vPIC cache at
an ignored local VIN decode CSV. Scargo does not call VIN services during ingest or
request handling.

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
Signed-in non-guest users can also manage Dropbox ingest from the dashboard:
connect OAuth, pause or resume polling, run one sync pass, disconnect, and view
the fixed ingest folder plus last sync, success, ingest, duplicate, and latest
error status. Guest users do not see Dropbox controls.
Dropbox OAuth uses the redirect URI
`${SCARGO_BASE_URL}/api/dropbox/oauth/callback`, requests offline token access,
and stores encrypted token material, cursors, and per-file sync state in the
database. Keep that database storage persistent and backed up in any deployment
with Dropbox enabled.
It also returns each metric's `category`, `sensitivity`, `rollup`,
`public_cohort`, and `derived_preferred` policy fields. New ingest writes raw
metrics and incrementally maintains a durable `vehicle_metric_day` rollup only
for allowlisted numeric vehicle metrics. `obd2_metric_reading` remains a
TimescaleDB hypertable without a per-sample uniqueness constraint so recent
detail can stay available while long-term day/week/month reads move to the daily
rollup. Duplicate upload packets are still blocked by `ingest_upload`, and both
raw rows and daily rollups now carry `upload_id` so account-private access can
be revoked without deleting already approved public aggregates.

The vehicle key is supplied by the upload query string, not by the CSV body:

```bash
curl --data-binary @CSVLog_20260327_185401.csv \
  -H "Authorization: Bearer $SCARGO_API_TOKEN" \
  'http://localhost:8080/api/ingest/csv?vin=DEMO-HONDA-ACCORD'
```

Duplicate headers are retained with stable suffixes such as
`intake_manifold_absolute_pressure` and `intake_manifold_absolute_pressure_2`.

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

## Drop-folder ingest

For full same-machine reloads, place CSVs under folders named by a
non-identifying vehicle key:

```text
drop-root/
  DEMO-HONDA-ACCORD/
    CSVLog_20260327_185401.csv
  DEMO-TOYOTA-CAMRY/
    CSVLog_20260413_164511.csv
```

Rebuild the Scargo tables, bulk load the drop root, rebuild daily rollups, and
finalize runtime indexes in one command:

```bash
cargo run --bin scargo-bulk-ingest -- /path/to/drop-root --rebuild-db
```

The bulk loader bypasses HTTP, loads files directly into the database, rebuilds
`vehicle_metric_day` after raw load, then enables the normal runtime indexes,
compression policy, and hourly continuous aggregate. It continues past bad CSVs,
prints a failure summary, and exits non-zero if any file failed.

Use the Python watcher only for incremental uploads while the app is already
running:

```bash
python3 scripts/scan-and-ingest.py /path/to/drop-root --once
```

The watcher records successful uploads in
`/path/to/drop-root/.scargo-ingest-state.json`, keyed by vehicle key and SHA-256 file
hash, and leaves original CSVs where they are. Later scans skip already recorded
paths without rehashing, and still fall back to hash-based duplicate detection
when the same file appears at a new path.
The watcher posts the raw CSV bytes to the same `/api/ingest/csv` parser used by
manual uploads, so batch ingest accepts the same canonicalized test-data shapes.
Scargo also records each upload packet hash in `ingest_upload` with a database
uniqueness constraint on vehicle and content hash. Re-uploading the same CSV or
future data packet for the same vehicle is skipped by the API even when the
upload does not come from the drop-folder watcher.

Use `--api-token TOKEN` or `SCARGO_API_TOKEN=TOKEN` to claim watcher uploads for
a dashboard account. Deprecated `--user-key` and `SCARGO_USER_KEY` are accepted
only as a dev/test fallback. The watcher uploads files concurrently with
`--workers N` (default 4) and
hashes each file during the upload read to reduce bulk-ingest overhead. Worker
parallelism is allowed across all files; the API/database handle same-vehicle
serialization where needed. The watcher batches state-file writes every
`--state-save-every N` completed files (default 100) instead of rewriting the
whole JSON file after each upload.
Use `--timeout-sec N` to cap how long one HTTP upload can block shutdown.
For direct DB bulk loads, `--api-token TOKEN` resolves the account before
loading. A destructive `--rebuild-db` drops token rows, so local rebuild smoke
flows use the guest account unless a follow-up account/token is created.

Dropbox ingest follows the same parser and duplicate protections without using
the local watcher. It lists `/OBD Fusion/CsvLogs`, treats each direct child
folder as the VIN or vehicle key, ingests only direct CSV children, skips
root-level and nested CSV files with visible status, and leaves Dropbox files in
place.

TimescaleDB stores raw metric readings in a hypertable. Compression policy and
an hourly continuous aggregate are configured during runtime bootstrap and after
bulk-load finalization.

## Rollups and retention

Daily owner/public reads now flow through `vehicle_metric_day`:

- `1d` summary buckets read daily rows directly
- `1w` and `1mon` roll up daily rows with weighted averages from `value_sum / reading_count`
- daily rollups include only metric-policy allowlisted numeric vehicle channels
- public cohorts require `year`, `make`, `model`, `engine_family`, and a public metric-policy channel
- vehicles missing `model` or `engine_family` stay owner-visible but are excluded from public cohorts

After a bulk reload, use `python3 scripts/rollup-retention-report.py` to inspect
raw-vs-rollup coverage and footprint.

To enrich vehicles for public cohorts from an offline VIN decode export:

```bash
python3 scripts/fetch-vin-decodes.py --missing-only --cache vin-decodes.csv
python3 scripts/backfill-vehicle-metadata.py --decode-csv vin-decodes.csv --overrides overrides.csv
```

Normalize `engine_family` to `EV`, `Hybrid`, `PHEV`, or labels such as
`2.4L 4cyl NA`, `1.5L I4 Turbo`, and `3.5L V6 NA` when vPIC provides cylinder
configuration. Metadata backfill applies manual overrides
first, including optional `year` fixes for demo or malformed VINs, then exact
VIN cache hits, then conservative inference from prior direct decode rows only
when VIN positions 1-8 plus model year map to one unique
`(make, model, engine_family)` result. The intended retention target is 180
days of compressed raw rows plus indefinite daily rollups.

## Privacy And Scaling

Scargo separates upload-linked private access from stable vehicle identity. VIN
is stored internally for ingest and de-duplication, but `/api/vehicles` returns
only the owner-visible vehicle UUID handle and non-sensitive metadata. Raw
telemetry APIs return data only for uploads still linked to the request account.
Dropbox OAuth routes require a real signed-in account and reject guest fallback.
Users can manage that link on `/vehicles.html`, including dropping a vehicle
from their account. In dev/test signed-in sessions, the same page can also
approve the account's still-pending exact-VIN or cohort uploads when manual
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
src/
├── main.rs          # Entry point
├── config/          # Settings, error types
├── db/              # Connection pool, schema bootstrap
├── ingest/          # CSV parsing and insertion
└── api/             # HTTP handlers
dashboard/
└── static/          # Web dashboard (HTML + JS)
```

See [AGENTS.md](AGENTS.md) for the full agent/developer context.

## License

MIT
