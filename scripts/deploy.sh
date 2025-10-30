#!/bin/bash

set -e

SERVICE_NAME="task-master.service"
WORK_DIR="$HOME/task-master"

echo "🚀 Starting deployment for $SERVICE_NAME..."

echo "📦 Step 1: Stopping service..."
systemctl stop $SERVICE_NAME
echo "✅ Service stopped"

echo "🔄 Step 2: Updating code..."
cd $WORK_DIR
if [ -d ".git" ]; then
    echo "   Detected git repository, pulling latest changes..."
    git pull
else
    echo "   Not a git repository - ensure code is up to date manually"
fi
echo "✅ Code updated"

echo "🔨 Step 3: Building release binary..."
cargo build --release
echo "✅ Build completed"

echo "🔄 Step 4: Starting service (migrations will run automatically)..."
systemctl start $SERVICE_NAME
echo "✅ Service started"

echo "📊 Step 5: Checking service status..."
sleep 2
systemctl status $SERVICE_NAME --no-pager -l

echo "✨ Deployment complete!"

