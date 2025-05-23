#!/bin/zsh
# Test script for Hoya

# Colors for better readability
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Start the server in the background
echo "${YELLOW}Starting Hoya server in the background...${NC}"
cargo run &
SERVER_PID=$!

# Give the server time to start
sleep 2

# Function to test JavaScript execution
test_js() {
  echo "${BLUE}=== Testing JavaScript Execution ===${NC}"
  
  # Serve the test.js file using Python's http.server
  # Start in the background on port 8000
  echo "Starting a simple HTTP server to serve the test.js file..."
  cd "$(dirname "$0")/examples"
  python3 -m http.server 8000 &
  HTTP_PID=$!
  
  # Give the HTTP server time to start
  sleep 1
  
  # Execute the test.js file via Hoya
  echo "Sending request to execute test.js..."
  RESPONSE=$(curl -s -X POST http://localhost:3000/execute \
    -H "Content-Type: application/json" \
    -d '{"url": "http://localhost:8000/test.js"}')
  
  echo "${GREEN}Response from server:${NC}"
  echo $RESPONSE | python3 -m json.tool
  
  # Stop the HTTP server
  kill $HTTP_PID
}

# Function to build and test WebAssembly execution
test_wasm() {
  echo "${BLUE}=== Testing WebAssembly Execution ===${NC}"
  
  # Check if the Wasm module exists, build it if not
  WASM_DIR="$(dirname "$0")/examples/wasm-test"
  TARGET_DIR="$WASM_DIR/target/wasm32-unknown-unknown/release"
  WASM_FILE="$TARGET_DIR/hoya_wasm_test.wasm"
  
  if [ ! -f "$WASM_FILE" ]; then
    echo "Building WebAssembly test module..."
    cd "$WASM_DIR"
    
    # Check if wasm32-unknown-unknown target is installed
    if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
      echo "Installing wasm32-unknown-unknown target..."
      rustup target add wasm32-unknown-unknown
    fi
    
    cargo build --target wasm32-unknown-unknown --release
    cd -
  fi
  
  # Serve the wasm file using Python's http.server
  echo "Starting a simple HTTP server to serve the wasm file..."
  cd "$TARGET_DIR"
  python3 -m http.server 8001 &
  WASM_HTTP_PID=$!
  
  # Give the HTTP server time to start
  sleep 1
  
  # Execute the wasm file via Hoya
  echo "Sending request to execute WebAssembly module..."
  RESPONSE=$(curl -s -X POST http://localhost:3000/execute \
    -H "Content-Type: application/json" \
    -d '{"url": "http://localhost:8001/hoya_wasm_test.wasm"}')
  
  echo "${GREEN}Response from server:${NC}"
  echo $RESPONSE | python3 -m json.tool
  
  # Stop the HTTP server
  kill $WASM_HTTP_PID
}

# Execute the tests
test_js
echo ""
test_wasm

# Clean up
echo "${YELLOW}Stopping the server...${NC}"
kill $SERVER_PID

echo "${GREEN}All tests complete!${NC}"
