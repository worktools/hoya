# Hoya

Hoya is a service that provides remote code execution for JavaScript and WebAssembly files. It fetches code from a URL and executes it in a sandboxed environment, providing a few key APIs for both JavaScript and WebAssembly runtimes.

## Features

### FR1: Code Retrieval

- **FR1.3**: Auto-detection of code type from URL (.js or .wasm)
- **FR1.4**: Downloading code from remote URLs

### FR2: JavaScript Execution

- **FR2.1**: Execute JavaScript code in a QuickJS runtime
- **FR2.3**: API Injections:
  - `app_log(level, message)`: Log messages with a specified level
  - `get_unixtime()`: Get the current Unix timestamp
  - `fetch(options)`: Make HTTP requests (limited implementation)

### FR3: WebAssembly Execution

- **FR3.1**: Execute WebAssembly modules
- **FR3.3**: API Injections:
  - `app_log(level_ptr, level_len, msg_ptr, msg_len)`: Log messages with a specified level
  - `get_unixtime()`: Get the current Unix timestamp
  - `fetch(options_ptr, options_len, resp_buf_ptr, resp_buf_max_len)`: Make HTTP requests

## Getting Started

### Prerequisites

- Rust toolchain

### Installation

```bash
# Clone the repository
git clone https://github.com/yourusername/hoya.git
cd hoya

# Build the project
cargo build
```

### Running the Server

```bash
cargo run
```

This will start the server on `http://127.0.0.1:3000`.

## Usage

Hoya exposes a single endpoint `/execute` which takes a JSON payload with a `url` field pointing to a JavaScript or WebAssembly file.

### Example: Execute JavaScript

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/your-script.js"}'
```

### Example: Execute WebAssembly

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/your-module.wasm"}'
```

## Testing the Features

The repository includes example files and a test script to help you verify the functionality.

### Quick Test

You can use the included test script to quickly test the JavaScript functionality:

```bash
# Make the script executable if needed
chmod +x test_hoya.sh

# Run the test script
./test_hoya.sh
```

This script:

1. Starts the Hoya server in the background
2. Starts a simple HTTP server to serve the example JavaScript file
3. Sends a request to execute the JavaScript file
4. Displays the response
5. Cleans up by stopping both servers

### Testing JavaScript Execution Manually

1. Use the included example JavaScript file in `examples/test.js`:

```javascript
// examples/test.js
app_log("INFO", "Testing app_log API");
app_log("ERROR", "This is an error message");
app_log("DEBUG", "This is a debug message");

// Test get_unixtime API
const timestamp = get_unixtime();
app_log("INFO", `Current Unix timestamp: ${timestamp}`);

// Test console.log (should be captured by app_log internally)
console.log("This is a console.log message");

// Try to use fetch API
try {
  fetch({ url: "https://example.com" });
} catch (error) {
  app_log("ERROR", `Fetch failed as expected: ${error.message}`);
}

// Return a value (will be sent back in the response)
("JavaScript execution completed successfully. 🎉");
```

2. Start a server to host the file (e.g., using Python's http.server):

```bash
cd examples
python3 -m http.server 8000
```

3. In another terminal, send a request to execute it:

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "http://localhost:8000/test.js"}'
```

### Testing WebAssembly Execution

The repository includes a WebAssembly example project in `examples/wasm-test/`:

1. Build the WebAssembly test module:

```bash
cd examples/wasm-test
rustup target add wasm32-unknown-unknown  # If you don't have the wasm target installed
cargo build --target wasm32-unknown-unknown --release
```

2. Start a server to host the compiled .wasm file:

```bash
cd target/wasm32-unknown-unknown/release
python3 -m http.server 8001
```

3. In another terminal, send a request to execute it:

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "http://localhost:8001/hoya_wasm_test.wasm"}'
```

You should receive a response indicating that the WebAssembly module was executed and see the log messages in the server output.

## License

MIT License

Copyright (c) 2025 Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
