#!/usr/bin/env bash
set -e

# Ensure Podman machine exists
if ! podman machine list | grep -q running; then
  if ! podman machine list | grep -q "podman-machine-default"; then
    echo "Creating Podman machine..."
    podman machine init
  fi
  echo "Starting Podman machine..."
  podman machine start
fi

# Run Postgres service via compose
echo "Starting Postgres container..."
podman compose -f docker-compose.yaml up -d postgres