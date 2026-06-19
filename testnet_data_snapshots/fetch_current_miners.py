#!/usr/bin/env python3
"""Fetch current testnet miner stats from Quantus GraphQL and save as JSON snapshot."""

from __future__ import annotations

import argparse
import json
import sys
import urllib.error
import urllib.request
from pathlib import Path

DEFAULT_ENDPOINT = "https://sub2.quantus.com/v1/graphql"
DEFAULT_OUTPUT = "planck_miners.json"

MINER_STATS_QUERY = """
query MyQuery {
  minerStats: account_stats(
    where: {total_mined_blocks: {_gt: 0}}
    order_by: {total_mined_blocks: desc}
  ) {
    id
    totalMinedBlocks: total_mined_blocks
    totalRewards: total_rewards
  }
}
""".strip()


def fetch_miner_stats(endpoint: str) -> dict:
    body = json.dumps({"query": MINER_STATS_QUERY}).encode("utf-8")
    request = urllib.request.Request(
        endpoint,
        data=body,
        headers={
            "Content-Type": "application/json",
            "User-Agent": "task-master/1.0 (miner-stats-fetch)",
        },
        method="POST",
    )

    try:
        with urllib.request.urlopen(request, timeout=60) as response:
            payload = json.load(response)
    except urllib.error.HTTPError as exc:
        error_body = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"GraphQL request failed ({exc.code}): {error_body}") from exc
    except urllib.error.URLError as exc:
        raise RuntimeError(f"Could not reach GraphQL endpoint: {exc.reason}") from exc

    if "errors" in payload:
        raise RuntimeError(f"GraphQL errors: {json.dumps(payload['errors'], indent=2)}")

    if "data" not in payload or "minerStats" not in payload["data"]:
        raise RuntimeError(f"Unexpected GraphQL response shape: {json.dumps(payload, indent=2)}")

    return payload


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--endpoint",
        default=DEFAULT_ENDPOINT,
        help=f"GraphQL endpoint URL (default: {DEFAULT_ENDPOINT})",
    )
    parser.add_argument(
        "--output",
        default=DEFAULT_OUTPUT,
        help=f"Output JSON filename inside testnet_data_snapshots (default: {DEFAULT_OUTPUT})",
    )
    parser.add_argument(
        "--export-csv",
        action="store_true",
        help="Also export the fetched JSON to CSV via export_miners_to_csv.py",
    )
    args = parser.parse_args()

    snapshot_dir = Path(__file__).resolve().parent
    output_path = snapshot_dir / args.output

    print(f"Fetching miner stats from {args.endpoint} ...")
    payload = fetch_miner_stats(args.endpoint)
    miner_count = len(payload["data"]["minerStats"])

    with output_path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2)
        handle.write("\n")

    print(f"Wrote {output_path.name} ({miner_count} miners)")

    if args.export_csv:
        from export_miners_to_csv import convert_json_to_csv

        convert_json_to_csv(output_path)

    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except RuntimeError as exc:
        print(exc, file=sys.stderr)
        raise SystemExit(1)
