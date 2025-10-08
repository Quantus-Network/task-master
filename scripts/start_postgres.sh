#!/usr/bin/env bash
set -e

DATA_DIR="/tmp/pgdata"

if [ ! -d "$DATA_DIR" ]; then
  initdb -D "$DATA_DIR"
fi

postgres -D "$DATA_DIR" > /tmp/postgres.log 2>&1 &

# Wait until PostgreSQL is ready
until pg_isready -q; do sleep 0.2; done

# Ensure postgres role with password exists (socket auth; no -h)
psql -d postgres -v ON_ERROR_STOP=1 -c "DO $$ BEGIN IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'postgres') THEN CREATE ROLE postgres WITH LOGIN SUPERUSER PASSWORD 'postgres'; END IF; END $$;" 2>/dev/null || true

# Ensure task_master database exists
createdb -U postgres task_master 2>/dev/null || true

# Run migrations
MIGRATION_FILE="$(cd "$(dirname "$0")"/.. && pwd)/migrations/001_initial_psql.sql"
psql -U postgres -d task_master -f "$MIGRATION_FILE" 2>/dev/null || true
