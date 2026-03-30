#!/bin/bash
set -e

# CONFIG ------------------------------------------
CONTAINER_NAME="task_master_test_db"       # Change if your container is named differently
DB_USER="postgres"
DB_NAME="task_master"
SQL_FILE="seed_relevant_tweets.sql"
# --------------------------------------------------

echo "🔧 Generating seed SQL..."

cat << 'EOF' > $SQL_FILE
INSERT INTO relevant_tweets (
    id, author_id, text, created_at, fetched_at
)
VALUES 
('2037267283798593848', '420308365', 'Test Tweet', '2026-03-26 20:35:45+00', '2026-03-29 12:38:29.171782+00');
EOF

echo "📦 Copying SQL file into container ($CONTAINER_NAME)..."
podman cp "$SQL_FILE" "$CONTAINER_NAME":/"$SQL_FILE"

echo "🚀 Running seed script inside Postgres..."
podman exec -it "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -f "/$SQL_FILE"

echo "🔍 Verifying result..."
podman exec -it "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -c "SELECT * FROM relevant_tweets WHERE id = '2037267283798593848';"

rm -rf "$SQL_FILE"

echo "✅ Seeding complete!"
