#!/bin/bash

set -e

if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
  echo "Postgres is not running. Starting it..."
  podman compose -f docker-compose.yaml up -d postgres
  
  echo "Waiting for postgres to be ready..."
  for i in {1..30}; do
    if podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
      echo "Postgres is ready!"
      break
    fi
    sleep 1
  done
  
  if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
    echo "Failed to start postgres!"
    exit 1
  fi
fi

echo "Running non-chain tests..."
cargo test -- --skip chain_ --test-threads=1 --nocapture
