#!/usr/bin/env bash
set -euo pipefail

set -a
[[ ! -f .env ]] || . ./.env
[[ ! -f .env.smoke ]] || . ./.env.smoke
set +a

export POSTGRES_PASSWORD="${POSTGRES_PASSWORD:-scargo}"

docker compose up -d scargo_db
ready=0
for _ in {1..60}; do
    if docker compose exec -T scargo_db pg_isready \
        -U "${POSTGRES_USER:-scargo}" \
        -d "${POSTGRES_DB:-scargo}" >/dev/null; then
        ready=1
        break
    fi
    sleep 1
done
[[ "$ready" == 1 ]] || { echo "scargo_db did not become ready" >&2; exit 1; }
cargo test --test smoke_stack -- --ignored --nocapture
