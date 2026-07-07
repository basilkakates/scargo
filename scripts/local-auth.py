#!/usr/bin/env python3
"""Register or log in a local Scargo user and print an upload token."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("username")
    parser.add_argument("password")
    parser.add_argument(
        "--api",
        default="http://127.0.0.1:8080/api",
        help="Scargo API base (default: %(default)s)",
    )
    parser.add_argument(
        "--login",
        action="store_true",
        help="Log in instead of register, then create a fresh dashboard token",
    )
    return parser.parse_args()


def post_json(url: str, body: dict[str, str], cookie: str | None = None) -> tuple[dict, str | None]:
    req = urllib.request.Request(
        url,
        data=json.dumps(body).encode(),
        headers={"Content-Type": "application/json", **({"Cookie": cookie} if cookie else {})},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req) as response:
            payload = json.load(response)
            set_cookie = response.headers.get("Set-Cookie")
            cookie_value = set_cookie.split(";", 1)[0] if set_cookie else None
            return payload, cookie_value
    except urllib.error.HTTPError as err:
        detail = err.read().decode().strip()
        raise SystemExit(f"{err.code} {detail or err.reason}") from err


def main() -> int:
    args = parse_args()
    auth_path = "login" if args.login else "register"
    payload, cookie = post_json(
        f"{args.api}/auth/{auth_path}",
        {"username": args.username, "password": args.password},
    )

    token = payload.get("upload_token")
    if not token:
        if not cookie:
            raise SystemExit("login succeeded but no session cookie returned")
        token_payload, _ = post_json(
            f"{args.api}/auth/tokens",
            {"label": "local-dev"},
            cookie=cookie,
        )
        token = token_payload.get("upload_token")

    if not token:
        raise SystemExit("no upload token returned")

    sys.stdout.write(token)
    sys.stdout.write("\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
