#!/usr/bin/env python3
"""Aggregate miner stats across testnets, grouped by migration root address."""

from __future__ import annotations

import argparse
import csv
import json
import sys
from collections import defaultdict
from dataclasses import dataclass, field
from pathlib import Path

MINER_COLUMNS = ("id", "totalMinedBlocks", "totalRewards")
TESTNET_ORDER = ("resonance_network", "schrodinger", "dirac", "planck")
OUTPUT_COLUMNS = TESTNET_ORDER + ("totalMinedBlocks", "totalRewards")
DEFAULT_CHAINS_PATH = Path(__file__).resolve().parent / "account_migrations.chains.json"
DEFAULT_OUTPUT_PATH = Path(__file__).resolve().parent / "aggregated_miners.csv"


@dataclass
class AggregatedMiner:
    total_mined_blocks: int = 0
    total_rewards: int = 0
    addresses: set[str] = field(default_factory=set)


def load_chains(path: Path) -> dict[str, list[str]]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(data, dict):
        raise SystemExit(f"Expected a JSON object in {path}")
    return data


def build_address_to_root(chains: dict[str, list[str]]) -> dict[str, str]:
    return {
        address: root
        for root, chain in chains.items()
        for address in chain
    }


def resolve_root(address: str, address_to_root: dict[str, str]) -> str:
    return address_to_root.get(address, address)


def testnet_name_from_csv(path: Path) -> str:
    stem = path.name.removesuffix("_miners.csv")
    if stem not in TESTNET_ORDER:
        raise SystemExit(
            f"Unexpected miner CSV name: {path.name} "
            f"(expected <{'|'.join(TESTNET_ORDER)}>_miners.csv)"
        )
    return stem


def load_miner_csv(path: Path) -> list[dict[str, str]]:
    with path.open(encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle)
        if reader.fieldnames != list(MINER_COLUMNS):
            raise SystemExit(
                f"Unexpected columns in {path.name}: {reader.fieldnames} "
                f"(expected {list(MINER_COLUMNS)})"
            )
        return list(reader)


def pick_address_for_testnet(
    matches: list[str],
    chain: list[str] | None,
) -> str:
    if not matches:
        return ""
    if len(matches) == 1:
        return matches[0]
    if chain:
        for address in reversed(chain):
            if address in matches:
                return address
    return sorted(matches)[0]


def build_testnet_address_columns(
    root: str,
    member_addresses: set[str],
    chains: dict[str, list[str]],
    testnet_addresses: dict[str, set[str]],
) -> dict[str, str]:
    chain = chains.get(root)
    columns: dict[str, str] = {}

    for testnet in TESTNET_ORDER:
        matches = [
            address
            for address in member_addresses
            if address in testnet_addresses[testnet]
        ]
        columns[testnet] = pick_address_for_testnet(matches, chain)

    return columns


def aggregate_miners(
    miner_csv_paths: list[Path],
    chains_path: Path,
) -> tuple[dict[str, AggregatedMiner], dict[str, list[str]], dict[str, set[str]]]:
    chains = load_chains(chains_path)
    address_to_root = build_address_to_root(chains)
    totals: dict[str, AggregatedMiner] = defaultdict(AggregatedMiner)
    testnet_addresses = {testnet: set() for testnet in TESTNET_ORDER}

    for csv_path in miner_csv_paths:
        testnet = testnet_name_from_csv(csv_path)
        for row in load_miner_csv(csv_path):
            address = row["id"]
            root = resolve_root(address, address_to_root)
            entry = totals[root]
            entry.total_mined_blocks += int(row["totalMinedBlocks"])
            entry.total_rewards += int(row["totalRewards"])
            entry.addresses.add(address)
            testnet_addresses[testnet].add(address)

    return totals, chains, testnet_addresses


def write_aggregated_csv(
    totals: dict[str, AggregatedMiner],
    chains: dict[str, list[str]],
    testnet_addresses: dict[str, set[str]],
    output_path: Path,
) -> int:
    rows = sorted(
        totals.items(),
        key=lambda item: item[1].total_mined_blocks,
        reverse=True,
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    with output_path.open("w", newline="", encoding="utf-8") as handle:
        writer = csv.DictWriter(handle, fieldnames=OUTPUT_COLUMNS)
        writer.writeheader()
        for root, entry in rows:
            testnet_columns = build_testnet_address_columns(
                root,
                entry.addresses,
                chains,
                testnet_addresses,
            )
            writer.writerow(
                {
                    **testnet_columns,
                    "totalMinedBlocks": str(entry.total_mined_blocks),
                    "totalRewards": str(entry.total_rewards),
                }
            )

    return len(rows)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--chains",
        type=Path,
        default=DEFAULT_CHAINS_PATH,
        help=f"Migration chains JSON (default: {DEFAULT_CHAINS_PATH.name})",
    )
    parser.add_argument(
        "--input",
        type=Path,
        action="append",
        dest="inputs",
        metavar="PATH",
        help=(
            "Miner CSV to include "
            "(default: dirac, resonance_network, schrodinger, and planck miner CSVs)"
        ),
    )
    parser.add_argument(
        "-o",
        "--output",
        type=Path,
        default=DEFAULT_OUTPUT_PATH,
        help=f"Output CSV path (default: {DEFAULT_OUTPUT_PATH.name})",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    snapshot_dir = Path(__file__).resolve().parent

    if args.inputs:
        miner_csv_paths = sorted(args.inputs)
    else:
        miner_csv_paths = [
            snapshot_dir / f"{testnet}_miners.csv"
            for testnet in TESTNET_ORDER
            if (snapshot_dir / f"{testnet}_miners.csv").is_file()
        ]

    if not miner_csv_paths:
        print("No miner CSV files found.", file=sys.stderr)
        return 1

    totals, chains, testnet_addresses = aggregate_miners(miner_csv_paths, args.chains)
    row_count = write_aggregated_csv(totals, chains, testnet_addresses, args.output)

    print(
        f"Aggregated {len(miner_csv_paths)} file(s), "
        f"{row_count} unique root address(es) -> {args.output.name}"
    )
    for csv_path in miner_csv_paths:
        print(f"  - {csv_path.name}")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
