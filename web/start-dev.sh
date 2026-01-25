#!/bin/bash

# Start UAST Development Environment
echo "Starting UAST Development Environment..."
echo "Frontend: http://localhost:3000"
echo "Backend:  http://localhost:8080"
echo "Press Ctrl+C to stop both servers"
echo ""

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "Stopping servers..."
    if [ -f .vite.pid ]; then
        kill $(cat .vite.pid) 2>/dev/null || true
        rm -f .vite.pid
    fi
    if [ -f .backend.pid ]; then
        kill $(cat .backend.pid) 2>/dev/null || true
        rm -f .backend.pid
    fi
    echo "Servers stopped."
    exit 0
}

# Set up signal handlers
trap cleanup SIGINT SIGTERM

# Start frontend
echo "Starting frontend (Vite)..."
npm run dev &
FRONTEND_PID=$!
echo $FRONTEND_PID > .vite.pid
echo "Frontend started with PID: $FRONTEND_PID"

# Wait for frontend to start
sleep 5

# Start backend
echo "Starting backend (UAST server)..."
uast server --static . --port 8080 &
BACKEND_PID=$!
echo $BACKEND_PID > .backend.pid
echo "Backend started with PID: $BACKEND_PID"

# Wait for both processes
wait 