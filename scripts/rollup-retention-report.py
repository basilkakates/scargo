#!/usr/bin/env python3
import json
import os
import subprocess
import sys


def database_url():
    if os.environ.get("SCARGO_DATABASE_URL"):
        return os.environ["SCARGO_DATABASE_URL"]
    host = os.environ.get("POSTGRES_HOST", "127.0.0.1")
    port = os.environ.get("POSTGRES_PORT", "5432")
    user = os.environ.get("POSTGRES_USER", "scargo")
    password = os.environ.get("POSTGRES_PASSWORD", "")
    db = os.environ.get("POSTGRES_DB", "scargo")
    auth = f"{user}:{password}" if password else user
    return f"postgresql://{auth}@{host}:{port}/{db}"


QUERY = r"""
WITH raw AS (
    SELECT COUNT(*)::bigint AS rows,
           MIN(time) AS first_at,
           MAX(time) AS last_at,
           pg_total_relation_size('obd2_metric_reading')::bigint AS bytes
    FROM obd2_metric_reading
),
daily AS (
    SELECT COUNT(*)::bigint AS rows,
           MIN(bucket_day) AS first_at,
           MAX(bucket_day) AS last_at,
           pg_total_relation_size('vehicle_metric_day')::bigint AS bytes
    FROM vehicle_metric_day
),
coverage AS (
    SELECT COUNT(*)::bigint AS vehicles_total,
           COUNT(*) FILTER (WHERE model <> '' AND engine_family <> '')::bigint AS public_ready,
           COUNT(*) FILTER (WHERE model = '' OR engine_family = '')::bigint AS excluded_public
    FROM vehicle
)
SELECT json_build_object(
    'raw', row_to_json(raw),
    'daily', row_to_json(daily),
    'coverage', row_to_json(coverage),
    'retention_target', json_build_object(
        'raw', '180 days compressed',
        'daily', 'indefinite'
    )
)
FROM raw, daily, coverage;
"""


def main():
    try:
        out = subprocess.check_output(
            ["psql", database_url(), "-X", "-A", "-q", "-t", "-c", QUERY],
            text=True,
        ).strip()
    except FileNotFoundError:
        print("psql is required for the retention report", file=sys.stderr)
        return 1
    print(json.dumps(json.loads(out), indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
