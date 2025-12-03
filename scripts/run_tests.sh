# Our integration tests require postgres to be running
# Check for that first

if ! podman compose -f docker-compose.yaml exec -T postgres pg_isready -U postgres >/dev/null 2>&1; then
  echo "Postgres is not running!"
  echo "Start it like this:"
  echo "podman machine start"
  echo "podman compose -f docker-compose.yaml up -d postgres"
  exit 1
fi

# Check if chain tests should be run
if [ "$1" = "chain" ]; then
  # Check for blockchain node when running chain tests
  if [ "$(curl -s -o /dev/null -w '%{http_code}' http://localhost:9944)" != "405" ]; then
    echo "Blockchain node is not running!"
    echo "Please start the development chain first."
    exit 1
  fi
  
  echo "Running tests with chain feature enabled..."
  cargo test --features chain -- --test-threads=1 --nocapture
else
  echo "Running tests without chain feature..."
  echo "Note: Run this script with a 'chain' parameter to run chain tests too"
  echo "Example: ./scripts/run_tests.sh chain"
  cargo test -- --test-threads=1 --nocapture
fi