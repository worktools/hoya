# Hoya

A serverless execution engine for dynamically running JavaScript and WebAssembly scripts fetched from remote URLs. Hoya provides a sandboxed environment, captures standard I/O, and injects limited host functionalities.

Refer to [REQUIREMENTS.md](REQUIREMENTS.md) for detailed project requirements and functional specifications.

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

### Example

```bash
curl -X POST http://localhost:3000/execute \
  -H "Content-Type: application/json" \
  -d '{"url": "https://example.com/your-script.js"}'
```

## Testing

Refer to the test scripts (`test_hoya.sh`, `test_stdout_stderr.sh`) and the `examples/` directory for testing various features.

## License

MIT License (see LICENSE file for details).
