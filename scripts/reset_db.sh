#!/bin/bash

set -e

echo "🔄 Resetting database..."

if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
  echo "Postgres is not running!"
  echo "Start it like this:"
  echo "podman machine start"
  echo "podman compose -f docker-compose.yaml up -d postgres"
  exit 1
fi

echo "Dropping all tables and resetting migration state..."

podman compose -f docker-compose.yaml exec -T postgres psql -U postgres -d task_master <<EOF
DROP SCHEMA public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO postgres;
GRANT ALL ON SCHEMA public TO public;
EOF

echo "✅ Database reset complete!"
echo "Migrations will be applied automatically on next application startup."

