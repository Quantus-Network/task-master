#!/bin/bash
set -e

# CONFIG ------------------------------------------
CONTAINER_NAME="task_master_test_db"       # Change if your container is named differently
DB_USER="postgres"
DB_NAME="task_master"
SQL_FILE="seed_authors.sql"
# --------------------------------------------------

echo "🔧 Generating seed SQL..."

cat << 'EOF' > $SQL_FILE
INSERT INTO tweet_authors (
    id, name, username, followers_count, following_count, 
    tweet_count, listed_count, like_count, media_count, fetched_at
)
VALUES 
('1862779229277954048', 'Yuvi Lightman', 'YuviLightman', 0, 0, 0, 0, 0, 0, NOW())
ON CONFLICT (id) DO UPDATE SET
    name = EXCLUDED.name,
    username = EXCLUDED.username,
    followers_count = EXCLUDED.followers_count,
    following_count = EXCLUDED.following_count,
    tweet_count = EXCLUDED.tweet_count,
    listed_count = EXCLUDED.listed_count,
    like_count = EXCLUDED.like_count,
    media_count = EXCLUDED.media_count,
    fetched_at = NOW();
EOF

echo "📦 Copying SQL file into container ($CONTAINER_NAME)..."
podman cp "$SQL_FILE" "$CONTAINER_NAME":/"$SQL_FILE"

echo "🚀 Running seed script inside Postgres..."
podman exec -it "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -f "/$SQL_FILE"

echo "🔍 Verifying result..."
podman exec -it "$CONTAINER_NAME" psql -U "$DB_USER" -d "$DB_NAME" -c "SELECT * FROM tweet_authors WHERE id = '1862779229277954048';"

rm -rf "$SQL_FILE"

echo "✅ Seeding complete!"
