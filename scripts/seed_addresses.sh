#!/bin/bash
set -e

# CONFIG ------------------------------------------
CONTAINER_NAME="task_master_test_db"       # Change if your container is named differently
DB_USER="postgres"
DB_NAME="task_master"
SQL_FILE="seed_addresses.sql"
# --------------------------------------------------

echo "🔧 Generating seed SQL..."

cat << 'EOF' > $SQL_FILE
-- Optional: clean table first
TRUNCATE TABLE addresses RESTART IDENTITY CASCADE;

INSERT INTO addresses (quan_address, referral_code, referrals_count)
SELECT
    'quan_' || g AS quan_address,
    'REF' || lpad(g::text, 4, '0') AS referral_code,
    (random() * 20)::int AS referrals_count
FROM generate_series(1, 100) g;
EOF

echo "📦 Copying SQL file into container ($CONTAINER_NAME)..."
podman cp "$SQL_FILE" "$CONTAINER_NAME":/"$SQL_FILE"

echo "🚀 Running seed script inside Postgres..."
podman exec -it "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -f "/$SQL_FILE"

echo "🔍 Verifying result..."
podman exec -it "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -c "SELECT COUNT(*) AS total_rows FROM addresses;"

rm -rf "$SQL_FILE"

echo "✅ Seeding complete!"
