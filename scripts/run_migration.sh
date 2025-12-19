#!/bin/bash

set -e

# --- Configuration ---
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"

# Database credentials (must match docker-compose)
DB_HOST="127.0.0.1"
DB_PORT="55432"
DB_NAME="task_master"
DB_USER="postgres"
DB_PASSWORD="postgres"
CONTAINER_NAME="task_master_test_db"

# Construct DATABASE_URL for sqlx
export DATABASE_URL="postgres://${DB_USER}:${DB_PASSWORD}@${DB_HOST}:${DB_PORT}/${DB_NAME}"

# --- Checks ---

echo "🔍 Checking prerequisites..."

# 1. Check if sqlx-cli is installed
if ! command -v sqlx &> /dev/null; then
    echo "❌ Error: 'sqlx' CLI is not installed."
    echo "   To install it, run: cargo install sqlx-cli --no-default-features --features postgres"
    exit 1
fi

# 2. Check if Podman is installed
if ! command -v podman &> /dev/null; then
    echo "❌ Error: 'podman' is not installed or not in your PATH."
    exit 1
fi

# 3. Check if the specific container is running
if ! podman ps --format '{{.Names}}' | grep -q "^${CONTAINER_NAME}$"; then
    echo "❌ Error: Podman container '${CONTAINER_NAME}' is not running."
    echo "   Please start it with: ./scripts/start_postgres.sh"
    exit 1
fi

# 4. Check if Postgres is actually accepting connections
echo "⏳ Waiting for database to be ready..."
# We use 'podman exec' to run the check inside the container
until podman exec "$CONTAINER_NAME" pg_isready -U "$DB_USER" > /dev/null 2>&1; do
    echo "   ... database not ready yet. Retrying in 1s..."
    sleep 1
done
echo "✅ Database is up and reachable."

# --- Execution ---

echo "🔄 Running migrations using SQLx..."
echo "   Source: $MIGRATIONS_DIR"
echo "   Target: $DB_HOST:$DB_PORT/$DB_NAME"

# Run the migrations
sqlx migrate run --source "$MIGRATIONS_DIR"

if [ $? -eq 0 ]; then
    echo "✅ All migrations applied successfully!"
else
    echo "❌ Migration failed!"
    exit 1
fi