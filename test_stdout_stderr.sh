#!/bin/zsh
# Test script for stdout and stderr capturing

# Build the test WASM module
echo "Building WASM test module..."
cd examples/wasm-stdout-stderr
cargo build --target wasm32-wasi --release
cd ../..

# Start the Hoya server in the background
echo "Starting Hoya server..."
cargo run &
SERVER_PID=$!

# Give the server time to start
sleep 2

# Test JavaScript stdout/stderr
echo "\nTesting JavaScript stdout/stderr capturing..."
curl -X POST http://127.0.0.1:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "file:///Users/jon.chen/repo/worktools/hoya/examples/stdout_stderr_test.js"}' | jq

# Test WebAssembly stdout/stderr
echo "\nTesting WebAssembly stdout/stderr capturing..."
curl -X POST http://127.0.0.1:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "file:///Users/jon.chen/repo/worktools/hoya/examples/wasm-stdout-stderr/target/wasm32-wasi/release/wasm_stdout_stderr.wasm"}' | jq

# Clean up
echo "\nStopping Hoya server..."
kill $SERVER_PID
