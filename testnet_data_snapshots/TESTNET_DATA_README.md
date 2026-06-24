## Overview

This folder contains snapshot data from different testnets to figure out rewards

Note that addresses between resonance and schrodinger have changed, and will change again for testnet (3).

Past testnets: `resonance_network`, `schrodinger`, `dirac`. Current testnet snapshot: `planck_miners.json`.

## Fetch current testnet data

```bash
python3 testnet_data_snapshots/fetch_current_miners.py
```

This queries `https://sub2.quantus.com/v1/graphql` and writes `planck_miners.json`.

Options:

- `--output other_miners.json` — custom output filename
- `--export-csv` — also write the matching `.csv` file

You can also query GraphQL manually with the `minerStats` alias on `account_stats`.

## Export to CSV

To convert miner JSON snapshots into spreadsheet-friendly CSV:

```bash
python3 testnet_data_snapshots/export_miners_to_csv.py
```

This writes one `.csv` per `*_miners.json` file (same directory) with columns: `id`, `totalMinedBlocks`, `totalRewards`.