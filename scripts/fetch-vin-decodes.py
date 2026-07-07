#!/usr/bin/env python3
import argparse
import csv
import json
import os
import subprocess
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
from datetime import datetime, timezone
from pathlib import Path

VPIC_API_BASE = "https://vpic.nhtsa.dot.gov/api/vehicles/DecodeVinValuesExtended"
CACHE_COLUMNS = [
    "vin",
    "year",
    "make",
    "model",
    "powertrain",
    "displacement_l",
    "cylinders",
    "engine_configuration",
    "aspiration",
    "body_class",
    "trim",
    "lookup_status",
    "source",
    "decoded_at",
]


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
    parser = argparse.ArgumentParser(
        description="Fetch offline VIN decode cache rows from NHTSA vPIC."
    )
    parser.add_argument("--cache", type=Path, default=Path("vin-decodes.csv"))
    parser.add_argument("--vin", help="Fetch exactly one VIN")
    parser.add_argument("--missing-only", action="store_true")
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def parse_year(value):
    text = str(value or "").strip()
    if not text:
        return 0
    try:
        year = int(text)
    except ValueError:
        return 0
    return year if year > 0 else 0


def pick(row, *keys):
    for key in keys:
        value = str(row.get(key) or "").strip()
        if value:
            return value
    return ""


def normalize_powertrain(row):
    fields = " ".join(
        filter(
            None,
            [
                pick(row, "ElectrificationLevel"),
                pick(row, "EngineConfiguration"),
                pick(row, "FuelTypePrimary"),
                pick(row, "FuelTypeSecondary"),
            ],
        )
    ).lower()
    if "plug-in hybrid" in fields or "phev" in fields:
        return "PHEV"
    if "hybrid" in fields or "hev" in fields:
        return "Hybrid"
    if "electric" in fields or "bev" in fields or fields == "ev":
        return "EV"
    return ""


def normalize_aspiration(row):
    aspiration = pick(row, "AspirationType", "Turbo")
    lowered = aspiration.lower()
    if lowered in {"yes", "true", "1"}:
        return "turbo"
    if lowered in {"no", "false", "0"}:
        return ""
    return aspiration


def format_displacement(value):
    text = str(value or "").strip()
    if not text:
        return ""
    try:
        return f"{float(text):.1f}"
    except ValueError:
        return text


def normalize_cylinder_layout(value):
    text = str(value or "").strip().lower()
    if not text:
        return ""
    if "v-shaped" in text or text == "v":
        return "V"
    if "inline" in text or text in {"i", "in-line"}:
        return "I"
    if "flat" in text or "boxer" in text or text == "h":
        return "H"
    if "w" in text and ("shaped" in text or text == "w"):
        return "W"
    return ""


def normalize_engine_family(row):
    powertrain = (row.get("powertrain") or "").strip().lower()
    if powertrain == "ev":
        return "EV"
    if powertrain == "phev":
        return "PHEV"
    if powertrain == "hybrid":
        return "Hybrid"

    displacement = format_displacement(row.get("displacement_l"))
    cylinders = (row.get("cylinders") or "").strip()
    engine_configuration = normalize_cylinder_layout(row.get("engine_configuration"))
    aspiration = (row.get("aspiration") or "").strip().lower()
    if not displacement or not cylinders:
        return ""
    if aspiration in {"", "naturally aspirated", "na", "no", "false", "0"}:
        aspiration_label = "NA"
    elif aspiration in {"turbo", "turbocharged", "yes", "true", "1"}:
        aspiration_label = "Turbo"
    else:
        aspiration_label = aspiration
    cylinder_family = (
        f"{engine_configuration}{cylinders}" if engine_configuration else f"{cylinders}cyl"
    )
    return f"{displacement}L {cylinder_family} {aspiration_label}".strip()


def map_vpic_result(vin, requested_year, row):
    mapped = {
        "vin": vin,
        "year": pick(row, "ModelYear") or (str(requested_year) if requested_year else ""),
        "make": pick(row, "Make"),
        "model": pick(row, "Model", "ModelName"),
        "powertrain": normalize_powertrain(row),
        "displacement_l": format_displacement(pick(row, "DisplacementL")),
        "cylinders": pick(row, "EngineCylinders"),
        "engine_configuration": pick(row, "EngineConfiguration"),
        "aspiration": normalize_aspiration(row),
        "body_class": pick(row, "BodyClass"),
        "trim": pick(row, "Trim"),
        "lookup_status": "incomplete",
        "source": "vpic",
        "decoded_at": datetime.now(timezone.utc).replace(microsecond=0).isoformat(),
    }
    if mapped["make"] and mapped["model"] and normalize_engine_family(mapped):
        mapped["lookup_status"] = "ok"
    return mapped


def load_cached_vins(path):
    if not path.exists():
        return set()
    with path.open(newline="") as handle:
        return {
            (row.get("vin") or row.get("VIN") or "").strip().upper()
            for row in csv.DictReader(handle)
            if (row.get("vin") or row.get("VIN") or "").strip()
        }


def ensure_cache_file(path):
    path.parent.mkdir(parents=True, exist_ok=True)
    if path.exists() and path.stat().st_size > 0:
        return
    with path.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=CACHE_COLUMNS)
        writer.writeheader()


def append_cache_row(path, row):
    with path.open("a", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=CACHE_COLUMNS)
        writer.writerow(row)


def psql_lines(sql):
    proc = subprocess.Popen(
        ["psql", database_url(), "-X", "-A", "-F", "\t", "-q", "-t", "-c", sql],
        text=True,
        stdout=subprocess.PIPE,
    )
    assert proc.stdout is not None
    for line in proc.stdout:
        if line:
            yield line.rstrip("\n")
    if proc.wait() != 0:
        raise subprocess.CalledProcessError(proc.returncode, proc.args)


def fetch_missing_vehicles():
    sql = """
SELECT vin, year
FROM vehicle
WHERE model = '' OR engine_family = ''
ORDER BY vin
"""
    for line in psql_lines(sql):
        vin, year = line.split("\t")
        yield {"vin": vin.strip().upper(), "year": parse_year(year)}


def decode_vin(vin, year):
    params = {"format": "json"}
    if year > 0:
        params["modelyear"] = str(year)
    query = urllib.parse.urlencode(params)
    url = f"{VPIC_API_BASE}/{urllib.parse.quote(vin)}?{query}"
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "scargo-vin-fetch/1.0"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        payload = json.load(response)
    results = payload.get("Results") or []
    if not results:
        return map_vpic_result(vin, year, {})
    return map_vpic_result(vin, year, results[0])


def iter_targets(args):
    if args.vin:
        return [{"vin": args.vin.strip().upper(), "year": 0}]
    return list(fetch_missing_vehicles())


def self_test():
    row = map_vpic_result(
        "DEMOHONDAACCORD01",
        2003,
        {
            "ModelYear": "2003",
            "Make": "Honda",
            "Model": "Accord",
            "DisplacementL": "2.4",
            "EngineCylinders": "4",
            "Turbo": "No",
            "BodyClass": "Sedan/Saloon",
            "Trim": "EX",
        },
    )
    assert row["year"] == "2003"
    assert row["make"] == "Honda"
    assert row["model"] == "Accord"
    assert row["powertrain"] == ""
    assert row["displacement_l"] == "2.4"
    assert row["cylinders"] == "4"
    assert row["engine_configuration"] == ""
    assert row["aspiration"] == ""
    assert row["body_class"] == "Sedan/Saloon"
    assert row["trim"] == "EX"
    assert row["lookup_status"] == "ok"
    ev = map_vpic_result(
        "DEMOTESLAMODEL301",
        2019,
        {
            "ModelYear": "2019",
            "Make": "Tesla",
            "Model": "Model 3",
            "ElectrificationLevel": "Battery Electric Vehicle (BEV)",
        },
    )
    assert ev["powertrain"] == "EV"
    assert ev["lookup_status"] == "ok"
    assert (
        normalize_engine_family(
            {
                "displacement_l": "3.456",
                "cylinders": "6",
                "engine_configuration": "V-Shaped",
                "aspiration": "No",
            }
        )
        == "3.5L V6 NA"
    )
    assert (
        normalize_engine_family(
            {
                "displacement_l": "3.456",
                "cylinders": "6",
                "aspiration": "No",
            }
        )
        == "3.5L 6cyl NA"
    )


def main(argv=None):
    args = parse_args(argv)
    if args.self_test:
        self_test()
        return 0

    try:
        cached_vins = load_cached_vins(args.cache)
        targets = iter_targets(args)
        ensure_cache_file(args.cache)

        fetched = 0
        skipped = 0
        last_fetch_at = 0.0

        for target in targets:
            vin = target["vin"]
            year = target["year"]
            if not args.refresh and vin in cached_vins:
                skipped += 1
                continue

            now = time.monotonic()
            delay = 1.0 - (now - last_fetch_at)
            if fetched > 0 and delay > 0:
                time.sleep(delay)

            row = decode_vin(vin, year)
            append_cache_row(args.cache, row)
            cached_vins.add(vin)
            fetched += 1
            last_fetch_at = time.monotonic()
    except FileNotFoundError:
        print("psql is required unless --vin is used", file=sys.stderr)
        return 1
    except urllib.error.URLError as exc:
        print(f"vPIC lookup failed: {exc}", file=sys.stderr)
        return 1

    print(f"Fetched {fetched} VIN decodes, skipped {skipped} cached VINs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
