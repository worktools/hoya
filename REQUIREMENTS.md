# Project Requirements: Serverless Dynamic Script Execution Engine

## 1. Overview

The project aims to develop a serverless execution engine capable of dynamically running scripts (initially JavaScript and WebAssembly) fetched from remote URLs. This engine will provide a secure and sandboxed environment for code execution, with capabilities for capturing standard output/error and injecting a limited set of host functionalities.

## 2. Core Functional Requirements

### 2.1. Code Retrieval and Handling

    - **FR2.1.1:** The engine must be able to download executable code (JavaScript files, WebAssembly modules) from user-provided HTTP/HTTPS URLs.
    - **FR2.1.2:** The engine should attempt to auto-detect the code type (e.g., `.js`, `.wasm`) based on the URL or content if possible. If auto-detection fails, it may rely on user-provided hints or default to a primary type.
    - **FR2.1.3:** The engine must handle potential download errors gracefully (e.g., network issues, invalid URLs, non-existent files).

### 2.2. JavaScript Execution Environment

    - **FR2.2.1:** Provide a sandboxed JavaScript runtime environment (e.g., using QuickJS or a similar lightweight engine).
    - **FR2.2.2:** Capture `stdout` (e.g., from `console.log()`) and `stderr` (e.g., from `console.error()`) produced during script execution.
    - **FR2.2.3:** Inject a predefined set of host APIs into the JavaScript global scope:
        - `app_log(level, message)`: For structured logging from the script.
        - `get_unixtime()`: To retrieve the current Unix timestamp.
        - `fetch(options)`: A limited capability to make outbound HTTP/HTTPS requests from the script. Security and resource limits must be considered.
    - **FR2.2.4:** Return the final evaluated expression or a designated return value from the JavaScript execution.
    - **FR2.2.5:** Handle JavaScript execution errors (runtime errors, syntax errors) and report them.

### 2.3. WebAssembly (Wasm) Execution Environment

    - **FR2.3.1:** Provide a sandboxed WebAssembly runtime environment.
    - **FR2.3.2:** Capture `stdout` and `stderr` streams from the Wasm module execution. This might involve specific Wasm interface (e.g., WASI) or custom host function implementations.
    - **FR2.3.3:** Inject a predefined set of host functions callable from Wasm:
        - `app_log(level_ptr, level_len, msg_ptr, msg_len)`: For structured logging.
        - `get_unixtime()`: To retrieve the current Unix timestamp.
        - `fetch(options_ptr, options_len, resp_buf_ptr, resp_buf_max_len)`: A limited capability for outbound HTTP/HTTPS requests.
        - `capture_stdout(ptr, len)`: To write to the host-captured standard output.
        - `capture_stderr(ptr, len)`: To write to the host-captured standard error.
    - **FR2.3.4:** Handle Wasm execution errors (e.g., instantiation errors, runtime traps) and report them.

### 2.4. API and Output

    - **FR2.4.1:** Expose a primary API endpoint (e.g., `/execute`) that accepts a request (e.g., JSON payload) specifying the URL of the code to be executed.
    - **FR2.4.2:** The API response should include:
        - Execution status (e.g., "success", "error").
        - The script's primary output/return value (if any).
        - Captured `stdout` content.
        - Captured `stderr` content.
        - Any execution errors encountered.
        - Metadata:
            - Execution time.
            - Detected code type.
            - Timestamp of execution.
            - Size of the downloaded resource.

## 3. Non-Functional Requirements

### 3.1. Security

    - **NFR3.1.1:** All remote code execution must occur in a strictly sandboxed environment to prevent unauthorized access to the host system or network.
    - **NFR3.1.2:** Limits on resource usage (CPU, memory, execution time, network bandwidth for `fetch`) must be enforced to prevent abuse.
    - **NFR3.1.3:** The `fetch` API provided to scripts must have configurable restrictions (e.g., allowed domains, request size, response size).

### 3.2. Performance

    - **NFR3.2.1:** The engine should aim for low-latency script execution.
    - **NFR3.2.2:** Efficient resource utilization, especially when handling multiple concurrent requests (if applicable in the future).

### 3.3. Scalability

    - **NFR3.3.1:** The architecture should be designed with serverless principles in mind, allowing for potential scaling to handle varying loads.

### 3.4. Error Handling and Logging

    - **NFR3.4.1:** Robust error handling for all internal operations and script executions.
    - **NFR3.4.2:** Comprehensive logging on the server-side for diagnostics and monitoring.

## 4. Target Functionality (Goals)

    - **G1:** A reliable and secure platform for executing untrusted JavaScript and WebAssembly code retrieved from remote sources.
    - **G2:** A simple and clear API for developers to submit code for execution and receive results.
    - **G3:** Provide essential I/O capabilities (stdout/stderr capturing) and limited host interactions (logging, time, basic HTTP client) to the executed scripts.
    - **G4:** Lay the foundation for a flexible serverless execution environment that could potentially be extended to support other languages or runtimes in the future.
    - **G5:** Ensure that the execution environment is lightweight and fast, suitable for serverless function-like use cases.

## 5. Out of Scope (Initially)

    - **OOS1:** Persistent storage for scripts or execution results beyond the immediate request-response cycle.
    - **OOS2:** Complex orchestration or workflow management of multiple script executions.
    - **OOS3:** User authentication or authorization for accessing the execution API (can be layered on top if needed).
    - **OOS4:** Support for languages other than JavaScript and WebAssembly in the initial version.
    - **OOS5:** Interactive debugging capabilities for the remote scripts.
