#!/usr/bin/env bash
set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

COMPOSE_FILE="docker-compose.yaml"
SERVICE_NAME="postgres"
CHAIN_DIR="../chain"
CHAIN_LOG="/tmp/chain_node.log"
CHAIN_PID=""

echo -e "${YELLOW}Starting PostgreSQL container...${NC}"
podman compose -f "$COMPOSE_FILE" up -d

# Function to cleanup on exit
cleanup() {
  EXIT_CODE=$?
  
  # Stop blockchain node if running
  if [ -n "$CHAIN_PID" ] && kill -0 "$CHAIN_PID" 2>/dev/null; then
    echo -e "\n${YELLOW}Stopping blockchain node (PID: $CHAIN_PID)...${NC}"
    kill "$CHAIN_PID" 2>/dev/null || true
    wait "$CHAIN_PID" 2>/dev/null || true
  fi
  
  # Stop PostgreSQL
  echo -e "${YELLOW}Stopping and removing PostgreSQL container...${NC}"
  podman compose -f "$COMPOSE_FILE" down -v
  
  if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}Tests completed successfully!${NC}"
  else
    echo -e "${RED}Tests failed with exit code $EXIT_CODE${NC}"
  fi
  
  exit $EXIT_CODE
}

# Register cleanup function to run on script exit
trap cleanup EXIT INT TERM

echo -e "${YELLOW}Waiting for PostgreSQL to be ready...${NC}"
# Wait for the healthcheck to pass
MAX_ATTEMPTS=30
ATTEMPT=0
until podman compose -f "$COMPOSE_FILE" exec -T "$SERVICE_NAME" pg_isready -U postgres > /dev/null 2>&1; do
  ATTEMPT=$((ATTEMPT + 1))
  if [ $ATTEMPT -ge $MAX_ATTEMPTS ]; then
    echo -e "${RED}PostgreSQL failed to become ready in time${NC}"
    exit 1
  fi
  sleep 1
done

echo -e "${GREEN}PostgreSQL is ready!${NC}\n"

# Start blockchain node
echo -e "${YELLOW}Starting blockchain node...${NC}"
cd "$CHAIN_DIR"
cargo run -- --dev > "$CHAIN_LOG" 2>&1 &
CHAIN_PID=$!
echo -e "${GREEN}Blockchain node started (PID: $CHAIN_PID)${NC}"
echo -e "${YELLOW}Logs available at: $CHAIN_LOG${NC}\n"

# Wait for blockchain node to be ready
echo -e "${YELLOW}Waiting for blockchain node to be ready...${NC}"
sleep 20

MAX_ATTEMPTS=30
ATTEMPT=0
until [ "$(curl -s -o /dev/null -w '%{http_code}' http://localhost:9944)" = "405" ]; do
  ATTEMPT=$((ATTEMPT + 1))
  if [ $ATTEMPT -ge $MAX_ATTEMPTS ]; then
    echo -e "${RED}Blockchain node failed to become ready in time${NC}"
    echo -e "${RED}Check logs at: $CHAIN_LOG${NC}"
    exit 1
  fi
  sleep 1
done

echo -e "${GREEN}Blockchain node is ready!${NC}\n"

# Go back to test directory
cd - > /dev/null

echo -e "${YELLOW}Running cargo tests...${NC}"

# Run the tests
cargo test -- --test-threads=1

# The cleanup function will automatically run due to the trap