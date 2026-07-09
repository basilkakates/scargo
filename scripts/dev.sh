#!/usr/bin/env bash
set -euo pipefail

set -a
[[ ! -f .env ]] || . ./.env
set +a

if [[ "${SCARGO_ENV:-dev}" == [Pp][Rr][Oo][Dd][Uu][Cc][Tt][Ii][Oo][Nn] ]]; then
    echo "scripts/dev.sh refuses SCARGO_ENV=production" >&2
    exit 1
fi

unset SCARGO_DATABASE_URL
export POSTGRES_HOST="127.0.0.1"
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

exec cargo run
