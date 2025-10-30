#!/bin/bash

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"

DB_HOST="127.0.0.1"
DB_PORT="55432"
DB_NAME="task_master"
DB_USER="postgres"
DB_PASSWORD="postgres"

echo "🔄 Running database migrations..."

if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres > /dev/null 2>&1; then
    echo "❌ PostgreSQL is not ready. Please start it first with: ./scripts/start_postgres.sh"
    exit 1
fi

MIGRATION_FILE="$MIGRATIONS_DIR/002_add_opt_ins_table.sql"

if [ ! -f "$MIGRATION_FILE" ]; then
    echo "❌ Migration file not found: $MIGRATION_FILE"
    exit 1
fi

echo "📝 Applying migration: $(basename $MIGRATION_FILE)"

PGPASSWORD="$DB_PASSWORD" psql -h "$DB_HOST" -p "$DB_PORT" -U "$DB_USER" -d "$DB_NAME" -f "$MIGRATION_FILE"

if [ $? -eq 0 ]; then
    echo "✅ Migration applied successfully!"
else
    echo "❌ Migration failed!"
    exit 1
fi

