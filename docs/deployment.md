# Deployment Runbook

This runbook covers the v1 production path: Scargo runs as a containerized web
app on a small always-on Linux/container host and connects to a
TimescaleDB-compatible managed database. `docs/deployment-options.md` currently
recommends Tiger Cloud Performance for the production database, with a
self-hosted TimescaleDB host only as the cost fallback.

## Production Configuration

Store real values in the host secret manager, platform environment settings, or
ignored `.env` files. Do not commit real database URLs, passwords, OAuth values,
token keys, or raw vehicle CSV/ZIP artifacts.

Required app settings:

| Variable | Production value |
| --- | --- |
| `SCARGO_ENV` | `production` |
| `SCARGO_DATABASE_URL` | PostgreSQL URL for a database with TimescaleDB support |
| `SCARGO_HTTP_HOST` | `0.0.0.0` inside a container or host service |
| `SCARGO_HTTP_PORT` | `8080` unless the host maps a different port |
| `RUST_LOG` | `info` by default |

Dropbox ingest is optional. When it is enabled, add:

| Variable | Production value |
| --- | --- |
| `SCARGO_DROPBOX_ENABLED` | `true` |
| `DROPBOX_APP_KEY` | Dropbox app key |
| `DROPBOX_APP_SECRET` | Dropbox app secret |
| `SCARGO_BASE_URL` | Public app origin, for example `https://scargo.example.com` |
| `SCARGO_DROPBOX_REDIRECT_URI` | Optional exact callback override |
| `SCARGO_TOKEN_ENCRYPTION_KEY` | 64 hex characters, exactly 32 decoded bytes |
| `SCARGO_DROPBOX_POLL_SEC` | Poll interval, default `300` |

If `SCARGO_DROPBOX_REDIRECT_URI` is unset, register this exact Dropbox callback:

```text
SCARGO_BASE_URL + /api/dropbox/oauth/callback
```

## Database Preparation

1. Provision a PostgreSQL service that supports `CREATE EXTENSION timescaledb`.
2. Create the production database and app user.
3. Confirm the app user can create the `timescaledb` extension, hypertables,
   compression policy, and continuous aggregate during startup bootstrap.
4. Record the provider backup policy, point-in-time recovery window, retention,
   and restore procedure.
5. Run a restore drill before taking user data.

For Tiger Cloud, use the connection string as `SCARGO_DATABASE_URL`. For a
self-hosted fallback, the operator owns backups, restore drills, monitoring, OS
patching, TimescaleDB upgrades, disk growth, and incident response.

## Build And Deploy

Build the release image from the repository root:

```bash
docker build -t scargo:<tag> .
```

Deploy that image with production environment values injected by the host. The
container must expose Scargo on `SCARGO_HTTP_HOST=0.0.0.0` and should publish
`SCARGO_HTTP_PORT=8080` through the host proxy or load balancer.

For a small VM-style host, the minimum shape is:

```bash
docker run -d --name scargo --restart unless-stopped \
  --env-file /etc/scargo/scargo.env \
  -p 127.0.0.1:8080:8080 \
  scargo:<tag>
```

Terminate TLS at the host proxy or platform edge, then forward to the container.
The app does not require persistent local storage. Dropbox CSV bytes are
downloaded only for ingestion and are discarded after Postgres writes complete.

## Verify

After each deploy, verify the app and database bootstrap:

```bash
curl -fsS https://scargo.example.com/api/health
```

Expected response:

```json
{"status":"ok"}
```

Then check logs for configuration, database, migration, and Dropbox worker
errors:

```bash
docker logs --tail 200 scargo
```

If Dropbox ingest is enabled, create or reconnect a non-guest account on
`/dropbox.html` and confirm `/api/dropbox/connection` returns the expected
connection state after OAuth.

## Rebuild, Restart, And Roll Back

Rebuild a new image for each deploy:

```bash
docker build -t scargo:<new-tag> .
docker stop scargo
docker rm scargo
docker run -d --name scargo --restart unless-stopped \
  --env-file /etc/scargo/scargo.env \
  -p 127.0.0.1:8080:8080 \
  scargo:<new-tag>
curl -fsS https://scargo.example.com/api/health
```

Keep the previous image tag until the new release is verified. To roll back,
restart the same environment with the prior image tag and rerun the health and
log checks. Do not roll back the database by restoring from backup unless data
corruption or an explicitly reviewed migration problem requires it.

## CI Gate

GitHub Actions runs Rust tests, formatting, Clippy, and a production image build.
The image-build job is intentionally only `docker build -t scargo:ci .`; it does
not publish images or require repository secrets.
