#!/bin/bash

# TaskMaster API Test Script
# Tests the HTTP endpoints of the TaskMaster server

set -e

BASE_URL="http://localhost:3000"

echo "🧪 Testing TaskMaster API Endpoints"
echo "=================================="

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Function to make HTTP requests and check response
test_endpoint() {
    local method=$1
    local endpoint=$2
    local data=$3
    local expected_status=$4

    echo -e "${YELLOW}Testing: $method $endpoint${NC}"

    if [ -z "$data" ]; then
        response=$(curl -s -w "HTTPSTATUS:%{http_code}" -X "$method" "$BASE_URL$endpoint")
    else
        response=$(curl -s -w "HTTPSTATUS:%{http_code}" -X "$method" "$BASE_URL$endpoint" \
            -H "Content-Type: application/json" \
            -d "$data")
    fi

    body=$(echo "$response" | sed -E 's/HTTPSTATUS\:[0-9]{3}$//')
    status=$(echo "$response" | tr -d '\n' | sed -E 's/.*HTTPSTATUS:([0-9]{3})$/\1/')

    if [ "$status" -eq "$expected_status" ]; then
        echo -e "${GREEN}✅ Status: $status (expected $expected_status)${NC}"
        if [ -n "$body" ] && [ "$body" != "null" ]; then
            echo "📄 Response: $(echo "$body" | jq -C . 2>/dev/null || echo "$body")"
        fi
    else
        echo -e "${RED}❌ Status: $status (expected $expected_status)${NC}"
        echo "📄 Response: $body"
    fi
    echo ""
}

# Wait for server to be ready
echo "⏳ Checking if TaskMaster server is running..."
max_attempts=10
attempt=1

while [ $attempt -le $max_attempts ]; do
    if curl -s "$BASE_URL/health" > /dev/null 2>&1; then
        echo -e "${GREEN}✅ Server is ready!${NC}"
        break
    else
        echo "   Attempt $attempt/$max_attempts - waiting for server..."
        sleep 2
        ((attempt++))
    fi
done

if [ $attempt -gt $max_attempts ]; then
    echo -e "${RED}❌ Server is not responding. Please start TaskMaster first.${NC}"
    echo "   Run: cargo run"
    exit 1
fi

echo ""

# Test 1: Health Check
test_endpoint "GET" "/health" "" 200

# Test 2: Status Check
test_endpoint "GET" "/status" "" 200

# Test 3: List All Tasks
test_endpoint "GET" "/tasks" "" 200

# Test 4: Complete Task (should fail - task doesn't exist)
task_completion_data='{"task_url": "999999999999"}'
test_endpoint "POST" "/complete" "$task_completion_data" 404

# Test 5: Complete Task with invalid format
invalid_task_data='{"task_url": "invalid"}'
test_endpoint "POST" "/complete" "$invalid_task_data" 400

# Test 6: Get Non-existent Task
test_endpoint "GET" "/tasks/nonexistent-task-id" "" 404

echo "🎉 API endpoint testing completed!"
echo ""

# Additional functionality tests
echo "📊 Additional Server Information"
echo "==============================="

# Get current status
echo "Current Status:"
curl -s "$BASE_URL/status" | jq -C . 2>/dev/null || curl -s "$BASE_URL/status"
echo ""

echo "Health Check:"
curl -s "$BASE_URL/health" | jq -C . 2>/dev/null || curl -s "$BASE_URL/health"
echo ""

# Show example of how to complete a task when one exists
echo "💡 To complete a task when one exists:"
echo "   curl -X POST $BASE_URL/complete \\"
echo '     -H "Content-Type: application/json" \'
echo '     -d '"'"'{"task_url": "123456789012"}'"'"
echo ""

echo "📝 To monitor the CSV file:"
echo "   tail -f tasks.csv"
echo ""

echo "🔍 To check logs with debug level:"
echo "   TASKMASTER_LOGGING__LEVEL=debug cargo run"
