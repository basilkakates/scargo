#!/usr/bin/env python3
import argparse
import csv
import json
import math
import os
import subprocess
import sys
from collections import defaultdict
from datetime import datetime, timedelta, timezone
from pathlib import Path


DAILY_QUERY = r"""
SELECT d.vehicle_id::text,
       d.bucket_day::text,
       m.key,
       (d.value_sum / d.reading_count::double precision)::text
FROM vehicle_metric_day d
JOIN obd2_metric m ON m.id = d.metric_id
WHERE d.reading_count > 0
ORDER BY d.vehicle_id, d.bucket_day, m.key;
"""

RAW_QUERY = r"""
SELECT r.vehicle_id::text, r.time::text, m.key, r.value::text
FROM obd2_metric_reading r
JOIN obd2_metric m ON m.id = r.metric_id
WHERE r.value IS NOT NULL
ORDER BY r.vehicle_id, r.time, m.key;
"""

DAILY_RELATIONSHIP_QUERY = r"""
SELECT x.key, y.key, COUNT(*)::text, CORR(x.value, y.value)::text,
       AVG(x.value)::text, AVG(y.value)::text,
       MIN(x.value)::text, MAX(x.value)::text,
       MIN(y.value)::text, MAX(y.value)::text
FROM (
    SELECT d.vehicle_id, d.bucket_day AS sample_at, m.key,
           d.value_sum / d.reading_count::double precision AS value
    FROM vehicle_metric_day d
    JOIN obd2_metric m ON m.id = d.metric_id
    WHERE d.reading_count > 0
) x
JOIN (
    SELECT d.vehicle_id, d.bucket_day AS sample_at, m.key,
           d.value_sum / d.reading_count::double precision AS value
    FROM vehicle_metric_day d
    JOIN obd2_metric m ON m.id = d.metric_id
    WHERE d.reading_count > 0
) y ON y.vehicle_id = x.vehicle_id AND y.sample_at = x.sample_at AND y.key > x.key
GROUP BY x.key, y.key
ORDER BY ABS(CORR(x.value, y.value)) DESC NULLS LAST, COUNT(*) DESC, x.key, y.key;
"""

RAW_RELATIONSHIP_QUERY = r"""
SELECT x.key, y.key, COUNT(*)::text, CORR(x.value, y.value)::text,
       AVG(x.value)::text, AVG(y.value)::text,
       MIN(x.value)::text, MAX(x.value)::text,
       MIN(y.value)::text, MAX(y.value)::text
FROM (
    SELECT r.vehicle_id, r.time, m.key, r.value
    FROM obd2_metric_reading r
    JOIN obd2_metric m ON m.id = r.metric_id
    WHERE r.value IS NOT NULL
) x
JOIN (
    SELECT r.vehicle_id, r.time, m.key, r.value
    FROM obd2_metric_reading r
    JOIN obd2_metric m ON m.id = r.metric_id
    WHERE r.value IS NOT NULL
) y ON y.vehicle_id = x.vehicle_id AND y.time = x.time AND y.key > x.key
GROUP BY x.key, y.key
ORDER BY ABS(CORR(x.value, y.value)) DESC NULLS LAST, COUNT(*) DESC, x.key, y.key;
"""


def vin_query(vin):
    escaped = vin.replace("'", "''")
    return f"""
SELECT r.vehicle_id::text, r.time::text, m.key, r.value::text
FROM obd2_metric_reading r
JOIN obd2_metric m ON m.id = r.metric_id
JOIN vehicle v ON v.id = r.vehicle_id
WHERE r.value IS NOT NULL
  AND v.vin = '{escaped}'
ORDER BY r.vehicle_id, r.time, m.key;
"""


def vin_relationship_query(vin):
    escaped = vin.replace("'", "''")
    return f"""
SELECT x.key, y.key, COUNT(*)::text, CORR(x.value, y.value)::text,
       AVG(x.value)::text, AVG(y.value)::text,
       MIN(x.value)::text, MAX(x.value)::text,
       MIN(y.value)::text, MAX(y.value)::text
FROM (
    SELECT r.vehicle_id, r.time, m.key, r.value
    FROM obd2_metric_reading r
    JOIN obd2_metric m ON m.id = r.metric_id
    JOIN vehicle v ON v.id = r.vehicle_id
    WHERE r.value IS NOT NULL AND v.vin = '{escaped}'
) x
JOIN (
    SELECT r.vehicle_id, r.time, m.key, r.value
    FROM obd2_metric_reading r
    JOIN obd2_metric m ON m.id = r.metric_id
    JOIN vehicle v ON v.id = r.vehicle_id
    WHERE r.value IS NOT NULL AND v.vin = '{escaped}'
) y ON y.vehicle_id = x.vehicle_id AND y.time = x.time AND y.key > x.key
GROUP BY x.key, y.key
ORDER BY ABS(CORR(x.value, y.value)) DESC NULLS LAST, COUNT(*) DESC, x.key, y.key;
"""


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


def parse_args(argv=None):
    parser = argparse.ArgumentParser(description="Analyze stored Scargo telemetry.")
    parser.add_argument(
        "--events",
        type=Path,
        help="CSV with label,date and optional vehicle_id,before_days,after_days,uncertainty_days",
    )
    parser.add_argument("--vin", help="Limit analysis to one VIN and write reconstruction outputs")
    parser.add_argument("--reconstruct-threshold", type=float, default=0.98)
    parser.add_argument("--raw-relationships", action="store_true", help="Use Python row-pair fallback")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def psql_lines(query):
    proc = subprocess.Popen(
        ["psql", database_url(), "-X", "-A", "-F", "\t", "-q", "-t", "-c", query],
        text=True,
        stdout=subprocess.PIPE,
    )
    assert proc.stdout is not None
    for line in proc.stdout:
        if not line:
            continue
        yield line.rstrip("\n")
    if proc.wait() != 0:
        raise subprocess.CalledProcessError(proc.returncode, proc.args)


def psql_rows(vin=None, source="daily"):
    if vin:
        query = vin_query(vin)
    elif source == "raw":
        query = RAW_QUERY
    else:
        query = DAILY_QUERY
    for line in psql_lines(query):
        vehicle_id, at, key, value = line.split("\t")
        yield vehicle_id, at, key, float(value)


def pearson(xs, ys):
    count = len(xs)
    if count < 2:
        return None
    avg_x = sum(xs) / count
    avg_y = sum(ys) / count
    cov = sum((x - avg_x) * (y - avg_y) for x, y in zip(xs, ys))
    var_x = sum((x - avg_x) ** 2 for x in xs)
    var_y = sum((y - avg_y) ** 2 for y in ys)
    if not var_x or not var_y:
        return None
    return cov / math.sqrt(var_x * var_y)


def analyze_relationships(rows):
    by_sample = defaultdict(dict)
    for vehicle_id, at, key, value in rows:
        by_sample[(vehicle_id, at)][key] = value

    pairs = defaultdict(lambda: [[], []])
    for metrics in by_sample.values():
        keys = sorted(metrics)
        for i, x_key in enumerate(keys):
            for y_key in keys[i + 1 :]:
                xs, ys = pairs[(x_key, y_key)]
                xs.append(metrics[x_key])
                ys.append(metrics[y_key])

    rows = []
    for (x_key, y_key), (xs, ys) in pairs.items():
        rows.append(
            {
                "x": x_key,
                "y": y_key,
                "count": len(xs),
                "correlation": pearson(xs, ys),
                "x_avg": sum(xs) / len(xs),
                "y_avg": sum(ys) / len(ys),
                "x_min": min(xs),
                "x_max": max(xs),
                "y_min": min(ys),
                "y_max": max(ys),
            }
        )

    rows.sort(
        key=lambda row: (
            row["correlation"] is None,
            -abs(row["correlation"] or 0),
            -row["count"],
            row["x"],
            row["y"],
        )
    )
    return rows


def maybe_float(value):
    return None if value == "" else float(value)


def analyze_relationships_sql(vin=None, source="daily"):
    if vin:
        query = vin_relationship_query(vin)
    elif source == "raw":
        query = RAW_RELATIONSHIP_QUERY
    else:
        query = DAILY_RELATIONSHIP_QUERY
    out = []
    for line in psql_lines(query):
        x, y, count, corr, x_avg, y_avg, x_min, x_max, y_min, y_max = line.split("\t")
        out.append(
            {
                "x": x,
                "y": y,
                "count": int(count),
                "correlation": maybe_float(corr),
                "x_avg": float(x_avg),
                "y_avg": float(y_avg),
                "x_min": float(x_min),
                "x_max": float(x_max),
                "y_min": float(y_min),
                "y_max": float(y_max),
            }
        )
    return out


def sample_maps(rows):
    by_sample = defaultdict(dict)
    for vehicle_id, at, key, value in rows:
        by_sample[(vehicle_id, at)][key] = value
    return by_sample.values()


def metric_correlation(samples, x_key, y_key):
    pairs = [(sample[x_key], sample[y_key]) for sample in samples if x_key in sample and y_key in sample]
    if len(pairs) < 2:
        return None, len(pairs)
    xs = [x for x, _y in pairs]
    ys = [y for _x, y in pairs]
    return pearson(xs, ys), len(pairs)


def analyze_reconstruction(rows, threshold):
    samples = list(sample_maps(rows))
    keys = sorted({key for _vehicle_id, _at, key, _value in rows})
    uncovered = set(keys)
    selected = []
    targets = {}

    while uncovered:
        best = None
        for candidate in keys:
            if candidate in selected:
                continue
            coverage = {candidate} & uncovered
            candidate_targets = {candidate: (1.0, None)}
            for target in uncovered - {candidate}:
                corr, count = metric_correlation(samples, candidate, target)
                if corr is not None and abs(corr) >= threshold:
                    coverage.add(target)
                    candidate_targets[target] = (corr, count)
            score = (len(coverage), sum(abs(candidate_targets[k][0]) for k in coverage), candidate)
            if best is None or score > best[0]:
                best = (score, candidate, coverage, candidate_targets)

        _score, candidate, coverage, candidate_targets = best
        selected.append(candidate)
        for target in coverage:
            corr, count = candidate_targets[target]
            targets[target] = {
                "key": target,
                "source_key": candidate,
                "correlation": corr,
                "paired_count": count,
                "selected_directly": target == candidate,
            }
        uncovered -= coverage

    return {
        "threshold": threshold,
        "total_keys": len(keys),
        "selected_count": len(selected),
        "selected_keys": selected,
        "targets": [targets[key] for key in sorted(targets)],
    }


def analyze_reconstruction_from_relationships(relationships, threshold):
    keys = sorted({row["x"] for row in relationships} | {row["y"] for row in relationships})
    uncovered = set(keys)
    selected = []
    targets = {}
    related = defaultdict(dict)
    for row in relationships:
        related[row["x"]][row["y"]] = row
        related[row["y"]][row["x"]] = {
            **row,
            "x": row["y"],
            "y": row["x"],
        }

    while uncovered:
        best = None
        for candidate in keys:
            if candidate in selected:
                continue
            coverage = {candidate} & uncovered
            for target, row in related[candidate].items():
                corr = row["correlation"]
                if target in uncovered and corr is not None and abs(corr) >= threshold:
                    coverage.add(target)
            score = (
                len(coverage),
                sum(abs(related[candidate][k]["correlation"]) for k in coverage if k != candidate),
                candidate,
            )
            if best is None or score > best[0]:
                best = (score, candidate, coverage)

        _score, candidate, coverage = best
        selected.append(candidate)
        for target in coverage:
            row = related[candidate].get(target)
            targets[target] = {
                "key": target,
                "source_key": candidate,
                "correlation": 1.0 if target == candidate else row["correlation"],
                "paired_count": None if target == candidate else row["count"],
                "selected_directly": target == candidate,
            }
        uncovered -= coverage

    return {
        "threshold": threshold,
        "total_keys": len(keys),
        "selected_count": len(selected),
        "selected_keys": selected,
        "targets": [targets[key] for key in sorted(targets)],
    }


def parse_time(value):
    parsed = datetime.fromisoformat(value)
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=timezone.utc)
    return parsed.astimezone(timezone.utc)


def parse_event_date(value):
    if "T" in value or " " in value:
        return parse_time(value)
    return datetime.fromisoformat(value).replace(tzinfo=timezone.utc)


def load_events(path):
    with path.open(newline="") as f:
        events = []
        for row in csv.DictReader(f):
            events.append(
                {
                    "label": row.get("label") or row.get("event") or "event",
                    "vehicle_id": row.get("vehicle_id") or "",
                    "date": parse_event_date(row["date"]),
                    "before_days": int(row.get("before_days") or 30),
                    "after_days": int(row.get("after_days") or 14),
                    "uncertainty_days": int(row.get("uncertainty_days") or 0),
                }
            )
    return events


def slope(points):
    if len(points) < 2:
        return None
    start = points[0][0]
    xs = [(at - start).total_seconds() / 86400 for at, _value in points]
    ys = [value for _at, value in points]
    avg_x = sum(xs) / len(xs)
    avg_y = sum(ys) / len(ys)
    var_x = sum((x - avg_x) ** 2 for x in xs)
    if not var_x:
        return None
    return sum((x - avg_x) * (y - avg_y) for x, y in zip(xs, ys)) / var_x


def avg(points):
    return sum(value for _at, value in points) / len(points) if points else None


def analyze_events(rows, events):
    by_metric = defaultdict(list)
    for vehicle_id, at, key, value in rows:
        by_metric[(vehicle_id, key)].append((parse_time(at), value))

    out = []
    for points in by_metric.values():
        points.sort()

    for event in events:
        before_start = event["date"] - timedelta(days=event["before_days"])
        before_end = event["date"] - timedelta(days=event["uncertainty_days"])
        after_start = event["date"] + timedelta(days=event["uncertainty_days"])
        after_end = event["date"] + timedelta(days=event["after_days"])

        for (vehicle_id, key), points in by_metric.items():
            if event["vehicle_id"] and event["vehicle_id"] != vehicle_id:
                continue
            before = [(at, value) for at, value in points if before_start <= at < before_end]
            after = [(at, value) for at, value in points if after_start <= at <= after_end]
            if len(before) < 2 or not after:
                continue
            before_avg = avg(before)
            after_avg = avg(after)
            out.append(
                {
                    "event": event["label"],
                    "event_date": event["date"].date().isoformat(),
                    "vehicle_id": vehicle_id,
                    "metric": key,
                    "before_count": len(before),
                    "after_count": len(after),
                    "before_avg": before_avg,
                    "after_avg": after_avg,
                    "before_slope_per_day": slope(before),
                    "change_after_event": after_avg - before_avg,
                    "percent_change_after_event": None
                    if not before_avg
                    else ((after_avg - before_avg) / abs(before_avg)) * 100,
                }
            )

    out.sort(
        key=lambda row: (
            -abs(row["percent_change_after_event"] or 0),
            row["event"],
            row["metric"],
        )
    )
    return out


def write_rows(path, rows, fieldnames):
    with path.open("w", newline="") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def write_outputs(rows, event_rows, reconstruction):
    out_dir = Path("analysis")
    out_dir.mkdir(exist_ok=True)
    json_path = out_dir / "telemetry-relationships.json"
    csv_path = out_dir / "telemetry-relationships.csv"

    json_path.write_text(json.dumps(rows, indent=2) + "\n")
    write_rows(
        csv_path,
        rows,
        ["x", "y", "count", "correlation", "x_avg", "y_avg", "x_min", "x_max", "y_min", "y_max"],
    )
    reconstruction_json = reconstruction_csv = None
    if reconstruction:
        reconstruction_json = out_dir / "telemetry-reconstruction.json"
        reconstruction_csv = out_dir / "telemetry-reconstruction.csv"
        reconstruction_json.write_text(json.dumps(reconstruction, indent=2) + "\n")
        write_rows(
            reconstruction_csv,
            reconstruction["targets"],
            ["key", "source_key", "correlation", "paired_count", "selected_directly"],
        )

    if event_rows is None:
        return json_path, csv_path, None, None, reconstruction_json, reconstruction_csv

    events_json = out_dir / "telemetry-events.json"
    events_csv = out_dir / "telemetry-events.csv"
    events_json.write_text(json.dumps(event_rows, indent=2) + "\n")
    write_rows(
        events_csv,
        event_rows,
        [
            "event",
            "event_date",
            "vehicle_id",
            "metric",
            "before_count",
            "after_count",
            "before_avg",
            "after_avg",
            "before_slope_per_day",
            "change_after_event",
            "percent_change_after_event",
        ],
    )
    return json_path, csv_path, events_json, events_csv, reconstruction_json, reconstruction_csv


def self_test():
    rows = [
        ("v", "2026-01-01 00:00:00+00", "fuel_trim", 1.0),
        ("v", "2026-01-02 00:00:00+00", "fuel_trim", 2.0),
        ("v", "2026-01-10 00:00:00+00", "fuel_trim", 0.5),
    ]
    events = [
        {
            "label": "service",
            "vehicle_id": "v",
            "date": parse_event_date("2026-01-03"),
            "before_days": 3,
            "after_days": 10,
            "uncertainty_days": 0,
        }
    ]
    result = analyze_events(rows, events)[0]
    assert result["before_slope_per_day"] == 1.0
    assert result["change_after_event"] == -1.0
    reconstruction = analyze_reconstruction(
        [
            ("v", "2026-01-01 00:00:00+00", "a", 1.0),
            ("v", "2026-01-01 00:00:00+00", "b", 2.0),
            ("v", "2026-01-01 00:00:00+00", "c", 9.0),
            ("v", "2026-01-02 00:00:00+00", "a", 2.0),
            ("v", "2026-01-02 00:00:00+00", "b", 4.0),
            ("v", "2026-01-02 00:00:00+00", "c", 8.0),
            ("v", "2026-01-03 00:00:00+00", "a", 3.0),
            ("v", "2026-01-03 00:00:00+00", "b", 6.0),
            ("v", "2026-01-03 00:00:00+00", "c", 9.0),
        ],
        0.98,
    )
    assert reconstruction["selected_count"] == 2
    reconstruction = analyze_reconstruction_from_relationships(
        [
            {"x": "a", "y": "b", "count": 3, "correlation": 1.0},
            {"x": "a", "y": "c", "count": 3, "correlation": 0.0},
            {"x": "b", "y": "c", "count": 3, "correlation": 0.0},
        ],
        0.98,
    )
    assert reconstruction["selected_count"] == 2


def main(argv=None):
    args = parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    source = "raw" if args.raw_relationships or args.vin else "daily"
    try:
        relationships = (
            analyze_relationships(list(psql_rows(args.vin, source="raw")))
            if args.raw_relationships
            else analyze_relationships_sql(args.vin, source=source)
        )
        event_rows = (
            analyze_events(list(psql_rows(args.vin, source=source)), load_events(args.events))
            if args.events
            else None
        )
    except FileNotFoundError:
        print("psql is required for telemetry analysis", file=sys.stderr)
        return 1
    reconstruction = (
        analyze_reconstruction_from_relationships(relationships, args.reconstruct_threshold)
        if args.vin
        else None
    )
    json_path, csv_path, events_json, events_csv, reconstruction_json, reconstruction_csv = write_outputs(
        relationships, event_rows, reconstruction
    )
    print(
        f"Wrote {len(relationships)} metric relationships from {source} data to "
        f"{json_path} and {csv_path}"
    )
    if event_rows is not None:
        print(f"Wrote {len(event_rows)} event effects to {events_json} and {events_csv}")
    if reconstruction is not None:
        print(
            f"Wrote {reconstruction['selected_count']} selected keys for {reconstruction['total_keys']} "
            f"total keys to {reconstruction_json} and {reconstruction_csv}"
        )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
