#!/usr/bin/env python3
"""Export GraphQL minerStats JSON snapshots to CSV for spreadsheet import."""

from __future__ import annotations

import csv
import json
import sys
from pathlib import Path

COLUMNS = ("id", "totalMinedBlocks", "totalRewards")


def convert_json_to_csv(json_path: Path) -> int:
    with json_path.open(encoding="utf-8") as handle:
        payload = json.load(handle)

    miners = payload["data"]["minerStats"]
    csv_path = json_path.with_suffix(".csv")

    with csv_path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=COLUMNS)
        writer.writeheader()
        writer.writerows(miners)

    print(f"{json_path.name} -> {csv_path.name} ({len(miners)} rows)")
    return len(miners)


def main() -> int:
    snapshot_dir = Path(__file__).resolve().parent
    json_files = sorted(snapshot_dir.glob("*_miners.json"))

    if not json_files:
        print(f"No *_miners.json files found in {snapshot_dir}", file=sys.stderr)
        return 1

    total_rows = 0
    for json_path in json_files:
        total_rows += convert_json_to_csv(json_path)

    print(f"Exported {len(json_files)} file(s), {total_rows} total miner rows.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
