# Hoya execution v1 (preview)

Hoya is independently deployed. The default engine mode exposes only health,
readiness/capability discovery and the authenticated execution endpoint. Set
`HOYA_MODE=legacy-demo` explicitly to run the old UI and unversioned endpoints;
that development mode retains its old limitations and is not the Hosta v1 path.

```sh
cargo build --locked
HOYA_AUTH_TOKEN=local-example PORT=3000 target/debug/hoya
curl -s http://127.0.0.1:3000/v1/capabilities
curl -s -H 'Authorization: Bearer local-example' -H 'Content-Type: application/json' \
  --data-binary @protocol/v1/echo.request.json http://127.0.0.1:3000/v1/executions
```

Engine mode refuses to start without a token and binds to `127.0.0.1` by
default. `HOYA_BIND` controls the bind address for container networking.
The execution token is not inherited by worker processes. `/v1/capabilities`
is public and advertises the supported protocol, ABI and guest limits.

## Request and response

Schemas and a JS request fixture live in `protocol/v1/`. Every request includes
`protocolVersion: "1"`, `runId`, `runtime`, `code`, `artifactSha256`, and JSON
`input`, plus optional JSON `datasource` (defaults to null). The hash is SHA-256 of UTF-8 JS source or **decoded WASM bytes**, not
base64 text. No remote code URL is accepted. Limits may be omitted (defaults
below); if supplied, supply all four fields. Unknown request fields are rejected.

Responses echo `runId` and `artifactSha256` and contain `status`, JSON `result`,
structured `logs`, `metrics.durationMs`, and `error`. Successful results have
`error: null`; failure responses have `result: null` and a stable error code,
message and `retryable` flag. IDs/hashes identify the submitted artifact;
clients must verify them before storing a result.

HTTP 400 rejects invalid requests/hashes/unsupported capabilities; 401 rejects
credentials; 429 reports `OVERLOADED`. Once admitted, guest failure and timeout
return HTTP 200 with a non-success execution status. CLI clients must inspect
that status, not just the HTTP status. Malformed JSON and authentication errors
cannot always provide execution correlation and return an error-only envelope.

Core error codes include `ARTIFACT_HASH_MISMATCH`, `UNSUPPORTED_PROTOCOL`,
`UNSUPPORTED_CAPABILITY`, `INVALID_LIMITS`, `OVERLOADED`, `USER_CODE_ERROR`,
`PROMISE_UNSETTLED`, `LOG_LIMIT`, `RESULT_LIMIT`, `INVALID_WASM`,
`WASM_EXECUTION_ERROR`, `FUEL_EXHAUSTED`, `EXECUTION_TIMEOUT`, and `WORKER_FAILED`.

## JavaScript

Define `function main(input, ctx)` or `async function main(input, ctx)` in a
script, without ESM imports/exports. Return a JSON-serializable value. `ctx`
provides `log(level, message, fields?)` and `now()` in milliseconds. Inputs
are parsed as values; neither input nor datasource is interpolated into code.
`ctx.datasource` receives the provided JSON snapshot as a fresh guest value.
The v1 API does not expose environment, filesystem,
Node builtins, console, timers or fetch. Promises must settle through bounded
microtask execution; unsupported pending promises fail explicitly.

## WASM: hoya-json-v1

Export linear `memory` and **`hoya_main() -> i32`**. The returned nonnegative
pointer addresses a NUL-terminated UTF-8 JSON value. A dedicated symbol avoids
toolchains treating `main` as the C argc/argv entrypoint. Missing exports,
invalid pointers/JSON, and missing termination are failures.

Allowed optional imports from `env`:

- `get_input(ptr: i32, capacity: i32) -> i32`: capacity 0 queries required byte
  length; otherwise copies all JSON bytes, returning length. An undersized
  buffer traps rather than silently truncating. Input excludes a trailing NUL.
- `get_datasource(ptr: i32, capacity: i32) -> i32`: same copy/query convention
  as get_input, for the optional JSON snapshot.
- `log(ptr: i32, length: i32)`: UTF-8 message, emitted as a structured info log.
- `now() -> i64`: current Unix time in milliseconds.

The guest owns its memory. The host reads the result before destroying the
entire invocation; there is no shared state or cross-invocation allocation.
No WASI or `fetch` imports are linked. Rust example:

```sh
rustup target add wasm32-unknown-unknown --toolchain stable
rustc +stable --edition=2021 --target wasm32-unknown-unknown --crate-type cdylib \
  -O examples/v1/echo.rs -o /tmp/hoya-echo.wasm
```

MoonBit integration is not yet verified and is not advertised as ready.

## Limits and current boundary

Default limits: 3000ms total worker lifetime (including initialization and
compilation), 32MiB guest heap/linear memory, 64KiB serialized logs, 1MiB result.
Request maximum is 2MiB; JS source maximum 128KiB, decoded WASM maximum 1MiB.
Configurable ranges are in the request schema. WASM additionally has a fixed
10-million fuel budget and bounded tables/stack. JS uses interrupt, heap and
stack limits plus at most 10,000 Promise drain iterations.

Each invocation launches a fresh worker with an empty environment. Four global
execution slots use immediate rejection; there is no unbounded waiting queue.
Timeout drops/kills the worker, covering compilation as well as guest code.
Log append and result-read bounds are checked; oversized JS serialization may
allocate within its guest heap before rejection. There is no compiled module
cache yet. `memoryMb` does **not** cap the total process RSS or compiler memory.
Use container CPU/RSS limits for the service; process separation alone is not
a complete multi-tenant isolation boundary. Dedicated container deployment,
OS limits, client-disconnect cancellation tests and supported egress policies
remain tracked in #7–#9.

Network is unavailable, even if requested: nonempty network capabilities are
rejected. Do not treat this as an implemented allowlist or SSRF-safe fetch API.

## Verification and compatibility

```sh
cargo test --locked
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked
python3 tests/protocol_http.py
```

Unit tests cover value binding, shared JSON semantics, missing entrypoints,
invalid results, unsettled promises, logs/results bounds and absent network
APIs. Real-process HTTP tests cover authentication, hidden legacy paths,
correlation, hash/capability rejection, worker timeout recovery and overload.
The Hosta repository runs its CLI-to-real-Hoya JS/Rust WASM smoke separately.

This is a new preview protocol. It does not silently emulate the legacy WASM
`main` ABI. Clients pin an engine revision and check discovery; future breaking
changes require a new protocol/ABI version. Release packaging and the supported
version matrix remain tracked in #11.
