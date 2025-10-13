# Our integration tests only run when both dev chain and postgres are running
# Check for that first


if [ "$(curl -s -o /dev/null -w '%{http_code}' http://localhost:9944)" != "405" ]; then
  echo "Blockchain node is not running!"
  echo "Please start the development chain first."
  exit 1
fi

if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
  echo "Postgres is not running!"
  echo "Start it: podman compose -f docker-compose.yaml up -d postgres"
  exit 1
fi

# Our integration tests must run single threaded.
cargo test -- --test-threads=1 --nocapture