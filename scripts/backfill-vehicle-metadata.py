#!/usr/bin/env python3
import argparse
import csv
import os
import subprocess
import sys
from pathlib import Path


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
        description="Backfill vehicle year/make/model/engine_family."
    )
    parser.add_argument("--decode-csv", type=Path)
    parser.add_argument("--overrides", type=Path, help="CSV keyed by vin for corrections")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--self-test", action="store_true")
    return parser.parse_args(argv)


def load_csv(path):
    with path.open(newline="") as handle:
        rows = {}
        for row in csv.DictReader(handle):
            vin = (row.get("vin") or row.get("VIN") or "").strip().upper()
            if vin:
                rows[vin] = row
        return rows


def pick(row, *keys):
    for key in keys:
        value = (row.get(key) or "").strip()
        if value:
            return value
    return ""


def parse_year(value):
    text = str(value or "").strip()
    if not text:
        return 0
    try:
        year = int(text)
    except ValueError:
        return 0
    return year if year > 0 else 0


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
    powertrain = (row.get("powertrain") or row.get("PowertrainType") or "").strip().lower()
    if powertrain == "ev":
        return "EV"
    if powertrain == "phev":
        return "PHEV"
    if powertrain == "hybrid":
        return "Hybrid"

    displacement = format_displacement(
        row.get("displacement_l") or row.get("DisplacementL") or row.get("displacement") or ""
    )
    cylinders = (row.get("cylinders") or row.get("EngineCylinders") or "").strip()
    engine_configuration = normalize_cylinder_layout(
        row.get("engine_configuration") or row.get("EngineConfiguration") or ""
    )
    aspiration = (
        row.get("aspiration")
        or row.get("AspirationType")
        or row.get("turbo")
        or ""
    ).strip()
    if not displacement or not cylinders:
        return ""

    lowered = aspiration.lower()
    if lowered in {"", "naturally aspirated", "na", "no", "false", "0"}:
        aspiration_label = "NA"
    elif lowered in {"turbo", "turbocharged", "yes", "true", "1"}:
        aspiration_label = "Turbo"
    else:
        aspiration_label = aspiration
    cylinder_family = (
        f"{engine_configuration}{cylinders}" if engine_configuration else f"{cylinders}cyl"
    )
    return f"{displacement}L {cylinder_family} {aspiration_label}".strip()


def metadata_from_row(row):
    return {
        "year": parse_year(pick(row, "year", "Year", "ModelYear")),
        "make": pick(row, "make", "Make"),
        "model": pick(row, "model", "Model", "ModelName"),
        "engine_family": normalize_engine_family(row),
    }


def merge_metadata(vehicle, *rows):
    merged = {
        "year": vehicle["year"],
        "make": vehicle["make"],
        "model": vehicle["model"],
        "engine_family": vehicle["engine_family"],
    }
    for row in rows:
        if not row:
            continue
        for key, value in metadata_from_row(row).items():
            if value:
                merged[key] = value
    return merged


def needs_public_metadata(metadata):
    return not metadata["model"] or not metadata["engine_family"]


def pattern_key(vin, year):
    vin = (vin or "").strip().upper()
    year = parse_year(year)
    if len(vin) != 17 or year <= 0:
        return ""
    return f"{vin[:8]}:{year}"


def build_inference_index(decoded, overrides):
    outcomes = {}
    for vin in sorted(set(decoded) | set(overrides)):
        metadata = merge_metadata(
            {"year": 0, "make": "", "model": "", "engine_family": ""},
            decoded.get(vin),
            overrides.get(vin),
        )
        if needs_public_metadata(metadata):
            continue
        key = pattern_key(
            vin,
            pick(overrides.get(vin) or {}, "year", "Year", "ModelYear")
            or pick(decoded.get(vin) or {}, "year", "Year", "ModelYear"),
        )
        if not key:
            continue
        outcomes.setdefault(key, set()).add(
            (metadata["make"], metadata["model"], metadata["engine_family"])
        )

    unique = {}
    ambiguous = set()
    for key, values in outcomes.items():
        if len(values) == 1:
            unique[key] = next(iter(values))
        else:
            ambiguous.add(key)
    return unique, ambiguous


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


def fetch_vehicles():
    sql = "SELECT id::text, vin, year, make, model, engine_family FROM vehicle ORDER BY vin"
    for line in psql_lines(sql):
        vehicle_id, vin, year, make, model, engine_family = line.split("\t")
        yield {
            "id": vehicle_id,
            "vin": vin.strip().upper(),
            "year": parse_year(year),
            "make": make,
            "model": model,
            "engine_family": engine_family,
        }


def escape_sql(value):
    return value.replace("'", "''")


def update_vehicle(vehicle_id, year, make, model, engine_family):
    sql = f"""
UPDATE vehicle
SET year = {year},
    make = '{escape_sql(make)}',
    model = '{escape_sql(model)}',
    engine_family = '{escape_sql(engine_family)}',
    updated_at = NOW()
WHERE id = '{vehicle_id}'::uuid;
"""
    subprocess.run(
        ["psql", database_url(), "-X", "-v", "ON_ERROR_STOP=1", "-c", sql],
        check=True,
    )


def self_test():
    assert normalize_engine_family({"powertrain": "ev"}) == "EV"
    assert normalize_engine_family({"powertrain": "hybrid"}) == "Hybrid"
    assert normalize_engine_family({"powertrain": "phev"}) == "PHEV"
    assert normalize_engine_family({"displacement_l": "2.4", "cylinders": "4"}) == "2.4L 4cyl NA"
    assert (
        normalize_engine_family(
            {
                "displacement_l": "1.5",
                "cylinders": "4",
                "engine_configuration": "Inline",
                "aspiration": "turbo",
            }
        )
        == "1.5L I4 Turbo"
    )
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
            {"displacement_l": "3.456", "cylinders": "6", "aspiration": "No"}
        )
        == "3.5L 6cyl NA"
    )
    assert pattern_key("DEMOHONDAACCORD01", 2003) == "DEMOHOND:2003"
    assert pattern_key("short", 2003) == ""
    unique, ambiguous = build_inference_index(
        {
            "DEMOHONDAACCORD01": {
                "vin": "DEMOHONDAACCORD01",
                "year": "2003",
                "make": "Honda",
                "model": "Accord",
                "displacement_l": "2.4",
                "cylinders": "4",
            }
        },
        {},
    )
    assert unique["DEMOHOND:2003"] == ("Honda", "Accord", "2.4L 4cyl NA")
    assert not ambiguous
    unique, ambiguous = build_inference_index(
        {
            "DEMOHONDAACCORD01": {
                "vin": "DEMOHONDAACCORD01",
                "year": "2003",
                "make": "Honda",
                "model": "Accord",
                "displacement_l": "2.4",
                "cylinders": "4",
            },
            "DEMOHONDAACCORD02": {
                "vin": "DEMOHONDAACCORD02",
                "year": "2003",
                "make": "Honda",
                "model": "Accord LX",
                "displacement_l": "2.4",
                "cylinders": "4",
            },
        },
        {},
    )
    assert "DEMOHOND:2003" in ambiguous
    assert "DEMOHOND:2003" not in unique


def main(argv=None):
    args = parse_args(argv)
    if args.self_test:
        self_test()
        return 0
    if not args.decode_csv:
        print("--decode-csv is required", file=sys.stderr)
        return 2

    try:
        decoded = load_csv(args.decode_csv)
        overrides = load_csv(args.overrides) if args.overrides else {}
        inferred, ambiguous = build_inference_index(decoded, overrides)

        direct_hits = 0
        inferred_hits = 0
        ambiguous_skips = 0
        unresolved_skips = 0
        prepared = 0

        for vehicle in fetch_vehicles():
            needs_metadata = needs_public_metadata(vehicle)
            direct_metadata = merge_metadata(
                vehicle,
                decoded.get(vehicle["vin"]),
                overrides.get(vehicle["vin"]),
            )
            source = ""
            final_metadata = direct_metadata

            if needs_metadata and not needs_public_metadata(direct_metadata):
                direct_hits += 1
                source = "direct"
            elif needs_metadata:
                key = pattern_key(vehicle["vin"], vehicle["year"])
                if key in ambiguous:
                    ambiguous_skips += 1
                elif key in inferred:
                    make, model, engine_family = inferred[key]
                    final_metadata = {
                        "year": vehicle["year"],
                        "make": make or vehicle["make"],
                        "model": model or vehicle["model"],
                        "engine_family": engine_family or vehicle["engine_family"],
                    }
                    if not needs_public_metadata(final_metadata):
                        inferred_hits += 1
                        source = "inferred"
                    else:
                        unresolved_skips += 1
                else:
                    unresolved_skips += 1

            changed = any(
                final_metadata[key] != vehicle[key]
                for key in ("year", "make", "model", "engine_family")
            )
            if changed and not needs_public_metadata(final_metadata):
                prepared += 1
                if not args.dry_run:
                    update_vehicle(
                        vehicle["id"],
                        final_metadata["year"],
                        final_metadata["make"],
                        final_metadata["model"],
                        final_metadata["engine_family"],
                    )
        print(
            "Prepared "
            f"{prepared} vehicle metadata updates "
            f"(direct={direct_hits}, inferred={inferred_hits}, "
            f"ambiguous={ambiguous_skips}, unresolved={unresolved_skips})"
        )
    except FileNotFoundError:
        print("psql is required for vehicle metadata backfill", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
