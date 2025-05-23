# Hoya HTTP API Protocol

## Overview

Hoya is a service that allows execution of JavaScript and WebAssembly code from remote URLs. This document describes the HTTP API specifications for interacting with the Hoya service.

## Base URL

```
http://127.0.0.1:3000
```

The service runs on localhost port 3000 by default.

## API Endpoints

### Execute Code

Executes JavaScript or WebAssembly code fetched from a remote URL.

**Endpoint:** `/execute`

**Method:** POST

**Request Format:**

```json
{
  "url": "string" // URL pointing to a .js or .wasm file
}
```

**Response Format:**

```json
{
  "status": "string", // "success" or "error"
  "output": "string", // Present if execution was successful
  "stdout": "string", // Standard output content captured during execution
  "stderr": "string", // Standard error content captured during execution
  "error": {
    // Present if execution failed, null otherwise
    "code": "string", // Error code
    "message": "string", // Error message
    "details": "object" // Optional additional error details
  },
  "metadata": {
    "executionTime": "number", // Execution time in milliseconds
    "codeType": "string", // "javascript" or "webassembly"
    "timestamp": "string", // ISO timestamp of when execution completed
    "resourceSize": "number" // Size of the executed code in bytes
  }
}
```

**Status Codes:**

- 200 OK: Request processed successfully
- 400 Bad Request: Invalid input
- 500 Internal Server Error: Error during code execution
- 502 Bad Gateway: Error when fetching the resource

**Examples:**

_Request (JavaScript):_

```json
{
  "url": "https://example.com/script.js"
}
```

_Response (JavaScript):_

```json
{
  "status": "success",
  "output": "Result from JavaScript execution",
  "stdout": "Log message from console.log",
  "stderr": "Error message from console.error",
  "error": null,
  "metadata": {
    "executionTime": 15,
    "codeType": "javascript",
    "timestamp": "2025-05-23T14:32:45Z",
    "resourceSize": 1024
  }
}
```

_Request (WebAssembly):_

```json
{
  "url": "https://example.com/module.wasm"
}
```

_Response (WebAssembly):_

```json
{
  "status": "success",
  "output": "WASM module executed (_start)",
  "stdout": "Output from println!() in Rust WASM",
  "stderr": "Error output from eprintln!() in Rust WASM",
  "error": null,
  "metadata": {
    "executionTime": 8,
    "codeType": "webassembly",
    "timestamp": "2025-05-23T14:35:12Z",
    "resourceSize": 2048
  }
}
```

_Error Response Example:_

```json
{
  "status": "error",
  "output": null,
  "stdout": "Log output before error occurred",
  "stderr": "Error traces from runtime",
  "error": {
    "code": "EXECUTION_FAILED",
    "message": "Failed to execute JavaScript code",
    "details": {
      "line": 12,
      "column": 5,
      "sourceSnippet": "const x = y.undefined.property;"
    }
  },
  "metadata": {
    "executionTime": 3,
    "codeType": "javascript",
    "timestamp": "2025-05-23T14:37:30Z",
    "resourceSize": 512
  }
}
```

## Available Runtime Functions

### JavaScript Runtime

The following functions are available in the JavaScript runtime:

1. **app_log(level, message)**

   - Description: Logs a message with a specified level
   - Parameters:
     - `level`: Log level (e.g., "INFO", "ERROR")
     - `message`: Message to log
   - Example: `app_log("INFO", "Hello, world!")`

2. **get_unixtime()**

   - Description: Returns the current Unix timestamp (seconds since Unix epoch)
   - Returns: Number (timestamp)
   - Example: `const time = get_unixtime()`

3. **fetch(options)**
   - Description: Performs HTTP requests (Note: Currently throws "not fully implemented" error)
   - Parameters:
     - `options`: Object containing fetch options
   - Example (intended usage):
     ```javascript
     fetch({
       url: "https://example.com/api",
       method: "GET",
       headers: { "Content-Type": "application/json" },
       body: JSON.stringify({ key: "value" }),
     });
     ```

### WebAssembly Runtime

The following functions are imported into the WebAssembly runtime from the "env" module:

1. **app_log(level_ptr, level_len, msg_ptr, msg_len)**

   - Description: Logs a message with a specified level
   - Parameters:
     - Memory pointers to level string and message string
     - Lengths of level string and message string
   - Example (conceptual): See WASM examples for memory handling

2. **get_unixtime()**

   - Description: Returns the current Unix timestamp (seconds since Unix epoch)
   - Returns: u64 (timestamp)

3. **fetch(options_ptr, options_len, resp_buf_ptr, resp_buf_max_len)**
   - Description: Performs HTTP requests
   - Parameters:
     - Memory pointer and length for options JSON
     - Memory pointer and max length for response buffer
   - Returns: Response length (or negative value if buffer is too small)
   - Options JSON format:
     ```json
     {
       "url": "string",
       "method": "string",
       "headers": { "header1": "value1", ... },
       "body": "string" (optional)
     }
     ```
   - Response JSON format:
     ```json
     {
       "status": 200,
       "headers": { "header1": "value1", ... },
       "body": "string"
     }
     ```

## Error Handling

The service returns appropriate HTTP status codes and error messages in the response body. Client applications should handle these errors gracefully.

## Output Capturing

The service captures all output written to the standard output (stdout) and standard error (stderr) during code execution. This includes:

- JavaScript: Output from `console.log()` and `console.error()`
- WebAssembly (Rust): Output from functions calling the imported `capture_stdout` and `capture_stderr` functions

These captured outputs are returned in the response JSON as separate fields:

```json
{
  "stdout": "All standard output content",
  "stderr": "All standard error content"
}
```

### JavaScript Output Example

When executing JavaScript code, anything written to `console.log()` will be captured in the `stdout` field, and anything written to `console.error()` will be captured in the `stderr` field:

```javascript
console.log("This goes to stdout");
console.error("This goes to stderr");
```

### WebAssembly Output Example

For WebAssembly modules (particularly those compiled from Rust), the following interface is available for writing to stdout and stderr:

```rust
// Imported from host environment
extern "C" {
    fn capture_stdout(ptr: *const u8, len: usize);
    fn capture_stderr(ptr: *const u8, len: usize);
}

// Usage
unsafe {
    let message = "This is a message";
    capture_stdout(message.as_ptr(), message.len());
}
```

## Security Considerations

- The service executes code from remote URLs, which presents potential security risks
- No authentication or authorization mechanisms are currently implemented
- Consider running the service in a sandboxed environment for production use
- URL validation and input sanitization should be implemented by clients

## Limitations

- The service only supports JavaScript (.js) and WebAssembly (.wasm) files
- JavaScript fetch implementation is currently not fully functional
- WebAssembly modules must export a "memory" object
