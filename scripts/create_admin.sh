if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
  echo "Postgres is not running!"
  echo "Start it: podman compose -f docker-compose.yaml up -d postgres"
  exit 1
fi

cargo run --bin create_admin