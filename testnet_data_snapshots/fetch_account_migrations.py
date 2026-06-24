#!/usr/bin/env python3
"""Fetch wallet account migration mappings from Supabase."""

from __future__ import annotations

import argparse
import csv
import json
import sys
import urllib.error
import urllib.parse
import urllib.request
from io import StringIO
from pathlib import Path

DEFAULT_ENV_PATH = Path(__file__).resolve().parent.parent / ".env"
TABLE = "account_id_mappings"
COLUMNS = "old_account_id,new_account_id,public_key_hex"


def load_env(path: Path) -> dict[str, str]:
    if not path.is_file():
        raise SystemExit(f"Env file not found: {path}")

    env: dict[str, str] = {}
    for line in path.read_text().splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        if "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        key = key.strip()
        value = value.strip()
        if len(value) >= 2 and value[0] == value[-1] and value[0] in "\"'":
            value = value[1:-1]
        env[key] = value
    return env


def require_env(env: dict[str, str], key: str) -> str:
    value = env.get(key, "").strip()
    if not value:
        raise SystemExit(f"{key} is missing from the env file")
    return value


def fetch_mappings(supabase_url: str, supabase_key: str) -> list[dict[str, str]]:
    query = urllib.parse.urlencode({"select": COLUMNS})
    url = f"{supabase_url.rstrip('/')}/rest/v1/{TABLE}?{query}"
    request = urllib.request.Request(
        url,
        headers={
            "apikey": supabase_key,
            "Authorization": f"Bearer {supabase_key}",
        },
    )

    try:
        with urllib.request.urlopen(request) as response:
            body = response.read().decode()
    except urllib.error.HTTPError as error:
        detail = error.read().decode()
        raise SystemExit(f"Supabase request failed ({error.code}): {detail}") from error

    rows = json.loads(body)
    if not isinstance(rows, list):
        raise SystemExit("Unexpected Supabase response: expected a JSON array")
    return rows


def build_old_to_new(rows: list[dict[str, str]]) -> dict[str, str]:
    return {row["old_account_id"]: row["new_account_id"] for row in rows}


def build_new_to_old(rows: list[dict[str, str]]) -> dict[str, str]:
    return {row["new_account_id"]: row["old_account_id"] for row in rows}


def extend_chain(start: str, old_to_new: dict[str, str]) -> list[str]:
    chain = [start]
    seen = {start}
    while chain[-1] in old_to_new:
        nxt = old_to_new[chain[-1]]
        if nxt in seen:
            break
        chain.append(nxt)
        seen.add(nxt)
    return chain


def build_all_chains(rows: list[dict[str, str]]) -> dict[str, list[str]]:
    """Build migration chains keyed by the root (first) address.

    Each value is the full ordered chain from oldest to newest address, e.g.
    ``{"qzj...": ["qzj...", "qzp...", "qzn..."]}``.
    """
    old_to_new = build_old_to_new(rows)
    if not old_to_new:
        return {}

    new_accounts = set(old_to_new.values())
    roots = sorted(old for old in old_to_new if old not in new_accounts)

    covered: set[str] = set()
    chains: list[list[str]] = []

    for root in roots:
        chain = extend_chain(root, old_to_new)
        chains.append(chain)
        covered.update(chain)

    for old in sorted(old_to_new):
        if old not in covered:
            chain = extend_chain(old, old_to_new)
            chains.append(chain)
            covered.update(chain)

    chains.sort(key=lambda chain: chain[0])
    return {chain[0]: chain for chain in chains}


def chain_for_address(rows: list[dict[str, str]], address: str) -> list[str]:
    old_to_new = build_old_to_new(rows)
    new_to_old = build_new_to_old(rows)

    start = address
    while start in new_to_old:
        start = new_to_old[start]

    return extend_chain(start, old_to_new)


def load_rows_from_file(path: Path) -> list[dict[str, str]]:
    data = json.loads(path.read_text())
    if not isinstance(data, list):
        raise SystemExit(f"Expected a JSON array in {path}")
    return data


def format_output(rows: list[dict[str, str]], fmt: str) -> str:
    if fmt == "json":
        return json.dumps(rows, indent=2)
    if fmt == "csv":
        output = StringIO()
        writer = csv.DictWriter(
            output,
            fieldnames=["old_account_id", "new_account_id", "public_key_hex"],
        )
        writer.writeheader()
        writer.writerows(rows)
        return output.getvalue().rstrip("\n")
    if fmt == "old-to-new":
        return json.dumps(build_old_to_new(rows), indent=2)
    if fmt == "new-to-old":
        return json.dumps(build_new_to_old(rows), indent=2)
    if fmt == "chains":
        return json.dumps(build_all_chains(rows), indent=2)
    raise SystemExit(f"Invalid format: {fmt}")


def lookup(rows: list[dict[str, str]], address: str, *, reverse: bool) -> str:
    mapping = build_new_to_old(rows) if reverse else build_old_to_new(rows)
    result = mapping.get(address)
    if result is None:
        raise SystemExit(f"No mapping found for: {address}")
    return result


def build_chain(rows: list[dict[str, str]], address: str) -> str:
    return json.dumps(chain_for_address(rows, address), indent=2)


def write_output(content: str, output_path: Path) -> None:
    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(content if content.endswith("\n") else content + "\n")
    print(f"Saved to {output_path.resolve()}", file=sys.stderr)


def default_output_path(fmt: str, *, mode: str = "export") -> Path:
    script_dir = Path(__file__).resolve().parent
    if mode == "lookup" or mode == "reverse":
        return script_dir / "account_migrations.lookup.txt"
    if mode == "chain":
        return script_dir / "account_migrations.chain.json"
    if fmt == "csv":
        return script_dir / "account_migrations.csv"
    if fmt == "chains":
        return script_dir / "account_migrations.chains.json"
    if fmt == "json":
        return script_dir / "account_migrations.json"
    return script_dir / f"account_migrations.{fmt}.json"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Fetch wallet account migration mappings from Supabase.",
    )
    parser.add_argument(
        "--env",
        type=Path,
        default=DEFAULT_ENV_PATH,
        help=f"Path to .env file (default: {DEFAULT_ENV_PATH})",
    )
    parser.add_argument(
        "--format",
        choices=["json", "csv", "old-to-new", "new-to-old", "chains"],
        default="chains",
        help="Output format (default: chains)",
    )
    parser.add_argument(
        "--input",
        type=Path,
        metavar="PATH",
        help="Read raw migration rows from a local JSON file instead of Supabase",
    )
    parser.add_argument("--lookup", metavar="ADDRESS", help="Resolve old account ID to new")
    parser.add_argument("--reverse", metavar="ADDRESS", help="Resolve new account ID to old")
    parser.add_argument("--chain", metavar="ADDRESS", help="Print full migration chain")
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        metavar="PATH",
        help="Output file path (default: scripts/account_migrations.<format>.json)",
    )
    return parser.parse_args()


def resolve_output_path(args: argparse.Namespace, *, mode: str = "export") -> Path:
    if args.output is not None:
        return args.output
    return default_output_path(args.format, mode=mode)


def main() -> None:
    args = parse_args()

    if args.input is not None:
        rows = load_rows_from_file(args.input)
    else:
        env = load_env(args.env)
        rows = fetch_mappings(
            require_env(env, "SUPABASE_URL"),
            require_env(env, "SUPABASE_ANON_KEY"),
        )

    if args.lookup:
        write_output(
            lookup(rows, args.lookup, reverse=False),
            resolve_output_path(args, mode="lookup"),
        )
        return
    if args.reverse:
        write_output(
            lookup(rows, args.reverse, reverse=True),
            resolve_output_path(args, mode="reverse"),
        )
        return
    if args.chain:
        write_output(
            build_chain(rows, args.chain),
            resolve_output_path(args, mode="chain"),
        )
        return

    write_output(format_output(rows, args.format), resolve_output_path(args))


if __name__ == "__main__":
    main()
