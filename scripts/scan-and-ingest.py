#!/usr/bin/env python3
"""Poll a vehicle-key drop folder and ingest CSV exports.

Folder shape:
  <drop-root>/<vehicle-key>/*.csv

Uploaded file hashes are recorded in <drop-root>/.scargo-ingest-state.json.
"""

import argparse
import hashlib
import json
import os
import sys
import time
import threading
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
from pathlib import Path
from urllib import parse, request
from urllib.error import HTTPError


DEFAULT_INGEST_PATH = os.environ.get("SCARGO_INGEST_PATH", "/opt/scargo-drop")
DEFAULT_API = os.environ.get("SCARGO_API", "http://localhost:8080/api/ingest/csv")
DEFAULT_API_TOKEN = os.environ.get("SCARGO_API_TOKEN", "")
DEFAULT_USER_KEY = os.environ.get("SCARGO_USER_KEY", "")
DEFAULT_POLL_SEC = float(os.environ.get("SCARGO_POLL_SEC", "1.0"))
DEFAULT_STABLE_SEC = float(os.environ.get("SCARGO_STABLE_SEC", "0.5"))
DEFAULT_WORKERS = int(os.environ.get("SCARGO_INGEST_WORKERS", "4"))
DEFAULT_TIMEOUT_SEC = float(os.environ.get("SCARGO_UPLOAD_TIMEOUT_SEC", "30"))
DEFAULT_STATE_SAVE_EVERY = int(os.environ.get("SCARGO_STATE_SAVE_EVERY", "100"))
STATE_VERSION = 1


def parse_args(argv=None):
    ap = argparse.ArgumentParser(description="Poll for vehicle-key CSV drops and ingest them.")
    ap.add_argument("ingest_path", nargs="?", type=Path, default=Path(DEFAULT_INGEST_PATH))
    ap.add_argument("--api", type=str, default=DEFAULT_API)
    ap.add_argument("--api-token", type=str, default=DEFAULT_API_TOKEN)
    ap.add_argument("--user-key", type=str, default=DEFAULT_USER_KEY)
    ap.add_argument("--poll-sec", type=float, default=DEFAULT_POLL_SEC)
    ap.add_argument("--stable-sec", type=float, default=DEFAULT_STABLE_SEC)
    ap.add_argument("--workers", type=int, default=DEFAULT_WORKERS)
    ap.add_argument("--timeout-sec", type=float, default=DEFAULT_TIMEOUT_SEC)
    ap.add_argument("--state-save-every", type=int, default=DEFAULT_STATE_SAVE_EVERY)
    ap.add_argument("--state-file", type=Path, default=None)
    ap.add_argument(
        "--reset-state",
        action="store_true",
        help="Forget prior watcher uploads before scanning; useful after truncating the database",
    )
    ap.add_argument("--once", action="store_true", help="Run one scan and exit")
    ap.add_argument("--print-updates", action="store_true", help="Log skipped duplicates too")
    return ap.parse_args(argv)


def now_iso() -> str:
    return datetime.now(timezone.utc).replace(microsecond=0).isoformat()


def load_state(path: Path) -> dict:
    if not path.exists():
        return {"version": STATE_VERSION, "uploaded": {}, "paths": {}}
    with path.open("r", encoding="utf-8") as f:
        state = json.load(f)
    state.setdefault("version", STATE_VERSION)
    state.setdefault("uploaded", {})
    state.setdefault("paths", {})
    return state


def save_state(path: Path, state: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with tmp.open("w", encoding="utf-8") as f:
        json.dump(state, f, indent=2, sort_keys=True)
        f.write("\n")
    tmp.replace(path)


def vehicle_key_from_rel(rel: Path) -> str:
    if len(rel.parts) < 2:
        raise ValueError(f"Cannot derive vehicle key from path: {rel}")
    return rel.parts[0]


def read_with_digest(path: Path) -> tuple[bytes, str]:
    h = hashlib.sha256()
    chunks = []
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
            chunks.append(chunk)
    return b"".join(chunks), h.hexdigest()


def post_csv(
    api: str,
    body: bytes,
    vehicle_key: str,
    api_token: str,
    user_key: str,
    timeout_sec: float,
) -> dict:
    separator = "&" if "?" in api else "?"
    url = f"{api}{separator}{parse.urlencode({'vin': vehicle_key})}"
    req = request.Request(url, data=body, method="POST")
    req.add_header("Content-Type", "text/csv")
    if api_token:
        req.add_header("Authorization", f"Bearer {api_token}")
    elif user_key:
        req.add_header("X-Scargo-User-Key", user_key)
    with request.urlopen(req, timeout=timeout_sec) as resp:
        payload = resp.read().decode("utf-8", "ignore")
    try:
        return json.loads(payload)
    except json.JSONDecodeError:
        return {"raw": payload}


def candidate_files(ingest_root: Path, state_file: Path):
    for csv_path in ingest_root.rglob("*.csv"):
        rel = csv_path.relative_to(ingest_root)
        if ".processed" in rel.parts:
            continue
        if csv_path == state_file:
            continue
        if len(rel.parts) < 2:
            continue
        yield csv_path, rel


def is_stable(path: Path, stable_sec: float) -> bool:
    try:
        stat = path.stat()
    except FileNotFoundError:
        return False
    now = time.time()
    changed_at = stat.st_mtime
    if changed_at > now:
        changed_at = stat.st_ctime
    return now - changed_at >= stable_sec


def ingest_candidate(
    args,
    state: dict,
    stop_event: threading.Event,
    csv_path: Path,
    rel: Path,
):
    if stop_event.is_set():
        return None
    if not is_stable(csv_path, args.stable_sec):
        return None

    vehicle_key = vehicle_key_from_rel(rel)
    path_key = str(rel)
    if path_key in state["paths"] and state["paths"][path_key] in state["uploaded"]:
        return {"status": "duplicate", "rel": rel, "key": state["paths"][path_key]}

    try:
        stat = csv_path.stat()
        body, digest = read_with_digest(csv_path)
    except FileNotFoundError:
        return None

    key = f"{vehicle_key}:{digest}"
    if key in state["uploaded"]:
        return {"status": "duplicate", "path": csv_path, "rel": rel, "vin": vehicle_key, "digest": digest, "key": key}

    try:
        if stop_event.is_set():
            return None
        result = post_csv(args.api, body, vehicle_key, args.api_token, args.user_key, args.timeout_sec)
    except HTTPError as exc:
        return {"status": "failed", "rel": rel, "error": f"{exc.code} {exc.reason}"}
    except Exception as exc:
        return {"status": "failed", "rel": rel, "error": str(exc)}

    return {
        "status": "ingested",
        "path": csv_path,
        "rel": rel,
        "vin": vehicle_key,
        "digest": digest,
        "key": key,
        "size": stat.st_size,
        "result": result,
    }


def mark_uploaded(state: dict, key: str, rel: Path, vehicle_key: str, digest: str, size: int, result: dict) -> None:
    state["uploaded"][key] = {
        "vin": vehicle_key,
        "sha256": digest,
        "original_path": str(rel),
        "size": size,
        "uploaded_at": now_iso(),
        "rows_ingested": result.get("rows_ingested"),
    }
    state["paths"][str(rel)] = key


def watch_once(args, state: dict) -> dict:
    ingest_root = args.ingest_path.resolve()
    state_file = (args.state_file or ingest_root / ".scargo-ingest-state.json").resolve()

    updated = {}
    workers = max(1, args.workers)
    save_every = max(1, args.state_save_every)
    pending_state_writes = 0
    stop_event = threading.Event()
    pool = ThreadPoolExecutor(max_workers=workers)
    futures = [
        pool.submit(ingest_candidate, args, state, stop_event, csv_path, rel)
        for csv_path, rel in candidate_files(ingest_root, state_file)
    ]
    try:
        for future in as_completed(futures):
            item = future.result()
            if not item:
                continue

            rel = item["rel"]
            if item["status"] == "failed":
                print(f"ingest error {rel}: {item['error']}", file=sys.stderr)
                continue

            state["paths"][str(rel)] = item["key"]
            if item["status"] == "duplicate":
                pending_state_writes += 1
                if args.print_updates:
                    print(f"skipped duplicate: {rel}")
                if pending_state_writes >= save_every:
                    save_state(state_file, state)
                    pending_state_writes = 0
                continue

            mark_uploaded(
                state,
                item["key"],
                rel,
                item["vin"],
                item["digest"],
                item["size"],
                item["result"],
            )
            pending_state_writes += 1
            updated[str(rel)] = item["result"]
            print(f"ingested: {rel} -> {item['result']}")
            if pending_state_writes >= save_every:
                save_state(state_file, state)
                pending_state_writes = 0
    except KeyboardInterrupt:
        stop_event.set()
        for future in futures:
            future.cancel()
        pool.shutdown(wait=False, cancel_futures=True)
        raise
    else:
        pool.shutdown(wait=True)

    if pending_state_writes:
        save_state(state_file, state)
    return updated


def main(argv=None) -> int:
    args = parse_args(argv)
    ingest_root = args.ingest_path.resolve()
    state_file = (args.state_file or ingest_root / ".scargo-ingest-state.json").resolve()
    state = {"version": STATE_VERSION, "uploaded": {}, "paths": {}} if args.reset_state else load_state(state_file)
    print(
        f"SCARGO watcher: root={ingest_root} api={args.api} poll={args.poll_sec}s "
        f"stable={args.stable_sec}s workers={args.workers} timeout={args.timeout_sec}s "
        f"auth={'api-token' if args.api_token else ('user-key' if args.user_key else 'guest')} "
        f"save_every={args.state_save_every} state={state_file} "
        f"reset_state={args.reset_state}"
    )

    try:
        while True:
            watch_once(args, state)
            save_state(state_file, state)
            if args.once:
                break
            time.sleep(args.poll_sec)
    except KeyboardInterrupt:
        save_state(state_file, state)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
